import AppKit
import CoreGraphics

final class PassivePanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
    init(interactive: Bool) {
        super.init(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true; level = .floating; hidesOnDeactivate = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        isOpaque = false; backgroundColor = .clear; hasShadow = false
        ignoresMouseEvents = !interactive; isReleasedWhenClosed = false
        // Intentionally no sharingType=.none: Apple's current documentation
        // calls it a legacy constant macOS no longer uses. ready is unsupported.
    }
}

final class StatusView: NSView {
    let brand = NSTextField(labelWithString: "")
    let status = NSTextField(labelWithString: "Waiting for Host")
    init(style: Style) {
        super.init(frame: .zero)
        wantsLayer = true; layer?.backgroundColor = NSColor(calibratedWhite: 0.075, alpha: 0.96).cgColor
        layer?.cornerRadius = 12; layer?.borderColor = NSColor(calibratedWhite: 0.35, alpha: 1).cgColor; layer?.borderWidth = 1
        brand.stringValue = style.label; brand.textColor = .white; brand.font = .systemFont(ofSize: 12, weight: .semibold)
        status.textColor = NSColor(calibratedWhite: 0.84, alpha: 1); status.font = .systemFont(ofSize: 11)
        for label in [brand, status] { label.lineBreakMode = .byTruncatingTail; label.maximumNumberOfLines = 1; addSubview(label) }
        let dot = NSView(frame: NSRect(x: 12, y: 20, width: 8, height: 8))
        dot.wantsLayer = true; dot.layer?.cornerRadius = 4; dot.layer?.backgroundColor = style.color.cgColor
        dot.layer?.borderWidth = 1; dot.layer?.borderColor = NSColor.white.withAlphaComponent(0.7).cgColor; addSubview(dot)
        brand.setAccessibilityLabel(style.label)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }
    override func layout() {
        super.layout()
        brand.frame = NSRect(x: 30, y: 26, width: bounds.width - 42, height: 17)
        status.frame = NSRect(x: 30, y: 7, width: bounds.width - 42, height: 17)
    }
}
final class RingView: NSView {
    let color: NSColor
    var kind = "move"
    init(color: NSColor) { self.color = color; super.init(frame: .zero) }
    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }
    override func draw(_ dirtyRect: NSRect) {
        let rect = bounds.insetBy(dx: 5, dy: 5)
        let outline = NSBezierPath(ovalIn: rect); outline.lineWidth = kind == "click" ? 5 : 3
        NSColor.white.withAlphaComponent(0.88).setStroke(); outline.stroke()
        let ring = NSBezierPath(ovalIn: rect.insetBy(dx: 1, dy: 1)); ring.lineWidth = kind == "click" ? 3 : 2
        color.setStroke(); ring.stroke()
    }
}
extension Style {
    var color: NSColor { NSColor(srgbRed: Double((rgb >> 16) & 255) / 255, green: Double((rgb >> 8) & 255) / 255, blue: Double(rgb & 255) / 255, alpha: 1) }
}

final class FeedbackWindows: NSObject, NSWindowDelegate {
    let bar = PassivePanel(interactive: false)
    let stopPanel = PassivePanel(interactive: true)
    let ring = PassivePanel(interactive: false)
    let stopButton = NSButton(title: "Stop", target: nil, action: nil)
    private let status: StatusView
    private let ringView: RingView
    private var quitFromHost = false
    private var fallbackScreen: NSScreen
    var onStop: (() -> Void)?
    init(style: Style, screen: NSScreen) {
        fallbackScreen = screen; status = StatusView(style: style); ringView = RingView(color: style.color)
        super.init()
        bar.contentView = status; ring.contentView = ringView
        stopPanel.contentView = NSView(frame: NSRect(x: 0, y: 0, width: 88, height: 48))
        stopButton.frame = NSRect(x: 0, y: 0, width: 88, height: 48)
        stopButton.bezelStyle = .rounded; stopButton.font = .systemFont(ofSize: 14, weight: .semibold)
        stopButton.isEnabled = false; stopButton.target = self; stopButton.action = #selector(stop)
        stopButton.setAccessibilityLabel("Stop AI control session")
        stopButton.toolTip = "Request cancellation and input cleanup from Host"
        stopPanel.contentView?.addSubview(stopButton)
        bar.delegate = self; stopPanel.delegate = self
        place(on: screen); bar.orderFrontRegardless(); stopPanel.orderFrontRegardless()
    }
    @objc private func stop() { onStop?() }
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        if quitFromHost { return true }
        onStop?(); return false // Keep truth visible until Host confirms or stdin EOF.
    }
    func shutdown() { quitFromHost = true; ring.close(); bar.close(); stopPanel.close() }
    private func place(on screen: NSScreen) {
        let f = screen.visibleFrame
        let total = min(448.0, max(180.0, f.width - 32))
        bar.setFrame(NSRect(x: f.minX + 16, y: f.maxY - 64, width: total - 96, height: 48), display: true)
        stopPanel.setFrame(NSRect(x: f.minX + 16 + total - 88, y: f.maxY - 64, width: 88, height: 48), display: true)
    }
    func render(_ state: FeedbackState, showPointer: Bool = true) -> String? {
        status.status.stringValue = state.status; status.status.setAccessibilityValue(state.status)
        stopButton.isEnabled = state.canStop
        var screen: NSScreen?, geometryValid = false
        if let s = state.snapshot?.surface {
            screen = NSScreen.screens.first { candidate in
                guard let n = candidate.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else { return false }
                let id = n.uint32Value, b = CGDisplayBounds(id)
                return s.id == "macos:\(id)" && abs(b.minX - s.x) < 0.5 && abs(b.minY - s.y) < 0.5 && abs(b.width - s.width) < 0.5 && abs(b.height - s.height) < 0.5
            }
            geometryValid = screen != nil
        }
        if let screen { fallbackScreen = screen }
        else if !NSScreen.screens.contains(fallbackScreen), let available = NSScreen.screens.first { fallbackScreen = available }
        place(on: fallbackScreen)
        let pointer = state.snapshot?.pointer
        if geometryValid, showPointer, let p = pointer, let s = state.snapshot?.surface, let screen,
           state.snapshot?.session != nil, state.snapshot?.phase != "closed", state.snapshot?.phase != "faulted", state.stopRequested == nil,
           let point = Geometry.appKitPointer(p, surface: s, frameLeft: screen.frame.minX, frameTop: screen.frame.maxY) {
            ringView.kind = p.kind; ringView.needsDisplay = true
            ring.setFrame(NSRect(x: point.x - 22, y: point.y - 22, width: 44, height: 44), display: true)
            ring.orderFrontRegardless()
        } else { ring.orderOut(nil) }
        if state.snapshot?.surface != nil && !geometryValid {
            status.status.stringValue = "Display unavailable · " + state.status
            return "surface_geometry_unavailable"
        }
        return nil
    }
    func expirePointer() { ring.orderOut(nil) }
    var initialized: Bool { bar.windowNumber > 0 && stopPanel.windowNumber > 0 && bar.isVisible && stopPanel.isVisible }
}
