//
//  Computer Use Acceptance Fixture (macOS, AppKit)
//  A small native visual target range for screenshot-only computer-use agents.
//  No network, no file access except the optional coordinator-owned evidence file.
//
//  Usage: acceptance-fixture [--seed N] [--evidence-file PATH]
//

import AppKit
import Foundation

// MARK: - Constants

let kSampleText = "computer-use 你好 10×20"   // exact Unicode sample the agent must type
let kNonceChars = Array("ABCDEFGHJKLMNPQRSTUVWXYZ23456789") // no ambiguous 0/O 1/I
let kWindowTitle = "Computer Use Acceptance"

// MARK: - CLI config

struct Config {
    var seed: UInt64? = nil
    var evidencePath: String? = nil

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
            } else {
                i += 1
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

    init?(path: String?) {
        guard let p = path, !p.isEmpty else { return nil }
        self.path = p
        if !FileManager.default.fileExists(atPath: p) {
            FileManager.default.createFile(atPath: p, contents: nil)
        }
    }

    func log(_ type: String, _ fields: [String: Any] = [:]) {
        var obj: [String: Any] = ["type": type, "ts": ts.string(from: Date())]
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
        NSColor.white.setFill()
        dirtyRect.fill()
        for (slot, item) in items.enumerated() {
            let p = path(for: item.kind, in: rect(forSlot: slot).insetBy(dx: 4, dy: 4))
            item.color.setFill()
            p.fill()
            NSColor.black.setStroke()
            p.lineWidth = 3
            p.stroke()
        }
    }

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

    private var window: NSWindow!
    private var shapesView: ShapesView!
    private var trialLabel: NSTextField!
    private var nonceLabel: NSTextField!
    private var statusLabel: NSTextField!
    private var resultLabel: NSTextField!
    private var textView: NSTextView!

    private var trialIndex = 0
    private var items: [Item] = []
    private var targetSlot = 0
    private var nonce = ""

    init(config: Config) {
        self.config = config
        self.seedUsed = config.seed ?? UInt64.random(in: UInt64.min ... UInt64.max)
        self.rng = SplitMix64(state: seedUsed)
        self.logger = EvidenceLogger(path: config.evidencePath)
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildUI()
        logger?.log("session", ["seed": String(seedUsed), "platform": "macos"])
        startTrial()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func windowWillClose(_ notification: Notification) { NSApp.terminate(nil) }

    // MARK: UI construction

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
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 650),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered, defer: false)
        window.title = kWindowTitle
        window.delegate = self
        window.center()
        guard let v = window.contentView else { return }

        trialLabel = label(NSRect(x: 20, y: 604, width: 300, height: 32), "Trial 1", .boldSystemFont(ofSize: 18))
        nonceLabel = label(NSRect(x: 20, y: 548, width: 860, height: 44), "NONCE:",
                           .monospacedSystemFont(ofSize: 30, weight: .bold), align: .center)
        let instr = label(NSRect(x: 20, y: 516, width: 860, height: 26),
                          "TARGET: the only GREEN CIRCLE — click it", .systemFont(ofSize: 16), align: .center)
        statusLabel = label(NSRect(x: 20, y: 466, width: 860, height: 44), "", .boldSystemFont(ofSize: 26), align: .center)
        statusLabel.drawsBackground = true
        statusLabel.backgroundColor = .white

        shapesView = ShapesView(frame: NSRect(x: 20, y: 230, width: 860, height: 230))
        shapesView.onClick = { [weak self] slot in self?.shapeClicked(slot) }

        let sample = label(NSRect(x: 20, y: 196, width: 860, height: 26),
                           "Type exactly:  \(kSampleText)", .systemFont(ofSize: 16))

        let scroll = NSScrollView(frame: NSRect(x: 20, y: 80, width: 860, height: 110))
        scroll.hasVerticalScroller = true
        scroll.borderType = .lineBorder
        textView = NSTextView(frame: NSRect(origin: .zero, size: scroll.contentSize))
        textView.font = .systemFont(ofSize: 16)
        textView.isRichText = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        scroll.documentView = textView

        resultLabel = label(NSRect(x: 20, y: 24, width: 330, height: 40), "", .boldSystemFont(ofSize: 14))
        let checkBtn = button(NSRect(x: 360, y: 20, width: 160, height: 44), "Check text", #selector(checkText))
        let nextBtn  = button(NSRect(x: 540, y: 20, width: 160, height: 44), "Next trial", #selector(nextTrial))
        let closeBtn = button(NSRect(x: 720, y: 20, width: 160, height: 44), "Cancel / Close", #selector(closeApp))

        for sub in [trialLabel, nonceLabel, instr, statusLabel, shapesView, sample, scroll, resultLabel,
                    checkBtn, nextBtn, closeBtn] as [NSView] {
            v.addSubview(sub)
        }
    }

    // MARK: Trial logic

    private func makeNonce() -> String {
        String((0 ..< 6).map { _ in kNonceChars[rng.below(kNonceChars.count)] })
    }

    private func startTrial() {
        trialIndex += 1
        nonce = makeNonce()

        var generated: [Item] = [Item(kind: .circle, color: Palette.green, colorName: "green", isTarget: true)]
        let shapes: [ShapeKind] = [.square, .triangle, .diamond].shuffled(using: &rng)
        let colors: [(NSColor, String)] = [(Palette.red, "red"), (Palette.blue, "blue"), (Palette.yellow, "yellow")]
            .shuffled(using: &rng)
        for i in 0 ..< 3 {
            generated.append(Item(kind: shapes[i], color: colors[i].0, colorName: colors[i].1, isTarget: false))
        }
        items = generated.shuffled(using: &rng)
        targetSlot = items.firstIndex(where: { $0.isTarget }) ?? 0

        trialLabel.stringValue = "Trial \(trialIndex)"
        nonceLabel.stringValue = "NONCE: \(nonce)"
        statusLabel.stringValue = ""
        statusLabel.backgroundColor = .white
        resultLabel.stringValue = ""
        textView.string = ""
        shapesView.items = items

        let layout = items.enumerated().map { "\($0.offset):\($0.element.colorName)-\($0.element.kind.rawValue)" }.joined(separator: ",")
        logger?.log("trial", ["trial": trialIndex, "nonce": nonce, "target": "green circle",
                              "target_slot": targetSlot, "layout": layout])
    }

    private func shapeClicked(_ slot: Int) {
        let hit = (slot == targetSlot)
        statusLabel.stringValue = hit ? "TARGET HIT" : "WRONG TARGET"
        statusLabel.textColor = hit ? Palette.green : Palette.red
        statusLabel.backgroundColor = hit
            ? NSColor(calibratedRed: 0.82, green: 0.96, blue: 0.82, alpha: 1)
            : NSColor(calibratedRed: 0.99, green: 0.84, blue: 0.84, alpha: 1)
        logger?.log(hit ? "hit" : "wrong",
                    ["trial": trialIndex, "nonce": nonce, "slot": slot, "target_slot": targetSlot])
    }

    @objc private func checkText() {
        let got = textView.string
        let matched = (got == kSampleText)
        let expected = kSampleText.count   // grapheme-cluster count
        if matched {
            resultLabel.stringValue = "MATCHED — \(got.count) characters"
            resultLabel.textColor = Palette.green
        } else {
            resultLabel.stringValue = "MISMATCH — expected \(expected), got \(got.count)"
            resultLabel.textColor = Palette.red
        }
        logger?.log("text_check", ["trial": trialIndex, "nonce": nonce, "matched": matched,
                                   "expected_len": expected, "got_len": got.count])
    }

    @objc private func nextTrial() { startTrial() }

    @objc private func closeApp() { NSApp.terminate(nil) }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

// MARK: - Entry point

let config = Config.parse()
let app = NSApplication.shared
let delegate = AppDelegate(config: config)
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
