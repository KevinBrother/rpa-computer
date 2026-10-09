//
//  Computer Use Acceptance Fixture (macOS, AppKit)
//  A small native visual target range for screenshot-only computer-use agents.
//  No network, no file access except the optional coordinator-owned evidence file.
//
//  Usage: acceptance-fixture [--seed N] [--evidence-file PATH]
//                            [--suite legacy|baseline|punctuation|emoji|known-input]
//                            [--self-test] [--export-cases PATH]
//  Omitting --suite keeps the historical legacy single-sample behavior; legacy
//  results must never be merged with layered-suite results.
//

import AppKit
import CoreGraphics
import Foundation

// MARK: - Constants

let kNonceChars = Array("ABCDEFGHJKLMNPQRSTUVWXYZ23456789") // no ambiguous 0/O 1/I
let kWindowTitle = "Computer Use Acceptance"

// MARK: - CLI config

struct Config {
    var seed: UInt64? = nil
    var evidencePath: String? = nil
    var suite: Suite = .legacy
    var selfTest = false
    var exportPath: String? = nil
    var coordinatorRaise = false
    var gestureSuite: String? = nil

    static func fail(_ msg: String) -> Never {
        FileHandle.standardError.write(Data("error: \(msg)\n".utf8))
        exit(1)
    }

    static func parse() -> Config {
        var cfg = Config()
        let a = CommandLine.arguments
        var i = 1
        while i < a.count {
            let arg = a[i]
            if arg == "--seed", i + 1 < a.count {
                cfg.seed = UInt64(a[i + 1]); i += 2
            } else if arg == "--evidence-file", i + 1 < a.count {
                cfg.evidencePath = a[i + 1]; i += 2
            } else if arg.hasPrefix("--seed=") {
                cfg.seed = UInt64(String(arg.dropFirst(7))); i += 1
            } else if arg.hasPrefix("--evidence-file=") {
                cfg.evidencePath = String(arg.dropFirst(16)); i += 1
            } else if arg == "--self-test" {
                cfg.selfTest = true; i += 1
            } else if arg == "--suite" {
                guard i + 1 < a.count else { fail("--suite requires a value (legacy|baseline|punctuation|emoji|known-input|multiclick|drag|scroll)") }
                if GestureCatalog.isGesture(suite: a[i + 1]) {
                    cfg.gestureSuite = a[i + 1]; i += 2; continue
                }
                guard let s = Suite(rawValue: a[i + 1]) else {
                    fail("unknown suite '\(a[i + 1])' (valid: legacy|baseline|punctuation|emoji|known-input|multiclick|drag|scroll)")
                }
                cfg.gestureSuite = nil; cfg.suite = s; i += 2
            } else if arg.hasPrefix("--suite=") {
                let v = String(arg.dropFirst("--suite=".count))
                if GestureCatalog.isGesture(suite: v) {
                    cfg.gestureSuite = v; i += 1; continue
                }
                guard let s = Suite(rawValue: v) else {
                    fail("unknown suite '\(v)' (valid: legacy|baseline|punctuation|emoji|known-input|multiclick|drag|scroll)")
                }
                cfg.gestureSuite = nil; cfg.suite = s; i += 1
            } else if arg == "--export-cases", i + 1 < a.count {
                cfg.exportPath = a[i + 1]; i += 2
            } else if arg.hasPrefix("--export-cases=") {
                cfg.exportPath = String(arg.dropFirst("--export-cases=".count)); i += 1
            } else if arg == "--coordinator-raise" {
                cfg.coordinatorRaise = true; i += 1
            } else {
                Config.fail("unknown argument '\(arg)'")
            }
        }
        return cfg
    }
}

// MARK: - Deterministic RNG (SplitMix64)

struct SplitMix64 {
    var state: UInt64
    mutating func next() -> UInt64 {
        state &+= 0x9E3779B97F4A7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58476D1CE4E5B9
        z = (z ^ (z >> 27)) &* 0x94D049BB133111EB
        return z ^ (z >> 31)
    }
    mutating func below(_ n: Int) -> Int { Int(next() % UInt64(n)) }
}

