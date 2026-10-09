import Foundation

// MARK: - Verdict

struct GestureVerdict {
    var matched: Bool
    var reason: String
    var observed: String
}

// MARK: - Judges (pure; synthetic-event testable, no UI)

enum GestureJudge {

    // ---- multiclick helpers ----
    static func downs(_ events: [GestureEvent], area: String? = nil, button: String? = nil) -> [GestureEvent] {
        return events.filter { e in
            e.kind == "down"
                && (area == nil || e.area == area)
                && (button == nil || e.button == button)
        }
    }
    static func maxCount(_ events: [GestureEvent]) -> Int {
        return events.reduce(0) { max($0, $1.nativeCount) }
    }
    static func doubleClickMsgCount(_ events: [GestureEvent]) -> Int {
        return events.filter { $0.doubleClickMsg }.count
    }

    // One DOWN (including Windows DBLCLK) is one press. UP is mandatory.
    static func validStream(_ events: [GestureEvent], drag: Bool = false) -> Bool {
        var held: String? = nil
        var lastTime = -Double.infinity
        for e in events {
            guard e.tMs.isFinite, e.tMs >= 0, e.tMs >= lastTime,
                  e.x.isFinite, e.y.isFinite, e.dx.isFinite, e.dy.isFinite else { return false }
            lastTime = e.tMs
            switch e.kind {
            case "down":
                if held != nil { return false }; held = e.button
            case "up":
                if held != e.button || e.held { return false }; held = nil
            case "move":
                if e.held && (held != e.button || !drag) { return false }
                if drag && held != nil && !e.held { return false }
            default: return false
            }
        }
        return held == nil
    }

    static func judgeMulticlick(_ c: GestureCase, _ events: [GestureEvent], _ obs: GestureObserved,
                               platform: String, clickWindowMs: Double = 500, clickSlop: Double = 6) -> GestureVerdict {
        let flow = c.field("flow") ?? ""
        let d = downs(events)
        let target = d.filter { $0.area == "target" }
        func v(_ ok: Bool, _ why: String) -> GestureVerdict {
            GestureVerdict(matched: ok, reason: why,
                observed: "downs=\(d.count) ups=\(events.filter { $0.kind == "up" }.count) native=\(d.map { $0.nativeCount }) dblclk=\(doubleClickMsgCount(events)) selection='\(obs.selection)'")
        }
        if !validStream(events) { return v(false, "invalid event order/timing or missing release") }
        if flow == "invalid_count" { return v(events.isEmpty, "no native input permitted; rejection still needs tool evidence") }
        if d.isEmpty || events.contains(where: { $0.area == "outside" }) { return v(false, "missing target hit or outside input") }
        if events.filter({ $0.kind == "up" }).map({ $0.area }) != d.map({ $0.area }) { return v(false, "release outside pressed target") }
        func rapid(_ ds: [GestureEvent]) -> Bool {
            zip(ds, ds.dropFirst()).allSatisfy { a, b in
                b.tMs - a.tMs <= clickWindowMs && abs(b.x - a.x) <= clickSlop && abs(b.y - a.y) <= clickSlop
            }
        }
        func nativeMulti(_ ds: [GestureEvent], _ n: Int) -> Bool {
            if n < 2 || ds.count != n || !ds.allSatisfy({ $0.button == "left" }) || !rapid(ds) { return false }
            if platform == "macos" { return ds.map({ $0.nativeCount }) == Array(1...n) }
            // The third Windows press can be DOWN(count=1) or DBLCLK(count=2).
            return ds[0].nativeCount == 1 && ds.contains(where: { $0.doubleClickMsg && $0.nativeCount == 2 })
        }
        switch flow {
        case "single": return v(d.count == 1 && target.count == 1 && d[0].button == "left" && d[0].nativeCount == 1, "one released native single click")
        case "multi":
            let n = c.intField("expect_count") ?? 0
            return v(d.count == n && nativeMulti(target, n), "requires N real down/up pairs within native timing/position bounds")
        case "select_word", "select_line":
            let n = flow == "select_word" ? 2 : 3
            let expected = flow == "select_word" ? "alpha" : GestureCatalog.selectWordSentence
            let selectionOK = platform == "windows" && n == 3 ? !obs.selection.isEmpty : obs.selection == expected
            return v(nativeMulti(d.filter { $0.area == "text" }, n) && d.count == n && selectionOK,
                     platform == "windows" && n == 3 ? "three released presses with DBLCLK and native nonempty selection (not a line-selection guarantee)" : "native multi-click and exact visible selection")
        case "two_targets":
            return v(d.count == 2 && d.map({ $0.area }) == ["zoneA", "zoneB"] && d.allSatisfy({ $0.button == "left" && $0.nativeCount == 1 }), "A then B, independent released singles")
        case "two_positions":
            return v(d.count == 2 && target.count == 2 && d.allSatisfy({ $0.button == "left" && $0.nativeCount == 1 }) && abs(d[1].x - d[0].x) >= 200,
                     "far-apart released singles, distance >=200px")
        case "right_between":
            return v(target.count == 3 && d.count == 3 && d.map({ $0.button }) == ["left", "right", "left"] && d.allSatisfy({ $0.nativeCount == 1 }), "left/right/left; native counts must reset")
        case "slow_two":
            return v(target.count == 2 && d.count == 2 && d.allSatisfy({ $0.button == "left" && $0.nativeCount == 1 }) && d[1].tMs - d[0].tMs >= 2000,
                     "two released singles at least 2000ms apart")
        default: return v(false, "unknown flow")
        }
    }

