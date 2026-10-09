//
//  Gesture suites: AppKit views + per-case controller (macOS).
//  All case state and judging lives in GestureCases.swift (pure); this file
//  only wires real AppKit events into the pure recorder/judges and renders
//  agent-facing UI. No global input capture — only this window's own views.
//
//  Conventions preserved from main.swift: main-display placement, explicit
//  clipping, acceptsFirstMouse on clickable canvases, no repeated
//  front-raising without --coordinator-raise.
//

import AppKit

// MARK: - Event sink

protocol GestureEventSink: AnyObject {
    func record(_ e: GestureEvent)
}

// MARK: - Multiclick canvas

final class GestureClickCanvas: NSView {
    var zones: [GestureZone] = [] { didSet { needsDisplay = true } }
    var sink: GestureEventSink?
    private var trail: [(NSPoint, Bool)] = [] // (point, wasRightButton) live feedback
    private let heldTracking: Bool // drag mode: track moves between down/up

    private(set) var buttonHeld = false

    init(heldTracking: Bool) {
        self.heldTracking = heldTracking
        super.init(frame: .zero)
        clipsToBounds = true
    }
    required init?(coder: NSCoder) { fatalError("not used") }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override var isFlipped: Bool { true }

    private func area(for pt: NSPoint) -> String {
        for z in zones {
            if z.name == "TARGET" || z.name == "START" || z.name == "END" || z.name == "WAYPOINT" {
                let r = NSRect(x: z.x, y: z.y, width: z.w, height: z.h)
                if r.contains(pt) { return z.name == "TARGET" ? "target" : "canvas" }
            }
        }
        return "canvas"
    }

    private func areaName(for pt: NSPoint) -> String {
        for z in zones {
            let r = NSRect(x: z.x, y: z.y, width: z.w, height: z.h)
            if r.contains(pt) {
                switch z.name {
                case "TARGET": return "target"
                case "zoneA": return "zoneA"
                case "zoneB": return "zoneB"
                default: return "canvas"
                }
            }
        }
        return "outside"
    }

    private func emit(_ kind: String, _ event: NSEvent, area: String, count: Int, dbl: Bool) {
        let pt = convert(event.locationInWindow, from: nil)
        var raw = GestureEvent(kind: kind, button: event.type == .rightMouseDown || event.type == .rightMouseUp || event.type == .rightMouseDragged ? "right" : "left",
                                  nativeCount: count, doubleClickMsg: dbl, area: area,
                                  x: pt.x, y: pt.y, dx: 0, dy: 0, held: heldTracking && buttonHeld, tMs: 0)
        raw.nativeTimestamp = event.timestamp; raw.nativeType = Int(event.type.rawValue)
        sink?.record(raw)
        trail.append((pt, raw.button == "right"))
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        let area = areaName(for: convert(event.locationInWindow, from: nil))
        if heldTracking {
            buttonHeld = true
            emit("down", event, area: "canvas", count: event.clickCount, dbl: false)
        } else {
            emit("down", event, area: area, count: event.clickCount, dbl: false)
        }
    }

    override func rightMouseDown(with event: NSEvent) {
        let area = areaName(for: convert(event.locationInWindow, from: nil))
        emit("down", event, area: area, count: event.clickCount, dbl: false)
    }

    override func mouseDragged(with event: NSEvent) {
        guard heldTracking, buttonHeld else { return }
        emit("move", event, area: "canvas", count: event.clickCount, dbl: false)
    }

    override func rightMouseDragged(with event: NSEvent) {
        guard heldTracking, buttonHeld else { return }
        emit("move", event, area: "canvas", count: event.clickCount, dbl: false)
    }

    override func mouseUp(with event: NSEvent) {
        if heldTracking {
            buttonHeld = false
            emit("up", event, area: "canvas", count: event.clickCount, dbl: false)
        } else {
            emit("up", event, area: areaName(for: convert(event.locationInWindow, from: nil)), count: event.clickCount, dbl: false)
        }
    }

    override func rightMouseUp(with event: NSEvent) {
        if heldTracking { buttonHeld = false }
        emit("up", event, area: areaName(for: convert(event.locationInWindow, from: nil)), count: event.clickCount, dbl: false)
    }