extension Array {
    mutating func shuffle(using rng: inout SplitMix64) {
        guard count > 1 else { return }
        for i in stride(from: count - 1, through: 1, by: -1) {
            let j = rng.below(i + 1)
            swapAt(i, j)
        }
    }
    func shuffled(using rng: inout SplitMix64) -> [Element] {
        var copy = self
        copy.shuffle(using: &rng)
        return copy
    }
}

// MARK: - Evidence logger (coordinator-only JSONL oracle)

final class EvidenceLogger {
    private let path: String
    private let ts = ISO8601DateFormatter()
    private let runID = UUID().uuidString
    private let sessionID = UUID().uuidString

    init?(path: String?) {
        guard let p = path, !p.isEmpty else { return nil }
        self.path = p
        ts.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if !FileManager.default.fileExists(atPath: p) {
            FileManager.default.createFile(atPath: p, contents: nil)
        }
    }

    func log(_ type: String, _ fields: [String: Any] = [:]) {
        var obj: [String: Any] = ["type": type, "ts": ts.string(from: Date()), "run_id": runID, "session_id": sessionID]
        for (k, v) in fields { obj[k] = v }
        guard let data = try? JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys]),
              let line = String(data: data, encoding: .utf8),
              let handle = FileHandle(forWritingAtPath: path) else { return }
        handle.seekToEndOfFile()
        if let d = (line + "\n").data(using: .utf8) { handle.write(d) }
        handle.closeFile()
    }
}

// MARK: - Trial model

enum ShapeKind: String { case circle, square, triangle, diamond }

struct Item {
    let kind: ShapeKind
    let color: NSColor
    let colorName: String
    let isTarget: Bool
}

struct Palette {
    static let green  = NSColor(calibratedRed: 0.00, green: 0.72, blue: 0.12, alpha: 1)
    static let red    = NSColor(calibratedRed: 0.86, green: 0.16, blue: 0.16, alpha: 1)
    static let blue   = NSColor(calibratedRed: 0.12, green: 0.35, blue: 0.86, alpha: 1)
    static let yellow = NSColor(calibratedRed: 0.95, green: 0.78, blue: 0.05, alpha: 1)
}

// MARK: - Shapes canvas

final class ShapesView: NSView {
    var items: [Item] = [] { didSet { needsDisplay = true } }
    var onClick: ((Int) -> Void)?   // slot index

    // Test hooks (self-test only): record the actual fill rect and effective
    // clip bounds of the last draw() so an offscreen regression can assert
    // that painting never escapes this view's own bounds.
    var recordDrawing = false
    private(set) var lastFillRect: NSRect = .zero
    private(set) var lastDrawClipBounds: NSRect = .zero

    // Layer-backed rendering does not clip draw() to bounds; clip explicitly.
    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        clipsToBounds = true
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        clipsToBounds = true
    }

    private func rect(forSlot slot: Int) -> NSRect {
        let size: CGFloat = 110
        let cx = bounds.width * CGFloat(2 * slot + 1) / 8.0
        return NSRect(x: cx - size / 2, y: bounds.midY - size / 2, width: size, height: size)
    }

    private func path(for kind: ShapeKind, in r: NSRect) -> NSBezierPath {
        switch kind {
        case .circle: return NSBezierPath(ovalIn: r)
        case .square: return NSBezierPath(rect: r)
        case .triangle:
            let p = NSBezierPath()
            p.move(to: NSPoint(x: r.midX, y: r.maxY))
            p.line(to: NSPoint(x: r.maxX, y: r.minY))
            p.line(to: NSPoint(x: r.minX, y: r.minY))
            p.close()
            return p
        case .diamond:
            let p = NSBezierPath()
            p.move(to: NSPoint(x: r.midX, y: r.maxY))
            p.line(to: NSPoint(x: r.maxX, y: r.midY))
            p.line(to: NSPoint(x: r.midX, y: r.minY))
            p.line(to: NSPoint(x: r.minX, y: r.midY))
            p.close()
            return p
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        // Strictly limit painting to this view's own bounds. The header labels
        // are earlier siblings behind this view; an unclipped white fill was
        // suspected of painting over them, so every draw op is now clipped.
        let fillRect = bounds.intersection(dirtyRect)
        NSColor.white.setFill()
        fillRect.fill()
        if recordDrawing { lastFillRect = fillRect }

        if let ctx = NSGraphicsContext.current {
            ctx.cgContext.saveGState()
            ctx.cgContext.clip(to: bounds)
            if recordDrawing {
                lastDrawClipBounds = ctx.cgContext.boundingBoxOfClipPath
            }
            for (slot, item) in items.enumerated() {
                let p = path(for: item.kind, in: rect(forSlot: slot).insetBy(dx: 4, dy: 4))
                item.color.setFill()
                p.fill()
                NSColor.black.setStroke()
                p.lineWidth = 3
                p.stroke()
            }
            ctx.cgContext.restoreGState()
        }
    }

    // Click-through: like standard clickable controls, accept the first
    // mouse-down even when the window is inactive, instead of it only
    // activating the window.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        let pt = convert(event.locationInWindow, from: nil)
        for (slot, _) in items.enumerated() where rect(forSlot: slot).contains(pt) {
            onClick?(slot)
            return
        }
    }

    override var acceptsFirstResponder: Bool { true }
}