    // ---- drag helpers ----
    static func heldMoves(_ events: [GestureEvent], area: String = "canvas") -> [GestureEvent] {
        return events.filter { $0.kind == "move" && $0.area == area && $0.held }
    }
    static func firstDown(_ events: [GestureEvent], area: String, button: String = "left") -> GestureEvent? {
        return downs(events, area: area, button: button).first
    }
    static func lastUp(_ events: [GestureEvent], area: String, button: String = "left") -> GestureEvent? {
        return events.filter { $0.kind == "up" && $0.area == area && $0.button == button }.last
    }
    static func inZone(_ e: GestureEvent, _ z: GestureZone) -> Bool {
        return e.x >= z.x && e.x <= z.x + z.w && e.y >= z.y && e.y <= z.y + z.h
    }

    static func segmentNear(_ a: GestureEvent, _ b: GestureEvent, _ z: GestureZone) -> Bool {
        let minX = z.x + z.w / 2 - 60, maxX = minX + 120
        let minY = z.y + z.h / 2 - 60, maxY = minY + 120
        var lo = 0.0, hi = 1.0
        for (origin, delta, low, high) in [(a.x, b.x-a.x, minX, maxX), (a.y, b.y-a.y, minY, maxY)] {
            if abs(delta) < 0.0001 { if origin < low || origin > high { return false } }
            else {
                let t1 = (low-origin)/delta, t2 = (high-origin)/delta
                lo = max(lo, min(t1, t2)); hi = min(hi, max(t1, t2))
                if lo > hi { return false }
            }
        }
        return true
    }

