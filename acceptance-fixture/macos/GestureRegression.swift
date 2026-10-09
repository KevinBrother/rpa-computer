// Pure synthetic regression cases. Execution belongs to CC+GLM, not the implementer.
import Foundation

enum GestureRegression {
    static func run() -> [SelfTestResult] {
        var r: [SelfTestResult] = []
        func add(_ name: String, _ ok: Bool) {
            r.append(SelfTestResult(name: "gesture-regression-" + name, passed: ok, detail: "synthetic; not GUI evidence"))
        }
        func e(_ kind: String, _ area: String = "target", _ n: Int = 1, _ t: Double = 0,
               _ x: Double = 400, _ y: Double = 150, _ held: Bool = false, _ dbl: Bool = false) -> GestureEvent {
            GestureEvent(kind: kind, button: "left", nativeCount: n, doubleClickMsg: dbl,
                         area: area, x: x, y: y, dx: 0, dy: 0, held: held, tMs: t)
        }
        let obs = GestureObserved(selection: "", aVStart: 0, aVEnd: 0, aHStart: 0, aHEnd: 0,
                                  bVStart: 0, bVEnd: 0, bHStart: 0, bHEnd: 0,
                                  aVMax: 100, aHMax: 100, bVMax: 100, bHMax: 100)
        let one = GestureCatalog.multiclickCases[0], triple = GestureCatalog.multiclickCases[2]
        add("unreleased-click-fails", !GestureJudge.judgeMulticlick(one, [e("down")], obs, platform: "macos").matched)
        let winTriple = [e("down", "target", 1, 0), e("up", "target", 1, 10),
                         e("down", "target", 2, 80, 400, 150, false, true), e("up", "target", 2, 90),
                         e("down", "target", 1, 160), e("up", "target", 1, 170)]
        add("windows-third-down-one-not-three", GestureJudge.judgeMulticlick(triple, winTriple, obs, platform: "windows").matched)
        add("windows-clicks2-not-two-downs", !GestureJudge.judgeMulticlick(triple, Array(winTriple.prefix(4)), obs, platform: "windows").matched)
        var late = winTriple; late[4].tMs = 2100; late[5].tMs = 2120
        add("late-third-click-fails", !GestureJudge.judgeMulticlick(triple, late, obs, platform: "windows").matched)
        let slow = GestureCatalog.multiclickCases[8]
        add("slow-must-wait-two-seconds", !GestureJudge.judgeMulticlick(slow,
            [e("down"), e("up"), e("down", "target", 1, 1800), e("up", "target", 1, 1810)], obs, platform: "macos").matched)
        let d = GestureCatalog.dragCases[0], z = GestureLayout.zones(for: d.spec)
        let start = e("down", "canvas", 1, 0, 110, 130, true)
        let move = e("move", "canvas", 1, 50, 400, 130, true)
        let end = e("up", "canvas", 1, 100, 690, 130)
        add("coalesced-one-motion-valid", GestureJudge.judgeDrag(d, [start, move, end], obs, zones: z).matched)
        add("motion-before-down-fails", !GestureJudge.judgeDrag(d, [move, start, end], obs, zones: z).matched)
        add("endpoint-only-fails", !GestureJudge.judgeDrag(d, [start, end], obs, zones: z).matched)
        add("post-release-held-motion-fails", !GestureJudge.judgeDrag(d, [start, move, end, move], obs, zones: z).matched)
        var selected = obs; selected.selection = "quick brown"
        add("selection-without-release-fails", !GestureJudge.judgeDrag(GestureCatalog.dragCases[8],
            [e("down", "text"), e("move", "text", 1, 20, 450, 150, true)], selected, zones: []).matched)
        var atBottom = obs; atBottom.aVStart = 100; atBottom.aVEnd = 100
        let wheel = GestureEvent(kind: "wheel", button: "other", nativeCount: 0, doubleClickMsg: false,
                                 area: "panelA", x: 10, y: 10, dx: 0, dy: -30, held: false, tMs: 10)
        add("saturation-with-no-displacement-fails", !GestureJudge.judgeScroll(GestureCatalog.scrollCases[7], [wheel], atBottom, platform: "macos").matched)
        var wrongPanel = obs; wrongPanel.aVEnd = 90; wrongPanel.bHEnd = 5
        add("adjacent-horizontal-panel-change-fails", !GestureJudge.judgeScroll(GestureCatalog.scrollCases[0], [wheel], wrongPanel, platform: "macos").matched)
        for c in GestureCatalog.dragCases.prefix(4) {
            let zs = GestureLayout.zones(for: c.spec)
            add(c.id + "-direction-valid", GestureJudge.judgeDrag(c,
                [e("down", "canvas", 1, 0, zs[0].x+50, zs[0].y+30, true),
                 e("move", "canvas", 1, 50, (zs[0].x+zs[1].x)/2+50, (zs[0].y+zs[1].y)/2+30, true),
                 e("up", "canvas", 1, 100, zs[1].x+50, zs[1].y+30)], obs, zones: zs).matched)
        }
        let d6 = GestureCatalog.dragCases[5], z6 = GestureLayout.zones(for: d6.spec)
        add("short-coalesced-one-motion", GestureJudge.judgeDrag(d6,
            [e("down", "canvas", 1, 0, 350, 140, true), e("move", "canvas", 1, 30, 420, 140, true), e("up", "canvas", 1, 60, 490, 140)], obs, zones: z6).matched)
        let d8 = GestureCatalog.dragCases[7], z8 = GestureLayout.zones(for: d8.spec)
        add("edge-release-valid", GestureJudge.judgeDrag(d8,
            [e("down", "canvas", 1, 0, 130, 140, true), e("move", "canvas", 1, 30, 400, 140, true), e("up", "canvas", 1, 60, 610, 140)], obs, zones: z8).matched)
        add("center-release-not-edge", !GestureJudge.judgeDrag(d8,
            [e("down", "canvas", 1, 0, 130, 140, true), e("move", "canvas", 1, 30, 400, 140, true), e("up", "canvas", 1, 60, 690, 155)], obs, zones: z8).matched)
        let d7 = GestureCatalog.dragCases[6], z7 = GestureLayout.zones(for: d7.spec)
        let longDown = e("down", "canvas", 1, 0, 80, 140, true)
        let longMove = e("move", "canvas", 1, 400, 400, 140, true)
        add("long-duration-valid", GestureJudge.judgeDrag(d7, [longDown, longMove, e("up", "canvas", 1, 800, 760, 140)], obs, zones: z7).matched)
        add("long-too-short-fails", !GestureJudge.judgeDrag(d7, [longDown, e("move", "canvas", 1, 100, 400, 140, true), e("up", "canvas", 1, 200, 760, 140)], obs, zones: z7).matched)
        let d5 = GestureCatalog.dragCases[4], z5 = GestureLayout.zones(for: d5.spec)
        add("coalesced-waypoint-segment-valid", GestureJudge.judgeDrag(d5,
            [e("down", "canvas", 1, 0, 110, 155, true), e("move", "canvas", 1, 30, 350, 60, true),
             e("move", "canvas", 1, 60, 500, 60, true), e("up", "canvas", 1, 100, 750, 235)], obs, zones: z5).matched)
        let zeroWheel = GestureEvent(kind: "wheel", button: "other", nativeCount: 0, doubleClickMsg: false, area: "panelA", x: 0, y: 0, dx: 0, dy: 0, held: false, tMs: 0)
        add("zero-native-event-not-zero-injection", !GestureJudge.judgeScroll(GestureCatalog.scrollCases[8], [zeroWheel], obs, platform: "macos").matched)
        if let json = GestureExporter.manifestJSON(), let data = json.data(using: .utf8),
           let root = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any], let groups = root["suites"] as? [String: [String: Any]] {
            add("export-retains-63-cases", groups.values.reduce(0) { $0 + ($1["total"] as? Int ?? 0) } == 63 && groups.count == 8)
        } else { add("export-retains-63-cases", false) }
        add("json-escaping", (try? JSONSerialization.data(withJSONObject: ["v": "\"\\\n\t测试"])) != nil)
        return r
    }
}