// MARK: - App delegate / controller

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private let config: Config
    private var rng: SplitMix64
    private let seedUsed: UInt64
    private let logger: EvidenceLogger?
    private let suite: Suite
    private let suiteCases: [FixtureCase]

    private var gestureController: GestureController?
    private var window: NSWindow!
    private var shapesView: ShapesView!
    private var trialLabel: NSTextField!
    private var nonceLabel: NSTextField!
    private var statusLabel: NSTextField!
    private var resultLabel: NSTextField!
    private var sampleLabel: NSTextField!
    private var textView: NSTextView!

    private var trialIndex = 0        // 1-based; in suite mode == case index
    private var currentCase: FixtureCase? = nil
    private var suiteFinished = false
    private var items: [Item] = []
    private var targetSlot = 0
    private var nonce = ""

    // Strong reference so the SIGUSR1 DispatchSource outlives install.
    private var coordinatorRaiseSource: DispatchSourceSignal? = nil

    init(config: Config) {
        self.config = config
        self.seedUsed = config.seed ?? UInt64.random(in: UInt64.min ... UInt64.max)
        self.rng = SplitMix64(state: seedUsed)
        self.logger = EvidenceLogger(path: config.evidencePath)
        self.suite = config.suite
        self.suiteCases = CaseCatalog.cases(for: config.suite)
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        logger?.log("session", ["seed": String(seedUsed), "platform": "macos", "suite": config.gestureSuite ?? suite.rawValue])
        buildUI()
        if config.gestureSuite == nil { startTrial() }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        if config.coordinatorRaise { installCoordinatorRaise() }
    }

    // Coordinator-only environment preparation: the opaque backdrop can sit
    // above this window even after activation, so the coordinator sends
    // SIGUSR1 to make the fixture order itself front. Touches ONLY this
    // app's own single window — never other apps, no AX/AppleScript, no
    // input injection, no always-floating level change.
    private func installCoordinatorRaise() {
        signal(SIGUSR1, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: SIGUSR1, queue: .main)
        source.setEventHandler { [weak self] in
            guard let self = self, let w = self.window else { return }
            w.makeKeyAndOrderFront(nil)
            w.orderFrontRegardless()
            NSApp.activate(ignoringOtherApps: true)
            self.logger?.log("coordinator_raise", ["pid": Int(getpid()), "raised": true,
                                                   "key": w.isKeyWindow, "active": NSApp.isActive])
        }
        source.resume()
        coordinatorRaiseSource = source
        print("COORDINATOR_RAISE_READY=1 PID=\(getpid())")
        fflush(stdout)
    }

    // Focus-state evidence for the coordinator: lets the next run correlate
    // first-click behavior with actual key/active state. Own window only,
    // boolean metadata, no answers.
    func windowDidBecomeKey(_ notification: Notification) {
        logger?.log("window_key", ["key": true, "active": NSApp.isActive])
    }

    func windowDidResignKey(_ notification: Notification) {
        logger?.log("window_key", ["key": false, "active": NSApp.isActive])
    }

    func windowWillClose(_ notification: Notification) { NSApp.terminate(nil) }

    // MARK: UI construction

    // Center the window on the screen backing CGMainDisplayID — the Host
    // screenshots the primary display. window.center() previously landed on
    // a secondary display (probe: frame x=2430 on display 1, main=2).
    // Fails loudly (exit 1) instead of starting invisibly on another display.
    private func placeWindowOnMainDisplay() {
        let mainID = Int(CGMainDisplayID())
        let screens = NSScreen.screens.map { s -> ScreenInfo in
            let sid = (s.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.intValue ?? -1
            return ScreenInfo(displayID: sid, x: s.frame.origin.x, y: s.frame.origin.y,
                              w: s.frame.size.width, h: s.frame.size.height)
        }
        guard let f = ScreenPlacement.chooseFrame(screens: screens, mainDisplayID: mainID,
                                                  windowWidth: window.frame.size.width,
                                                  windowHeight: window.frame.size.height) else {
            Config.fail("no NSScreen matching CGMainDisplayID=\(mainID) (screens: " +
                        screens.map { "id=\($0.displayID) \($0.x),\($0.y) \($0.w)x\($0.h)" }.joined(separator: "; ") +
                        ") large enough for \(window.frame.size.width)x\(window.frame.size.height); refusing to start invisibly on a non-primary display")
        }
        window.setFrame(NSRect(x: f.x, y: f.y, width: f.w, height: f.h), display: false)
    }

    private func label(_ frame: NSRect, _ text: String, _ font: NSFont, align: NSTextAlignment = .left) -> NSTextField {
        let l = NSTextField(labelWithString: text)
        l.frame = frame
        l.font = font
        l.alignment = align
        return l
    }

    private func button(_ frame: NSRect, _ title: String, _ action: Selector) -> NSButton {
        let b = NSButton(title: title, target: self, action: action)
        b.frame = frame
        b.bezelStyle = .regularSquare
        b.font = NSFont.boldSystemFont(ofSize: 15)
        return b
    }

    private func buildUI() {
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 760),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered, defer: false)
        window.title = kWindowTitle
        window.delegate = self
        placeWindowOnMainDisplay()
        guard let v = window.contentView else { return }
        if let gesture = config.gestureSuite {
            let controller = GestureController(suite: gesture, logger: logger, seed: seedUsed)
            gestureController = controller
            controller.buildContent(in: v, window: window)
            return
        }

        trialLabel = label(NSRect(x: 20, y: 714, width: 500, height: 32), "Trial 1", .boldSystemFont(ofSize: 18))
        nonceLabel = label(NSRect(x: 20, y: 656, width: 860, height: 48), "NONCE:",
                           .monospacedSystemFont(ofSize: 30, weight: .bold), align: .center)
        let instr = label(NSRect(x: 20, y: 626, width: 860, height: 26),
                          "TARGET: the only GREEN CIRCLE — click it", .systemFont(ofSize: 16), align: .center)
        statusLabel = label(NSRect(x: 20, y: 576, width: 860, height: 44), "", .boldSystemFont(ofSize: 26), align: .center)
        statusLabel.drawsBackground = true
        statusLabel.backgroundColor = .white

        shapesView = ShapesView(frame: NSRect(x: 20, y: 346, width: 860, height: 224))
        shapesView.onClick = { [weak self] slot in self?.shapeClicked(slot) }

        // Large, wrapping sample label; emoji suite gets an even bigger font.
        sampleLabel = label(NSRect(x: 20, y: 196, width: 860, height: 144),
                            "", suite == .emoji ? .systemFont(ofSize: 40) : .systemFont(ofSize: 26))
        sampleLabel.lineBreakMode = .byCharWrapping
        sampleLabel.usesSingleLineMode = false
        sampleLabel.maximumNumberOfLines = 0

        let scroll = NSScrollView(frame: NSRect(x: 20, y: 60, width: 860, height: 130))
        scroll.hasVerticalScroller = true
        scroll.borderType = .lineBorder
        textView = NSTextView(frame: NSRect(origin: .zero, size: scroll.contentSize))
        textView.font = .systemFont(ofSize: 16)
        textView.isRichText = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        // The agent must reproduce the payload byte-for-byte; disable every
        // automatic text munging. Comparison stays strict UTF-16 — the actual
        // text is logged as-is, never normalized.
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.isContinuousSpellCheckingEnabled = false
        scroll.documentView = textView

        resultLabel = label(NSRect(x: 20, y: 16, width: 330, height: 40), "", .boldSystemFont(ofSize: 14))
        let checkBtn = button(NSRect(x: 360, y: 12, width: 160, height: 44), "Check text", #selector(checkText))
        let nextBtn  = button(NSRect(x: 540, y: 12, width: 160, height: 44), "Next trial", #selector(nextTrial))
        let closeBtn = button(NSRect(x: 720, y: 12, width: 160, height: 44), "Cancel / Close", #selector(closeApp))

        for sub in [trialLabel, nonceLabel, instr, statusLabel, shapesView, sampleLabel, scroll, resultLabel,
                    checkBtn, nextBtn, closeBtn] as [NSView] {
            v.addSubview(sub)
        }
    }

    // MARK: Trial logic

    private func makeNonce() -> String {
        String((0 ..< 6).map { _ in kNonceChars[rng.below(kNonceChars.count)] })
    }

    private func drawShapes() {
        var generated: [Item] = [Item(kind: .circle, color: Palette.green, colorName: "green", isTarget: true)]
        let shapes: [ShapeKind] = [.square, .triangle, .diamond].shuffled(using: &rng)
        let colors: [(NSColor, String)] = [(Palette.red, "red"), (Palette.blue, "blue"), (Palette.yellow, "yellow")]
            .shuffled(using: &rng)
        for i in 0 ..< 3 {
            generated.append(Item(kind: shapes[i], color: colors[i].0, colorName: colors[i].1, isTarget: false))
        }
        items = generated.shuffled(using: &rng)
        targetSlot = items.firstIndex(where: { $0.isTarget }) ?? 0
        shapesView.items = items
    }

    private func startTrial() {
        if suite != .legacy && suiteFinished {
            // Next beyond the last case: no new case; keep SUITE COMPLETE shown.
            return
        }
        trialIndex += 1
        nonce = makeNonce()
        drawShapes()

        if suite == .legacy {
            currentCase = suiteCases[0]
            trialLabel.stringValue = "Trial \(trialIndex)"
        } else {
            currentCase = suiteCases[trialIndex - 1]
            // Header must be machine/agent-readable with the current case ID.
            trialLabel.stringValue = "Trial \(trialIndex)/\(suiteCases.count) — \(currentCase?.id ?? "?") — suite: \(suite.rawValue)"
        }

        nonceLabel.stringValue = "NONCE: \(nonce)"

        statusLabel.stringValue = ""
        statusLabel.textColor = .black
        statusLabel.backgroundColor = .white
        resultLabel.stringValue = ""
        if let c = currentCase {
            sampleLabel.stringValue = "Type exactly:  \(c.payload)"
        }
        textView.string = ""

        let layout = items.enumerated().map { "\($0.offset):\($0.element.colorName)-\($0.element.kind.rawValue)" }.joined(separator: ",")
        logger?.log("trial", ["trial": trialIndex, "nonce": nonce, "target": "green circle",
                              "target_slot": targetSlot, "layout": layout,
                              "suite": suite.rawValue,
                              "case_id": currentCase?.id ?? "",
                              "case_index": trialIndex,
                              "case_total": suite == .legacy ? 0 : suiteCases.count,
                              "expected_text": currentCase?.expectedText ?? ""])
    }

    private func shapeClicked(_ slot: Int) {
        let hit = (slot == targetSlot)
        statusLabel.stringValue = hit ? "TARGET HIT" : "WRONG TARGET"
        statusLabel.textColor = hit ? Palette.green : Palette.red
        statusLabel.backgroundColor = hit
            ? NSColor(calibratedRed: 0.82, green: 0.96, blue: 0.82, alpha: 1)
            : NSColor(calibratedRed: 0.99, green: 0.84, blue: 0.84, alpha: 1)
        logger?.log(hit ? "hit" : "wrong",
                    ["trial": trialIndex, "nonce": nonce, "slot": slot, "target_slot": targetSlot,
                     "suite": suite.rawValue, "case_id": currentCase?.id ?? "",
                     "case_index": trialIndex,
                     "case_total": suite == .legacy ? 0 : suiteCases.count])
    }

    @objc private func checkText() {
        let got = textView.string
        let expected = currentCase?.expectedText ?? ""
        // Exact UTF-16 code-unit comparison — no Trim, no Unicode normalization.
        let matched = CaseContract.utf16ExactMatch(got, expected)
        let expectedUtf16Hex = CaseContract.utf16Hex(expected)
        let utf16Hex = CaseContract.utf16Hex(got)
        let expectedLen = expected.count   // grapheme-cluster count, informational
        let gotLen = got.count
        if matched {
            resultLabel.stringValue = "MATCHED — \(gotLen) characters"
            resultLabel.textColor = Palette.green
        } else {
            resultLabel.stringValue = "MISMATCH — expected \(expectedLen), got \(gotLen)"
            resultLabel.textColor = Palette.red
        }
        logger?.log("text_check", ["trial": trialIndex, "nonce": nonce, "matched": matched,
                                   "expected_len": expectedLen, "got_len": gotLen,
                                   "suite": suite.rawValue,
                                   "case_id": currentCase?.id ?? "",
                                   "case_index": trialIndex,
                                   "case_total": suite == .legacy ? 0 : suiteCases.count,
                                   "task_payload": currentCase?.payload ?? "",
                                   "expected_text": expected,
                                   "actual_text": got,
                                   "expected_utf16_hex": expectedUtf16Hex,
                                   "actual_utf16_hex": utf16Hex])
    }

    @objc private func nextTrial() {
        if suite != .legacy && !suiteFinished && trialIndex >= suiteCases.count {
            suiteFinished = true
            statusLabel.stringValue = "SUITE COMPLETE — \(suiteCases.count)/\(suiteCases.count) cases"
            statusLabel.textColor = Palette.green
            statusLabel.backgroundColor = NSColor(calibratedRed: 0.82, green: 0.96, blue: 0.82, alpha: 1)
            logger?.log("suite_complete", ["trial": trialIndex, "suite": suite.rawValue,
                                           "case_id": currentCase?.id ?? "",
                                           "case_index": trialIndex, "case_total": suiteCases.count])
            return
        }
        startTrial()
    }

    @objc private func closeApp() { NSApp.terminate(nil) }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

// MARK: - Offscreen drawing regression (no window, no screenshot, no input)
//
// Reproduces the suspected header-blanking mechanism: layer-backed rendering
// does not clip NSView.draw to bounds, so a white dirtyRect.fill() could paint
// over earlier sibling labels. The harness sensitivity check proves this
// offscreen setup CAN detect an unclipped fill escaping bounds; the fixed-view
// checks prove the real ShapesView never escapes.

enum DrawingSelfTest {
    // Mimics the pre-fix draw: unclipped dirtyRect white fill.
    final class LegacyShapesDraw {
        private(set) var legacyFillRect: NSRect = .zero
        func legacyDraw(_ dirtyRect: NSRect) {
            NSColor.white.setFill()
            dirtyRect.fill()
            legacyFillRect = dirtyRect
        }
    }

    private static func withBitmapContext(size: NSSize, _ body: () -> Void) {
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width), pixelsHigh: Int(size.height),
                                   bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                   colorSpaceName: .calibratedRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        let ctx = NSGraphicsContext(bitmapImageRep: rep)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = ctx
        body()
        NSGraphicsContext.restoreGraphicsState()
    }

    static func run() -> [SelfTestResult] {
        var r: [SelfTestResult] = []
        let viewSize = NSSize(width: 860, height: 224)
        // Outer clip deliberately much larger than the view bounds — simulates
        // a rendering context that does not pre-clip to the view.
        let bigOuter = NSRect(x: -500, y: -500, width: 2000, height: 2000)

        // Harness sensitivity: an unclipped fill with an oversized dirtyRect
        // escapes bounds. If this ever fails, the offscreen harness is blind
        // to the bug class it exists to catch.
        let legacy = LegacyShapesDraw()
        let viewBounds = NSRect(origin: .zero, size: viewSize)
        withBitmapContext(size: NSSize(width: 2000, height: 2000)) {
            NSGraphicsContext.current!.cgContext.clip(to: bigOuter)
            legacy.legacyDraw(NSRect(x: -200, y: -200, width: 1400, height: 700))
        }
        let escaped = !boundsContains(viewBounds, legacy.legacyFillRect)
        r.append(SelfTestResult(name: "draw-clip-harness-detects-unclipped-escape",
                                passed: escaped,
                                detail: escaped ? "legacy fill rect \(legacy.legacyFillRect) escapes bounds (as suspected)"
                                        : "harness could not detect escape — check setup"))

        // Fixed view: white fill strictly within bounds...
        let fixed = ShapesView(frame: NSRect(origin: .zero, size: viewSize))
        fixed.recordDrawing = true
        fixed.items = [Item(kind: .circle, color: Palette.green, colorName: "green", isTarget: true)]
        withBitmapContext(size: NSSize(width: 2000, height: 2000)) {
            NSGraphicsContext.current!.cgContext.clip(to: bigOuter)
            fixed.draw(NSRect(x: -200, y: -200, width: 1400, height: 700))
        }
        r.append(SelfTestResult(name: "draw-fill-within-bounds",
                                passed: boundsContains(fixed.bounds, fixed.lastFillRect),
                                detail: "fill \(fixed.lastFillRect)"))
        // ...and the effective clip during shape painting never leaves bounds.
        r.append(SelfTestResult(name: "draw-shape-clip-within-bounds",
                                passed: boundsContains(fixed.bounds, fixed.lastDrawClipBounds),
                                detail: "clip \(fixed.lastDrawClipBounds)"))

        // First-mouse click-through: a real NSView API behavior check (no
        // window). Inactive-window first clicks are swallowed by default
        // (acceptsFirstMouse(for:) == false), which matched the observed
        // first-click no-op; the fixture's own canvas opts in like standard
        // clickable controls do.
        let firstMouseTarget = ShapesView(frame: NSRect(origin: .zero, size: viewSize))
        r.append(SelfTestResult(name: "first-mouse-accepted",
                                passed: firstMouseTarget.acceptsFirstMouse(for: nil),
                                detail: "acceptsFirstMouse(for: nil) = \(firstMouseTarget.acceptsFirstMouse(for: nil))"))

        return r
    }

    private static func boundsContains(_ outer: NSRect, _ inner: NSRect) -> Bool {
        inner.width >= 0 && inner.height >= 0
            && inner.minX >= outer.minX - 0.01 && inner.minY >= outer.minY - 0.01
            && inner.maxX <= outer.maxX + 0.01 && inner.maxY <= outer.maxY + 0.01
    }
}

// MARK: - Entry point

let config = Config.parse()

if config.selfTest {
    var results = SelfTest.run()
    results.append(contentsOf: DrawingSelfTest.run())
    results.append(contentsOf: GestureSelfTest.run())
    results.append(contentsOf: GestureRegression.run())
    for r in results {
        print((r.passed ? "PASS" : "FAIL") + ": " + r.name + (r.detail.isEmpty ? "" : " — " + r.detail))
    }
    let failed = results.filter { !$0.passed }.count
    if failed == 0 {
        print("SELF-TEST PASSED (\(results.count) checks)")
    } else {
        print("SELF-TEST FAILED (\(failed)/\(results.count) checks failed)")
    }
    exit(failed == 0 ? 0 : 1)
}

if let exportPath = config.exportPath {
    guard let json = GestureExporter.manifestJSON() else {
        Config.fail("failed to build case manifest JSON")
    }
    do {
        try json.write(toFile: exportPath, atomically: true, encoding: .utf8)
    } catch {
        Config.fail("cannot write export file \(exportPath): \(error)")
    }
    print("exported case manifest: \(exportPath)")
    print("WARNING: coordinator-only file (contains answers). Must NOT be placed in the GUI agent publish directory.")
    exit(0)
}

let app = NSApplication.shared
let delegate = AppDelegate(config: config)
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