    static func judgeDrag(_ c: GestureCase, _ events: [GestureEvent], _ obs: GestureObserved, zones: [GestureZone]) -> GestureVerdict {
        let flow = c.field("flow") ?? ""
        if !validStream(events, drag: true) || downs(events).count != 1
            || events.filter({ $0.kind == "up" }).count != 1 || downs(events).first?.button != "left" {
            return GestureVerdict(matched: false, reason: "requires one ordered down / held motion / released up", observed: "events=\(events.count)")
        }
        if flow == "text_select" {
            let d = downs(events, area: "text")
            if d.count < 1 { return GestureVerdict(matched: false, reason: "no down on the sentence", observed: "selection='\(obs.selection)'") }
            if heldMoves(events, area: "text").count < 1 {
                return GestureVerdict(matched: false, reason: "no held motion on the sentence (endpoint-only selection is not a drag)",
                                      observed: "selection='\(obs.selection)'")
            }
            if lastUp(events, area: "text") == nil { return GestureVerdict(matched: false, reason: "no release on sentence", observed: obs.selection) }
            if obs.selection != "quick brown" {
                return GestureVerdict(matched: false, reason: "selection must be exactly 'quick brown', got '\(obs.selection)'",
                                      observed: "selection='\(obs.selection)'")
            }
            return GestureVerdict(matched: true, reason: "drag-selected 'quick brown'", observed: "selection='\(obs.selection)'")
        }

        func v(_ ok: Bool, _ why: String, _ extra: String = "") -> GestureVerdict {
            var obsSummary = "held_moves=\(heldMoves(events).count)"
            for z in zones { obsSummary += " \(z.name)=\(z.x),\(z.y),\(Int(z.w))x\(Int(z.h))" }
            return GestureVerdict(matched: ok, reason: why, observed: obsSummary + (extra.isEmpty ? "" : " " + extra))
        }
        guard let start = zones.first(where: { $0.name == "START" }),
              let end = zones.first(where: { $0.name == "END" }) else {
            return v(false, "layout missing START/END")
        }
        guard let down = firstDown(events, area: "canvas") else { return v(false, "no press inside the canvas") }
        guard down.button == "left" else { return v(false, "drag must use the left button") }
        guard inZone(down, start) else { return v(false, "press must start inside START") }
        guard let up = lastUp(events, area: "canvas") else { return v(false, "no release inside the canvas") }
        let moves = heldMoves(events)
        if moves.isEmpty { return v(false, "no held motion between press and release (endpoint-only is not a drag)") }
        // Release ends the drag: any held move AFTER the final up cannot exist
        // by construction (held=false), but a second down without up means a
        // stuck drag; require the last event to be the up.
        if moves.contains(where: { $0.tMs > up.tMs }) { return v(false, "held motion after release") }
        let endOK = inZone(up, end)
        let pts = [down] + moves + [up]
        let minX = pts.map { $0.x }.min() ?? 0, maxX = pts.map { $0.x }.max() ?? 0
        let minY = pts.map { $0.y }.min() ?? 0, maxY = pts.map { $0.y }.max() ?? 0
        let spanX = maxX - minX, spanY = maxY - minY

        let axis = c.field("axis")
        if axis == "h" {
            guard endOK else { return v(false, "release must be inside END") }
            if spanX < abs(end.x - start.x) * 0.6 { return v(false, "held path must span >=60% of the horizontal distance, got \(Int(spanX))px") }
            return v(true, "horizontal drag START->END")
        }
        if axis == "v" {
            guard endOK else { return v(false, "release must be inside END") }
            if spanY < abs(end.y - start.y) * 0.6 { return v(false, "held path must span >=60% of the vertical distance, got \(Int(spanY))px") }
            return v(true, "vertical drag START->END")
        }
        if c.field("path") == "polyline" {
            guard endOK else { return v(false, "release must be inside END") }
            guard let wp = zones.first(where: { $0.name == "WAYPOINT" }) else { return v(false, "layout missing WAYPOINT") }
            let near = zip(pts, pts.dropFirst()).contains { a, b in segmentNear(a, b, wp) }
            if !near { return v(false, "held path must pass near the WAYPOINT (within 60px)") }
            return v(true, "polyline drag through waypoint")
        }
        if c.field("path") == "curve" {
            guard endOK else { return v(false, "release must be inside END") }
            if moves.count < 3 { return v(false, "curved path needs >=3 held samples, got \(moves.count)") }
            var signChanges = 0
            var lastSign = 0
            for i in 1..<moves.count {
                let dx = moves[i].x - moves[i - 1].x
                let s = dx > 0 ? 1 : (dx < 0 ? -1 : 0)
                if s != 0 {
                    if lastSign != 0 && s != lastSign { signChanges += 1 }
                    lastSign = s
                }
            }
            if signChanges < 2 { return v(false, "path must change horizontal direction >=2 times, got \(signChanges)") }
            return v(true, "continuous curved path (\(moves.count) samples, \(signChanges) direction changes)")
        }
        if c.field("span") == "short" {
            guard endOK else { return v(false, "release must be inside END") }
            if spanX < 40 && spanY < 40 { return v(false, "displacement too small (\(Int(spanX)),\(Int(spanY)))") }
            return v(true, "short drag completed")
        }
        if c.field("span") == "long" {
            guard endOK else { return v(false, "release must be inside END") }
            if up.tMs - down.tMs < Double(c.intField("min_ms") ?? 0) { return v(false, "long drag must hold for at least 750ms") }
            if spanX < 500 { return v(false, "long drag must span >=500px, got \(Int(spanX))px") }
            return v(true, "long drag completed (\(Int(spanX))px)")
        }
        if c.field("release") == "inner_edge" {
            guard endOK else { return v(false, "release must be inside END") }
            // inner edge band: within 45px of the rect boundary, not the center
            let distToLeft = abs(up.x - end.x), distToRight = abs(up.x - (end.x + end.w))
            let distToTop = abs(up.y - end.y), distToBottom = abs(up.y - (end.y + end.h))
            let minEdge = min(distToLeft, distToRight, distToTop, distToBottom)
            let cx = end.x + end.w / 2, cy = end.y + end.h / 2
            let fromCenter = ((up.x - cx) * (up.x - cx) + (up.y - cy) * (up.y - cy)).squareRoot()
            if minEdge > 45 { return v(false, "release must be within 45px of the zone edge, got \(Int(minEdge))px") }
            if fromCenter < 40 { return v(false, "release is at the zone center; instruction says near the edge") }
            return v(true, "inner-edge release (\(Int(minEdge))px from edge)")
        }
        return v(false, "unknown drag spec '\(c.spec)'")
    }

