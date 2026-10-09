import AppKit

// Trial state, owned controls, and coordinator-only evidence; never a global monitor.
final class GestureController: NSObject, GestureEventSink {
    private let suite: String
    private let cases: [GestureCase]
    private let logger: EvidenceLogger?
    private var rng: SplitMix64
    private var trialIndex = 0, checkIndex = 0
    private var finished = false
    private var nonce = ""
    private var events: [GestureEvent] = []
    private var zones: [GestureZone] = []
    private var startMs: Double = 0
    private var starts = (av: 0, ah: 0, bv: 0, bh: 0)
    private var textHeld = false
    private var monitor: Any?
    private weak var window: NSWindow?
    private var trialLabel: NSTextField!, nonceLabel: NSTextField!, instruction: NSTextField!
    private var status: NSTextField!, live: NSTextField!
    private var canvas: GestureClickCanvas!, sentence: GestureSentenceView!
    private var panelA: GestureWheelScrollView!, panelB: GestureWheelScrollView!
    private var offsetA: NSTextField!, offsetB: NSTextField!
    private var next: NSButton!

    init(suite: String, logger: EvidenceLogger?, seed: UInt64) {
        self.suite = suite; cases = GestureCatalog.cases(forSuite: suite) ?? []
        self.logger = logger; rng = SplitMix64(state: seed)
        super.init()
    }
    deinit { if let monitor = monitor { NSEvent.removeMonitor(monitor) } }

    private func label(_ view: NSView, _ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat,
                       _ size: CGFloat, _ text: String = "") -> NSTextField {
        let l = NSTextField(labelWithString: text)
        l.frame = NSRect(x: x, y: y, width: w, height: h)
        l.font = .systemFont(ofSize: size, weight: .semibold)
        l.lineBreakMode = .byWordWrapping; l.usesSingleLineMode = false
        view.addSubview(l); return l
    }

