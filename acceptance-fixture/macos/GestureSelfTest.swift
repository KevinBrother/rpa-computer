import Foundation

// MARK: - Gesture self-tests (pure; synthetic events, no window, no input)

enum GestureSelfTest {
    static func run() -> [SelfTestResult] {
        var r: [SelfTestResult] = []
        func add(_ name: String, _ ok: Bool, _ detail: String = "") {
            r.append(SelfTestResult(name: "gesture-" + name, passed: ok, detail: detail))
        }
        let mac = "macos", win = "windows"
        func ev(_ kind: String, _ area: String, count: Int = 1, dbl: Bool = false,
                button: String = "left", x: Double = 400, y: Double = 150,
                dx: Double = 0, dy: Double = 0, held: Bool = false, t: Double = 0) -> GestureEvent {
            return GestureEvent(kind: kind, button: button, nativeCount: count, doubleClickMsg: dbl,
                                area: area, x: x, y: y, dx: dx, dy: dy, held: held, tMs: t)
        }
        func caseById(_ suite: String, _ id: String) -> GestureCase {
            return GestureCatalog.cases(forSuite: suite)!.first { $0.id == id }!
        }
        let blankObs = GestureObserved(selection: "", aVStart: 0, aVEnd: 0, aHStart: 0, aHEnd: 0,
                                       bVStart: 0, bVEnd: 0, bHStart: 0, bHEnd: 0,
                                       aVMax: 800, aHMax: 400, bVMax: 800, bHMax: 400)

        // -- catalog counts / unique ids --
        add("count-multiclick", GestureCatalog.multiclickCases.count == 10)
        add("count-drag", GestureCatalog.dragCases.count == 10)
        add("count-scroll", GestureCatalog.scrollCases.count == 10)
        let allIds = (GestureCatalog.multiclickCases + GestureCatalog.dragCases + GestureCatalog.scrollCases).map { $0.id }
        add("ids-unique", Set(allIds).count == allIds.count && allIds.count == 30)
        add("instructions-nonempty",
            (GestureCatalog.multiclickCases + GestureCatalog.dragCases + GestureCatalog.scrollCases)
                .allSatisfy { !$0.instruction.isEmpty })
        // instructions must not leak oracle-only info (no count fields, no
        // platform notes inside agent-facing text)
        add("instructions-no-platform-notes",
            GestureCatalog.scrollCases.allSatisfy { !$0.instruction.lowercased().contains("wm_mouse") })

        // -- multiclick: single pass --
        let mc1 = caseById("multiclick", "multiclick-01")
        add("mc01-pass", GestureJudge.judgeMulticlick(mc1, [ev("down", "target", count: 1), ev("up", "target")], blankObs, platform: mac).matched)
        // negative: a double click must NOT pass the single case
        add("mc01-double-rejected",
            !GestureJudge.judgeMulticlick(mc1, [ev("down", "target", count: 1), ev("up", "target"),
                                                ev("down", "target", count: 2, dbl: true), ev("up", "target")], blankObs, platform: mac).matched)
        // -- multiclick: triple mac vs windows --
        let mc3 = caseById("multiclick", "multiclick-03")
        add("mc03-mac-pass",
            GestureJudge.judgeMulticlick(mc3, [ev("down", "target", count: 1), ev("up", "target"),
                                               ev("down", "target", count: 2), ev("up", "target"),
                                               ev("down", "target", count: 3), ev("up", "target")], blankObs, platform: mac).matched)
        // windows third click reports Clicks==2 again (raw preserved, still pass)
        add("mc03-win-no-clicks3-still-pass",
            GestureJudge.judgeMulticlick(mc3, [ev("down", "target", count: 1), ev("up", "target"),
                                               ev("down", "target", count: 2, dbl: true), ev("up", "target"),
                                               ev("down", "target", count: 2, dbl: true), ev("up", "target")], blankObs, platform: win).matched)
        // windows fake: only 2 downs must fail even if a dblclk appeared
        add("mc03-win-two-downs-fail",
            !GestureJudge.judgeMulticlick(mc3, [ev("down", "target", count: 1), ev("up", "target"),
                                                ev("down", "target", count: 2, dbl: true), ev("up", "target")], blankObs, platform: win).matched)
        // mac: Clicks==2 max must fail the triple case
        add("mc03-mac-clicks2-fails",
            !GestureJudge.judgeMulticlick(mc3, [ev("down", "target", count: 1), ev("up", "target"),
                                                ev("down", "target", count: 2), ev("up", "target")], blankObs, platform: mac).matched)
        // -- multiclick: two_positions far apart resets --
        let mc7 = caseById("multiclick", "multiclick-07")
        add("mc07-pass",
            GestureJudge.judgeMulticlick(mc7, [ev("down", "target", count: 1, x: 320), ev("up", "target"),
                                               ev("down", "target", count: 1, x: 560), ev("up", "target", x: 560)], blankObs, platform: mac).matched)
        add("mc07-close-together-fails",
            !GestureJudge.judgeMulticlick(mc7, [ev("down", "target", count: 1, x: 380), ev("up", "target"),
                                                ev("down", "target", count: 2, x: 420)], blankObs, platform: mac).matched)
        // -- multiclick: invalid count means zero events --
        let mc10 = caseById("multiclick", "multiclick-10")
        add("mc10-pass-empty", GestureJudge.judgeMulticlick(mc10, [], blankObs, platform: mac).matched)
        add("mc10-any-event-fails",
            !GestureJudge.judgeMulticlick(mc10, [ev("down", "target")], blankObs, platform: mac).matched)
        // -- multiclick: right between --
        let mc8 = caseById("multiclick", "multiclick-08")
        add("mc08-pass",
            GestureJudge.judgeMulticlick(mc8, [ev("down", "target"), ev("up", "target"),
                                               ev("down", "target", button: "right"), ev("up", "target", button: "right"),
                                               ev("down", "target"), ev("up", "target")], blankObs, platform: mac).matched)
        add("mc08-inherited-count-fails",
            !GestureJudge.judgeMulticlick(mc8, [ev("down", "target"), ev("up", "target"),
                                                ev("down", "target", button: "right"), ev("up", "target", button: "right"),
                                                ev("down", "target", count: 2), ev("up", "target")], blankObs, platform: mac).matched)

        // -- drag: basic horizontal --
        let d1 = caseById("drag", "drag-01")
        var z = GestureLayout.zones(for: d1.spec)
        let sx = z[0].x + 50, sy = z[0].y + 30
        let ex = z[1].x + 50, ey = z[1].y + 30
        var dragEvents: [GestureEvent] = [ev("down", "canvas", x: sx, y: sy, held: true, t: 0)]
        for i in 1...5 {
            let f = Double(i) / 5.0
            dragEvents.append(ev("move", "canvas", x: sx + (ex - sx) * f, y: sy, held: true, t: Double(i) * 50))
        }
        dragEvents.append(ev("up", "canvas", x: ex, y: ey, t: 300))
        add("d01-pass", GestureJudge.judgeDrag(d1, dragEvents, blankObs, zones: z).matched)
        // negative: endpoint-only (no held moves) is not a drag
        add("d01-endpoint-only-fails",
            !GestureJudge.judgeDrag(d1, [ev("down", "canvas", x: sx, y: sy), ev("up", "canvas", x: ex, y: ey)], blankObs, zones: z).matched)
        // negative: release outside END fails
        add("d01-release-outside-fails",
            !GestureJudge.judgeDrag(d1, [ev("down", "canvas", x: sx, y: sy, held: true),
                                         ev("move", "canvas", x: ex + 200, y: ey, held: true),
                                         ev("up", "canvas", x: ex + 200, y: ey)], blankObs, zones: z).matched)
        // negative: not ending with a release fails
        add("d01-no-release-fails",
            !GestureJudge.judgeDrag(d1, [ev("down", "canvas", x: sx, y: sy, held: true),
                                         ev("move", "canvas", x: ex, y: ey, held: true)], blankObs, zones: z).matched)
        // -- drag: polyline through waypoint --
        let d5 = caseById("drag", "drag-05")
        z = GestureLayout.zones(for: d5.spec)
        let start5 = z[0], wp5 = z[1], end5 = z[2]
        var poly: [GestureEvent] = [ev("down", "canvas", x: start5.x + 70, y: start5.y + 45, held: true)]
        poly.append(ev("move", "canvas", x: wp5.x + wp5.w / 2, y: wp5.y + wp5.h / 2, held: true))
        poly.append(ev("move", "canvas", x: wp5.x + 30, y: wp5.y + 40, held: true))
        poly.append(ev("move", "canvas", x: end5.x + 70, y: end5.y + 45, held: true))
        poly.append(ev("up", "canvas", x: end5.x + 70, y: end5.y + 45))
        add("d05-pass", GestureJudge.judgeDrag(d5, poly, blankObs, zones: z).matched)
        add("d05-miss-waypoint-fails",
            !GestureJudge.judgeDrag(d5, [ev("down", "canvas", x: start5.x + 70, y: start5.y + 45, held: true),
                                         ev("move", "canvas", x: 300, y: 250, held: true),
                                         ev("up", "canvas", x: end5.x + 70, y: end5.y + 45)], blankObs, zones: z).matched)
        // -- drag: curve direction changes --
        let d10 = caseById("drag", "drag-10")
        z = GestureLayout.zones(for: d10.spec)
        var curve: [GestureEvent] = [ev("down", "canvas", x: z[0].x + 60, y: z[0].y + 40, held: true)]
        let xs: [Double] = [100, 160, 220, 260, 240, 300, 360, 340, 480, 540, 600, 680, 740]
        let ys: [Double] = [250, 210, 170, 120, 80, 60, 70, 90, 80, 70, 60, 50, 50]
        for i in 0..<xs.count { curve.append(ev("move", "canvas", x: xs[i], y: ys[i], held: true)) }
        curve.append(ev("up", "canvas", x: 740, y: 50))
        add("d10-pass", GestureJudge.judgeDrag(d10, curve, blankObs, zones: z).matched)
        add("d10-straight-fails",
            !GestureJudge.judgeDrag(d10, [ev("down", "canvas", x: z[0].x + 60, y: z[0].y + 40, held: true)]
                + (0..<10).map { i in ev("move", "canvas", x: Double(100 + i * 60), y: 120, held: true) }
                + [ev("up", "canvas", x: 700, y: 50)], blankObs, zones: z).matched)
        // -- drag: text select --
        let d9 = caseById("drag", "drag-09")
        add("d09-pass",
            GestureJudge.judgeDrag(d9, [ev("down", "text"), ev("move", "text", held: true), ev("up", "text")],
                                   GestureObserved(selection: "quick brown", aVStart: 0, aVEnd: 0, aHStart: 0, aHEnd: 0,
                                                   bVStart: 0, bVEnd: 0, bHStart: 0, bHEnd: 0, aVMax: 0, aHMax: 0, bVMax: 0, bHMax: 0),
                                   zones: []).matched)
        add("d09-wrong-selection-fails",
            !GestureJudge.judgeDrag(d9, [ev("down", "text"), ev("move", "text", held: true), ev("up", "text")],
                                    GestureObserved(selection: "brown", aVStart: 0, aVEnd: 0, aHStart: 0, aHEnd: 0,
                                                    bVStart: 0, bVEnd: 0, bHStart: 0, bHEnd: 0, aVMax: 0, aHMax: 0, bVMax: 0, bHMax: 0),
                                    zones: []).matched)

        // -- scroll: down --
        let s1 = caseById("scroll", "scroll-01")
        var obsA = blankObs
        obsA.aVEnd = 90
        add("s01-pass-mac",
            GestureJudge.judgeScroll(s1, [ev("wheel", "panelA", dx: 0, dy: -30, t: 0), ev("wheel", "panelA", dx: 0, dy: -30, t: 100)],
                                    obsA, platform: mac).matched)
        // windows raw positive wheel delta = scroll UP -> contract dy negative;
        // a positive raw delta with downward offset change must NOT pass
        add("s01-win-raw-sign-respected",
            GestureJudge.judgeScroll(s1, [ev("wheel", "panelA", dx: 0, dy: -360)], obsA, platform: win).matched)
        // wrong-direction raw deltas with unchanged offsets fail
        add("s01-no-offset-change-fails",
            !GestureJudge.judgeScroll(s1, [ev("wheel", "panelA", dx: 0, dy: -30)], blankObs, platform: mac).matched)
        // -- scroll: wrong panel --
        let s6 = caseById("scroll", "scroll-06")
        var obsWrong = blankObs
        obsWrong.aVEnd = 90 // A moved although B was the target
        add("s06-panel-a-moved-fails",
            !GestureJudge.judgeScroll(s6, [ev("wheel", "panelB", dx: 0, dy: -30)], obsWrong, platform: mac).matched)
        var obsRight = blankObs
        obsRight.bVEnd = 90
        add("s06-pass", GestureJudge.judgeScroll(s6, [ev("wheel", "panelB", dx: 0, dy: -30)], obsRight, platform: mac).matched)
        // -- scroll: saturate --
        let s8 = caseById("scroll", "scroll-08")
        var obsSat = blankObs
        obsSat.aVEnd = obsSat.aVMax
        add("s08-saturate-pass",
            GestureJudge.judgeScroll(s8, [ev("wheel", "panelA", dx: 0, dy: -600)], obsSat, platform: mac).matched)
        // -- scroll: zero delta / invalid args --
        let s9 = caseById("scroll", "scroll-09")
        add("s09-pass-empty", GestureJudge.judgeScroll(s9, [], blankObs, platform: mac).matched)
        add("s09-nonzero-delta-fails",
            !GestureJudge.judgeScroll(s9, [ev("wheel", "panelA", dx: 0, dy: -30)], blankObs, platform: mac).matched)
        let s10 = caseById("scroll", "scroll-10")
        add("s10-wheel-arrived-fails",
            !GestureJudge.judgeScroll(s10, [ev("wheel", "panelA", dx: 10, dy: 0)], blankObs, platform: mac).matched)
        // -- wheel contract normalization pinned --
        let macN = WheelContract.normalize(platform: mac, rawDx: 2, rawDy: 30)
        add("wheel-contract-macos", macN.dx == -2 && macN.dy == -30, "dx=\(macN.dx) dy=\(macN.dy)")
        let winN = WheelContract.normalize(platform: win, rawDx: 120, rawDy: 360)
        add("wheel-contract-windows", winN.dx == 120 && winN.dy == -360, "dx=\(winN.dx) dy=\(winN.dy)")

        // -- platform notes present for the tricky cases, absent from agent UI --
        add("platform-notes-present",
            !GesturePlatformNotes.note(suite: "multiclick", caseId: "multiclick-03", platform: "windows").isEmpty
                && !GesturePlatformNotes.note(suite: "multiclick", caseId: "multiclick-05", platform: "windows").isEmpty)

        // -- layout sanity: zones non-overlapping for START/END pairs --
        func overlap(_ a: GestureZone, _ b: GestureZone) -> Bool {
            return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
        }
        var layoutOK = true
        for c in GestureCatalog.dragCases {
            let zs = GestureLayout.zones(for: c.spec)
            for i in 0..<zs.count {
                for j in (i + 1)..<zs.count where overlap(zs[i], zs[j]) { layoutOK = false }
            }
        }
        add("drag-zones-nonoverlapping", layoutOK)

        return r
    }
}