    // ---- scroll helpers ----
    static func judgeScroll(_ c: GestureCase, _ events: [GestureEvent], _ obs: GestureObserved, platform: String) -> GestureVerdict {
        let flow = c.field("flow") ?? ""
        let panel = c.field("panel") ?? ""
        func summary(_ which: String) -> String {
            // raw + contract deltas are both reported; nothing normalized away
            let raw = events.filter { $0.kind == "wheel" }.map { e in
                String(format: "(dx=%.2f,dy=%.2f)", e.dx, e.dy)
            }.joined(separator: " ")
            return "panel=\(panel) raw=[\(raw)] " +
                "A v:\(obs.aVStart)->\(obs.aVEnd)/\(obs.aVMax) h:\(obs.aHStart)->\(obs.aHEnd)/\(obs.aHMax) " +
                "B v:\(obs.bVStart)->\(obs.bVEnd)/\(obs.bVMax) h:\(obs.bHStart)->\(obs.bHEnd)/\(obs.bHMax)"
        }
        func fail(_ why: String) -> GestureVerdict { return GestureVerdict(matched: false, reason: why, observed: summary("")) }

        if flow == "zero_delta" {
            let nonzero = events.filter { $0.kind == "wheel" }
            let moved = obs.aVEnd != obs.aVStart || obs.aHEnd != obs.aHStart || obs.bVEnd != obs.bVStart || obs.bHEnd != obs.bHStart
            if moved { return fail("offsets changed on a zero-delta scroll") }
            if !nonzero.isEmpty { return fail("zero-delta request must not inject ANY wheel event") }
            return GestureVerdict(matched: true, reason: "no scroll, no nonzero deltas", observed: summary(""))
        }
        if flow == "invalid_args" {
            let anyWheel = events.filter { $0.kind == "wheel" }
            let moved = obs.aVEnd != obs.aVStart || obs.aHEnd != obs.aHStart || obs.bVEnd != obs.bVStart || obs.bHEnd != obs.bHStart
            if !anyWheel.isEmpty { return fail("invalid args must be rejected pre-dispatch; got \(anyWheel.count) wheel events") }
            if moved { return fail("offsets changed despite invalid args") }
            return GestureVerdict(matched: true, reason: "no wheel events (rejected pre-dispatch)", observed: summary(""))
        }

        if events.contains(where: { !$0.dx.isFinite || !$0.dy.isFinite || !$0.tMs.isFinite || $0.tMs < 0 }) { return fail("invalid raw native event") }
        let wheel = events.filter { $0.kind == "wheel" }
        if wheel.isEmpty { return fail("no native wheel events were recorded") }
        var cd = WheelContract.normalize(platform: platform, rawDx: 0, rawDy: 0)
        var total = WheelContract(dx: 0, dy: 0)
        for e in wheel {
            cd = WheelContract.normalize(platform: platform, rawDx: e.dx, rawDy: e.dy)
            total.dx += cd.dx
            total.dy += cd.dy
        }
        if abs(total.dx) < 0.001 && abs(total.dy) < 0.001 {
            return fail("all wheel deltas are zero in contract units; zero deltas must not count as scrolling")
        }

        let dvA = obs.aVEnd - obs.aVStart
        let dhA = obs.aHEnd - obs.aHStart
        let dvB = obs.bVEnd - obs.bVStart
        let dhB = obs.bHEnd - obs.bHStart
        let axis = c.field("axis")
        let dir = c.field("dir") ?? ""
        let dir2 = c.field("dir2") ?? ""
        let ticks = c.intField("ticks") ?? 1
        // Shared thresholds: small amount >=15px, 3 ticks >=45px; boundary case must actually reach max.
        let minMove: Double = ticks <= 1 ? 15 : 45

        if panel == "B" {
            if dvA != 0 || dhA != 0 { return fail("Panel A must not move; A moved v=\(dvA) h=\(dhA)") }
            if dhB != 0 { return fail("Panel B must move vertically only") }
            if dvB <= 0 { return fail("Panel B must scroll DOWN (visible offset must increase), got v=\(dvB)") }
            if Double(dvB) < minMove { return fail("Panel B moved only \(dvB)px; expected >=\(Int(minMove))px for \(ticks) tick(s)") }
            if total.dy <= 0 { return fail("contract deltas must be positive-down; total dy=\(total.dy)") }
            return GestureVerdict(matched: true, reason: "Panel B scrolled down, Panel A untouched", observed: summary("B"))
        }

        // panel A cases
        if panel == "B" { return fail("unreachable") }
        if axis == "v" && dir == "down" && dir2.isEmpty {
            if dhA != 0 || dvB != 0 || dhB != 0 { return fail("only Panel A vertical may move") }
            let saturated = flow == "saturate" && obs.aVMax > 0 && obs.aVEnd >= obs.aVMax && dvA > 0
            if flow == "saturate" && !saturated { return fail("must reach bottom boundary with actual displacement") }
            if dvA <= 0 { return fail("Panel A must scroll DOWN (offset increase), got v=\(dvA)") }
            if !saturated && Double(dvA) < minMove { return fail("moved only \(dvA)px; expected >=\(Int(minMove))px for \(ticks) tick(s)") }
            if total.dy <= 0 { return fail("contract deltas must be positive-down; total dy=\(total.dy)") }
            let why = saturated ? "saturated at bottom (\(obs.aVEnd)/\(obs.aVMax)) with \(wheel.count) raw events" : "Panel A scrolled down \(dvA)px"
            return GestureVerdict(matched: true, reason: why, observed: summary("A"))
        }
        if axis == "v" && dir == "up" {
            if dhA != 0 || dvB != 0 || dhB != 0 { return fail("only Panel A vertical may move") }
            if dvA >= 0 { return fail("Panel A must scroll UP (offset decrease), got v=\(dvA)") }
            if Double(-dvA) < minMove { return fail("moved only \(dvA)px; expected >=\(Int(minMove))px") }
            if total.dy >= 0 { return fail("contract deltas must be negative-up; total dy=\(total.dy)") }
            return GestureVerdict(matched: true, reason: "Panel A scrolled up \(-dvA)px", observed: summary("A"))
        }
        if axis == "h" {
            if dvA != 0 || dvB != 0 || dhB != 0 { return fail("only Panel A horizontal may move") }
            let expectRight = dir == "right"
            let dh = dhA
            if expectRight && dh <= 0 { return fail("Panel A must scroll RIGHT (h offset increase), got h=\(dh)") }
            if !expectRight && dh >= 0 { return fail("Panel A must scroll LEFT (h offset decrease), got h=\(dh)") }
            if abs(Double(dh)) < minMove { return fail("moved only \(dh)px; expected >=\(Int(minMove))px") }
            if expectRight && total.dx <= 0 { return fail("contract deltas must be positive-right; total dx=\(total.dx)") }
            if !expectRight && total.dx >= 0 { return fail("contract deltas must be negative-left; total dx=\(total.dx)") }
            return GestureVerdict(matched: true, reason: "Panel A scrolled \(dir) \(dh)px", observed: summary("A"))
        }
        if axis == "both" {
            if dvB != 0 || dhB != 0 { return fail("Panel B must not move") }
            if dvA <= 0 { return fail("dual-axis down component missing (v=\(dvA))") }
            if dhA <= 0 { return fail("dual-axis right component missing (h=\(dhA))") }
            if Double(dvA) < minMove || Double(dhA) < minMove {
                return fail("dual-axis movement too small (v=\(dvA), h=\(dhA)); expected >=\(Int(minMove))px each")
            }
            if total.dy <= 0 || total.dx <= 0 { return fail("contract deltas must be positive down/right; dx=\(total.dx), dy=\(total.dy)") }
            return GestureVerdict(matched: true, reason: "Panel A scrolled down+\(dhA)px and down+\(dvA)px", observed: summary("A"))
        }
        return fail("unknown scroll spec '\(c.spec)'")
    }