    func buildContent(in view: NSView, window: NSWindow) {
        self.window = window
        trialLabel = label(view, 20, 714, 860, 32, 21)
        nonceLabel = label(view, 20, 666, 860, 44, 30)
        instruction = label(view, 20, 593, 860, 64, 20)
        live = label(view, 20, 552, 860, 34, 17)
        status = label(view, 20, 77, 860, 110, 22)
        canvas = GestureClickCanvas(heldTracking: suite == "drag")
        canvas.frame = NSRect(x: 20, y: 248, width: 860, height: 300)
        canvas.sink = self; view.addSubview(canvas)
        sentence = GestureSentenceView(frame: NSRect(x: 20, y: 198, width: 860, height: 44))
        sentence.isEditable = false; sentence.isSelectable = true; sentence.isRichText = false
        sentence.font = .systemFont(ofSize: 24); sentence.textContainerInset = NSSize(width: 8, height: 8)
        view.addSubview(sentence)
        (panelA, offsetA) = panel(view, "panelA", 20)
        (panelB, offsetB) = panel(view, "panelB", 455)
        let check = NSButton(title: "Check", target: self, action: #selector(checkAction))
        next = NSButton(title: "Next", target: self, action: #selector(nextAction))
        let close = NSButton(title: "Close", target: self, action: #selector(closeAction))
        for (i, b) in [check, next!, close].enumerated() {
            b.frame = NSRect(x: 520 + i * 120, y: 20, width: 110, height: 44)
            b.font = .boldSystemFont(ofSize: 18); b.bezelStyle = .regularSquare; view.addSubview(b)
        }
        // Application-local events only, scoped to this exact owned NSWindow.
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .leftMouseUp, .leftMouseDragged]) { [weak self] event in
            self?.recordTextEvent(event); return event
        }
        startTrial()
    }

    private func panel(_ view: NSView, _ name: String, _ x: CGFloat) -> (GestureWheelScrollView, NSTextField) {
        let s = GestureWheelScrollView(frame: NSRect(x: x, y: 248, width: 425, height: 260))
        s.borderType = .lineBorder; s.hasVerticalScroller = true; s.hasHorizontalScroller = true
        s.panelName = name; s.sink = self
        let grid = GestureGridView(frame: NSRect(x: 0, y: 0, width: 1200, height: 1440)); grid.panelName = name
        s.documentView = grid; view.addSubview(s)
        s.onOffsetChange = { [weak self] in self?.updateOffsets() }
        return (s, label(view, x, 513, 425, 32, 18))
    }
    private func recordTextEvent(_ native: NSEvent) {
        guard !finished, trialIndex > 0, native.window === window, !sentence.isHidden else { return }
        let pt = sentence.convert(native.locationInWindow, from: nil)
        let inside = sentence.bounds.contains(pt)
        if native.type == .leftMouseDown && !inside { return }
        if native.type != .leftMouseDown && !textHeld { return }
        let kind: String
        if native.type == .leftMouseDown { kind = "down"; textHeld = true }
        else if native.type == .leftMouseUp { kind = "up"; textHeld = false }
        else { kind = "move" }
        var raw = GestureEvent(kind: kind, button: "left", nativeCount: native.clickCount, doubleClickMsg: false,
            area: inside ? "text" : "outside", x: pt.x, y: pt.y, dx: 0, dy: 0, held: kind == "move" && textHeld, tMs: 0)
        raw.nativeTimestamp = native.timestamp; raw.nativeType = Int(native.type.rawValue)
        record(raw)
    }
    private func offset(_ s: NSScrollView) -> (v: Int, h: Int) {
        (Int(s.contentView.bounds.origin.y.rounded()), Int(s.contentView.bounds.origin.x.rounded()))
    }
    private func setOffset(_ s: NSScrollView, _ x: CGFloat, _ y: CGFloat) {
        s.contentView.scroll(to: NSPoint(x: x, y: y)); s.reflectScrolledClipView(s.contentView)
    }
    private func updateOffsets() {
        let a = offset(panelA), b = offset(panelB)
        offsetA.stringValue = "Panel A   row \(a.v / 36)   ↓\(a.v)  →\(a.h)"
        offsetB.stringValue = "Panel B   row \(b.v / 36)   ↓\(b.v)  →\(b.h)"
    }
    private func observed() -> GestureObserved {
        let a = offset(panelA), b = offset(panelB)
        return GestureObserved(selection: sentence.selectedText,
            aVStart: starts.av, aVEnd: a.v, aHStart: starts.ah, aHEnd: a.h,
            bVStart: starts.bv, bVEnd: b.v, bHStart: starts.bh, bHEnd: b.h,
            aVMax: Int(1440 - panelA.contentView.bounds.height), aHMax: Int(1200 - panelA.contentView.bounds.width),
            bVMax: Int(1440 - panelB.contentView.bounds.height), bHMax: Int(1200 - panelB.contentView.bounds.width))
    }
    private func fields() -> [String: Any] {
        ["suite": suite, "case_id": cases[trialIndex - 1].id, "nonce": nonce, "trial": trialIndex,
         "case_index": trialIndex, "case_total": cases.count]
    }
    private func log(_ type: String, _ extra: [String: Any] = [:]) {
        var f = fields(); for (k, v) in extra { f[k] = v }; logger?.log(type, f)
    }
    private func startTrial() {
        guard trialIndex < cases.count && !finished else { return }
        trialIndex += 1; checkIndex = 0; events.removeAll(); textHeld = false; canvas.reset()
        nonce = String((0..<6).map { _ in kNonceChars[rng.below(kNonceChars.count)] })
        let c = cases[trialIndex - 1]
        zones = GestureLayout.zones(for: c.spec)
        if c.field("flow") == "two_targets" {
            zones = [GestureZone(name: "zoneA", x: 100, y: 90, w: 220, h: 120), GestureZone(name: "zoneB", x: 540, y: 90, w: 220, h: 120)]
        }
        let jitterX = Double(rng.below(21) - 10), jitterY = Double(rng.below(21) - 10)
        zones = zones.map { GestureZone(name: $0.name, x: $0.x + jitterX, y: $0.y + jitterY, w: $0.w, h: $0.h) }
        canvas.zones = zones; canvas.isHidden = suite == "scroll" || c.field("flow") == "text_select" || c.field("flow") == "select_word" || c.field("flow") == "select_line"
        sentence.isHidden = !(c.field("flow") == "select_word" || c.field("flow") == "select_line" || c.field("flow") == "text_select")
        sentence.string = c.field("flow") == "text_select" ? GestureCatalog.dragSelectSentence : GestureCatalog.selectWordSentence
        sentence.setSelectedRange(NSRange(location: 0, length: 0))
        for v in [panelA!, panelB!, offsetA!, offsetB!] as [NSView] { v.isHidden = suite != "scroll" }
        setOffset(panelA, c.field("dir") == "left" ? 300 : 0, c.field("dir") == "up" ? 400 : 0)
        setOffset(panelB, 0, 0)
        let a = offset(panelA), b = offset(panelB); starts = (a.v, a.h, b.v, b.h)
        updateOffsets()
        trialLabel.stringValue = "\(suite.uppercased()) · \(c.id) · Trial \(trialIndex)/\(cases.count)"
        nonceLabel.stringValue = "NONCE: \(nonce)"
        instruction.stringValue = c.instruction
        status.stringValue = "READY — perform the requirement, then Check once"
        status.textColor = .labelColor; live.stringValue = "Waiting for native input"
        next.title = trialIndex == cases.count ? "Finish" : "Next"
        startMs = ProcessInfo.processInfo.systemUptime * 1000
        log("trial", ["spec": c.spec, "instruction": c.instruction, "platform": "macos",
             "platform_expected": GesturePlatformNotes.note(suite: suite, caseId: c.id, platform: "macos"),
             "started_ms": Date().timeIntervalSince1970 * 1000, "native_click_interval_ms": NSEvent.doubleClickInterval * 1000,
             "zones": zones.map { ["name": $0.name, "x": $0.x, "y": $0.y, "w": $0.w, "h": $0.h] as [String: Any] },
             "a_v_start": a.v, "a_h_start": a.h, "b_v_start": b.v, "b_h_start": b.h])
    }
    func record(_ raw: GestureEvent) {
        guard !finished, trialIndex > 0 else { return }
        var e = raw; e.tMs = ProcessInfo.processInfo.systemUptime * 1000 - startMs
        events.append(e)
        let cd = WheelContract.normalize(platform: "macos", rawDx: e.dx, rawDy: e.dy)
        let a = offset(panelA), b = offset(panelB)
        log("input_event", ["event_index": events.count, "kind": e.kind, "button": e.button, "native_count": e.nativeCount,
             "double_click_msg": e.doubleClickMsg, "area": e.area, "x": e.x, "y": e.y, "raw_dx": e.dx, "raw_dy": e.dy,
             "raw_delta_x": e.legacyDx, "raw_delta_y": e.legacyDy, "contract_dx": cd.dx, "contract_dy": cd.dy,
             "native_timestamp": e.nativeTimestamp, "native_type": e.nativeType, "precise": e.precise, "inverted": e.inverted,
             "phase": e.phase, "momentum_phase": e.momentumPhase, "held": e.held, "t_ms": e.tMs,
             "source": e.area == "text" ? "NSEvent/own-window-local-monitor" : "NSEvent/own-view",
             "a_v": a.v, "a_h": a.h, "b_v": b.v, "b_h": b.h])
        live.stringValue = "Native presses: \(GestureJudge.downs(events).count) · events: \(events.count) · \(e.kind) \(e.area)"
    }
    @objc private func checkAction() {
        guard !finished else { return }
        let c = cases[trialIndex - 1], obs = observed()
        let v: GestureVerdict
        if suite == "multiclick" { v = GestureJudge.judgeMulticlick(c, events, obs, platform: "macos", clickWindowMs: NSEvent.doubleClickInterval * 1000) }
        else { v = GestureJudge.judge(c, suite: suite, events: events, obs: obs, platform: "macos", zones: zones) }
        status.stringValue = (v.matched ? "MATCHED — " : "MISMATCH — ") + v.reason
        status.textColor = v.matched ? Palette.green : Palette.red
        checkIndex += 1
        log("gesture_check", ["check_index": checkIndex, "check_kind": checkIndex == 1 ? "first" : "final",
            "matched": v.matched, "reason": v.reason, "observed": v.observed, "event_count": events.count,
            "selection": obs.selection, "released": !canvas.buttonHeld && !textHeld,
            "a_v_start": obs.aVStart, "a_v_end": obs.aVEnd, "a_h_start": obs.aHStart, "a_h_end": obs.aHEnd,
            "b_v_start": obs.bVStart, "b_v_end": obs.bVEnd, "b_h_start": obs.bHStart, "b_h_end": obs.bHEnd,
            "a_v_max": obs.aVMax, "a_h_max": obs.aHMax, "b_v_max": obs.bVMax, "b_h_max": obs.bHMax])
    }
    @objc private func nextAction() {
        guard !finished else { return }
        log("trial_end", ["checks": checkIndex])
        if trialIndex == cases.count {
            finished = true; status.stringValue = "SUITE COMPLETE — \(cases.count) cases visited (not a pass count)"
            log("suite_complete", ["visited": trialIndex]); next.isEnabled = false
        } else { startTrial() }
    }
    @objc private func closeAction() { log("session_close", ["completed": finished]); NSApp.terminate(nil) }
}