    func reset() {
        trail.removeAll()
        buttonHeld = false
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.white.setFill()
        bounds.intersection(dirtyRect).fill()
        if let ctx = NSGraphicsContext.current {
            ctx.cgContext.saveGState()
            ctx.cgContext.clip(to: bounds)
            for z in zones {
                let r = NSRect(x: z.x, y: z.y, width: z.w, height: z.h)
                switch z.name {
                case "TARGET", "START", "END", "WAYPOINT":
                    NSColor(calibratedRed: 0.85, green: 0.92, blue: 1.0, alpha: 1).setFill()
                default:
                    NSColor(calibratedRed: 0.92, green: 0.96, blue: 0.92, alpha: 1).setFill()
                }
                r.fill()
                NSColor.gray.setStroke()
                let p = NSBezierPath(rect: r.insetBy(dx: 1, dy: 1))
                p.lineWidth = 2
                p.stroke()
                let label = z.name as NSString
                label.draw(at: NSPoint(x: r.minX + 6, y: r.maxY - 20),
                           withAttributes: [.font: NSFont.boldSystemFont(ofSize: 14), .foregroundColor: NSColor.darkGray])
            }
            // live held-trail feedback (drag suites)
            if trail.count > 1 {
                let p = NSBezierPath()
                p.move(to: trail[0].0)
                for pt in trail.dropFirst() { p.line(to: pt.0) }
                NSColor(calibratedRed: 0.9, green: 0.4, blue: 0.1, alpha: 0.6).setStroke()
                p.lineWidth = 3
                p.stroke()
            }
            ctx.cgContext.restoreGState()
        }
    }
}

// Native scroll receiver. Original NSEvent fields remain separate from content offsets.
final class GestureWheelScrollView: NSScrollView {
    weak var sink: GestureEventSink?
    var panelName = "panelA"
    var onOffsetChange: (() -> Void)?
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override func scrollWheel(with event: NSEvent) {
        var raw = GestureEvent(kind: "wheel", button: "other", nativeCount: 0, doubleClickMsg: false,
            area: panelName, x: convert(event.locationInWindow, from: nil).x,
            y: convert(event.locationInWindow, from: nil).y,
            dx: Double(event.scrollingDeltaX), dy: Double(event.scrollingDeltaY), held: false, tMs: 0)
        raw.nativeTimestamp = event.timestamp; raw.nativeType = Int(event.type.rawValue)
        raw.legacyDx = Double(event.deltaX); raw.legacyDy = Double(event.deltaY)
        raw.precise = event.hasPreciseScrollingDeltas; raw.inverted = event.isDirectionInvertedFromDevice
        raw.phase = event.phase.rawValue; raw.momentumPhase = event.momentumPhase.rawValue
        // Explicit fixture convention: content positive right/down = -NSEvent scrollingDelta.
        // This view owns the actual scroll; never apply both this and super's default scrolling.
        let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 30
        let old = contentView.bounds.origin
        let maxX = max(0, (documentView?.frame.width ?? 0) - contentView.bounds.width)
        let maxY = max(0, (documentView?.frame.height ?? 0) - contentView.bounds.height)
        contentView.scroll(to: NSPoint(x: min(maxX, max(0, old.x - event.scrollingDeltaX * scale)),
                                       y: min(maxY, max(0, old.y - event.scrollingDeltaY * scale))))
        reflectScrolledClipView(contentView)
        sink?.record(raw)
        onOffsetChange?()
    }
}

final class GestureGridView: NSView {
    var panelName = "panelA"
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        NSColor.white.setFill(); bounds.intersection(dirtyRect).fill()
        let attrs: [NSAttributedString.Key: Any] = [.font: NSFont.monospacedSystemFont(ofSize: 18, weight: .medium), .foregroundColor: NSColor.darkGray]
        for row in 0..<40 {
            for col in 0..<4 {
                let rect = NSRect(x: col * 300 + 10, y: row * 36 + 6, width: 290, height: 28)
                if rect.intersects(dirtyRect) { ("\(panelName) R\(row) C\(col)" as NSString).draw(in: rect, withAttributes: attrs) }
            }
        }
    }
}

// The standard NSTextView owns selection. A LOCAL, own-window event monitor in
// GestureController records events even during AppKit's nested selection tracking.
final class GestureSentenceView: NSTextView {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    var selectedText: String {
        let range = selectedRange()
        let ns = string as NSString
        return NSMaxRange(range) <= ns.length ? ns.substring(with: range) : ""
    }
}