    static func judge(_ c: GestureCase, suite: String, events: [GestureEvent], obs: GestureObserved,
                      platform: String, zones: [GestureZone]) -> GestureVerdict {
        switch suite {
        case "multiclick": return judgeMulticlick(c, events, obs, platform: platform)
        case "drag": return judgeDrag(c, events, obs, zones: zones)
        case "scroll": return judgeScroll(c, events, obs, platform: platform)
        default: return GestureVerdict(matched: false, reason: "unknown gesture suite '\(suite)'", observed: "")
        }
    }
}

// MARK: - Platform expected notes (oracle-only; NEVER shown to the agent)

enum GesturePlatformNotes {
    static func note(suite: String, caseId: String, platform: String) -> String {
        switch (suite, caseId, platform) {
        case ("multiclick", "multiclick-03", "macos"):
            return "macos: NSEvent.clickCount must reach 3"
        case ("multiclick", "multiclick-03", "windows"):
            return "windows: three real down/up pairs and at least one WM_LBUTTONDBLCLK; third native count may be 1 or 2, never require 3"
        case ("multiclick", "multiclick-05", "macos"):
            return "macos: triple click selects the whole line/sentence"
        case ("multiclick", "multiclick-05", "windows"):
            return "windows: standard EDIT has no triple-click line selection; pass = 3 downs + nonempty visible selection"
        case ("scroll", _, "macos"):
            return "macos: scrollWheel raw deltas logged; contract positive-down/right = -raw"
        case ("scroll", _, "windows"):
            return "windows: WM_MOUSEWHEEL/WM_MOUSEHWHEEL raw deltas logged; contract dy=-raw, dx=+raw"
        default:
            return ""
        }
    }
}

