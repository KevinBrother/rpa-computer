//
//  Gesture suites: pure catalog + event recorder + judges (no AppKit, no UI, no I/O).
//  Shared contract with windows/GestureCases.cs; spec strings must match
//  byte-for-byte at the UTF-16 level (verify with tools/check-cases-parity.py).
//
//  Suites (see docs/computer-use-acceptance-cases.md §3):
//    multiclick  10 cases — native multi-click counting / selection / resets
//    drag        10 cases — press->held motion->release semantics
//    scroll      10 cases — vertical/horizontal/dual-axis panel scrolling
//
//  Event contract: the fixture records RAW native fields verbatim (Mac
//  NSEvent.clickCount / scrollWheel deltas; Windows MouseEventArgs.Clicks /
//  WM_*DBLCLK / WM_MOUSEWHEEL deltas) AND a normalized contract value
//  (positive = right/down). Raw actuals are never normalized away.
//

import Foundation

// MARK: - Gesture event model (platform-independent; fields raw as received)

struct GestureEvent {
    var kind: String        // "down" | "up" | "move" | "wheel"
    var button: String      // "left" | "right" | "other"
    var nativeCount: Int    // Mac clickCount / Windows Clicks (raw, NOT normalized)
    var doubleClickMsg: Bool // Windows: event came from WM_xBUTTONDBLCLK
    var area: String        // "target" | "zoneA" | "zoneB" | "text" | "canvas" | "outside"
    var x: Double
    var y: Double
    var dx: Double          // raw native horizontal wheel delta (0 for click events)
    var dy: Double          // raw native vertical wheel delta (0 for click events)
    var held: Bool          // view-supplied: was the drag button held at this event
    var tMs: Double         // ms since trial start
    var nativeTimestamp: Double = 0
    var nativeType: Int = 0
    var legacyDx: Double = 0, legacyDy: Double = 0
    var precise: Bool = false, inverted: Bool = false
    var phase: UInt = 0, momentumPhase: UInt = 0
}

// MARK: - Observed state supplied by the view at judge time

struct GestureObserved {
    var selection: String          // selected text of the fixture's own text control ("" if none)
    var aVStart: Int, aVEnd: Int   // Panel A vertical offset at trial start / at check
    var aHStart: Int, aHEnd: Int
    var bVStart: Int, bVEnd: Int
    var bHStart: Int, bHEnd: Int
    var aVMax: Int, aHMax: Int, bVMax: Int, bHMax: Int
}

// MARK: - Case model

struct GestureCase {
    let id: String
    let spec: String      // "k=v;k=v" — literal shared across platforms (parity-checked)
    let instruction: String // agent-facing text (same on both platforms)

    init(id: String, spec: String, instruction: String) {
        self.id = id
        self.spec = spec
        self.instruction = instruction
    }

    func field(_ key: String) -> String? {
        for pair in spec.split(separator: ";") {
            let kv = pair.split(separator: "=", maxSplits: 1)
            if kv.count == 2 && kv[0] == key { return String(kv[1]) }
        }
        return nil
    }
    func intField(_ key: String) -> Int? { field(key).flatMap { Int($0) } }
}

// MARK: - Gesture catalogs (spec strings are the parity contract)

enum GestureCatalog {
    // Sentence used by selection cases (fixed, both platforms).
    static let selectWordSentence = "alpha beta gamma delta epsilon"
    static let dragSelectSentence = "the quick brown fox jumps over the lazy dog"

    // multiclick-01..10 (docs §3). flow names map to docs items 1..10.
    static let multiclickCases: [GestureCase] = [
        GestureCase(id: "multiclick-01", spec: "flow=single;expect_count=1",
                    instruction: "Click the big TARGET rectangle exactly once (a single left click)."),
        GestureCase(id: "multiclick-02", spec: "flow=multi;expect_count=2",
                    instruction: "Double-click the big TARGET rectangle (two rapid left clicks)."),
        GestureCase(id: "multiclick-03", spec: "flow=multi;expect_count=3",
                    instruction: "Triple-click the big TARGET rectangle (three rapid left clicks)."),
        GestureCase(id: "multiclick-04", spec: "flow=select_word",
                    instruction: "Double-click the word alpha in the sentence below to select it."),
        GestureCase(id: "multiclick-05", spec: "flow=select_line",
                    instruction: "Triple-click the sentence below to select the whole line (per this platform's convention)."),
        GestureCase(id: "multiclick-06", spec: "flow=two_targets",
                    instruction: "Click zone A once, then click zone B once (two separate single clicks)."),
        GestureCase(id: "multiclick-07", spec: "flow=two_positions",
                    instruction: "Click once near the LEFT edge of the TARGET, then once near the RIGHT edge (far apart)."),
        GestureCase(id: "multiclick-08", spec: "flow=right_between",
                    instruction: "Left-click the TARGET once, then RIGHT-click it once, then left-click it once."),
        GestureCase(id: "multiclick-09", spec: "flow=slow_two",
                    instruction: "Click the TARGET once, wait at least 2 seconds, then click it once again."),
        GestureCase(id: "multiclick-10", spec: "flow=invalid_count",
                    instruction: "Attempt a click with an INVALID count of 0 using your click tool. The app must receive no clicks at all."),
    ]

    // drag-01..10
    static let dragCases: [GestureCase] = [
        GestureCase(id: "drag-01", spec: "axis=h;dir=lr",
                    instruction: "Press and hold the left button inside zone START, drag horizontally to zone END, release inside END."),
        GestureCase(id: "drag-02", spec: "axis=h;dir=rl",
                    instruction: "Press and hold inside zone START (right side), drag horizontally left to zone END, release inside END."),
        GestureCase(id: "drag-03", spec: "axis=v;dir=tb",
                    instruction: "Press and hold inside zone START (top), drag vertically down to zone END, release inside END."),
        GestureCase(id: "drag-04", spec: "axis=v;dir=bt",
                    instruction: "Press and hold inside zone START (bottom), drag vertically up to zone END, release inside END."),
        GestureCase(id: "drag-05", spec: "path=polyline",
                    instruction: "Press inside START, drag through WAYPOINT, continue to zone END, release inside END."),
        GestureCase(id: "drag-06", spec: "span=short",
                    instruction: "Drag from START to END (short distance) with the button held the whole time."),
        GestureCase(id: "drag-07", spec: "span=long;min_ms=750",
                    instruction: "Drag from START to END across the canvas, keeping the button held for at least 0.75 seconds."),
        GestureCase(id: "drag-08", spec: "release=inner_edge",
                    instruction: "Press inside START, drag to the END zone, and release INSIDE it but near its edge (not at the center)."),
        GestureCase(id: "drag-09", spec: "flow=text_select",
                    instruction: "Drag-select the words quick brown in the sentence below (press before quick, release after brown)."),
        GestureCase(id: "drag-10", spec: "path=curve",
                    instruction: "Press inside START, draw a smooth curved path (direction changes at least twice) and release inside END."),
    ]

    // scroll-01..10
    static let scrollCases: [GestureCase] = [
        GestureCase(id: "scroll-01", spec: "panel=A;axis=v;dir=down;ticks=3",
                    instruction: "Scroll Panel A DOWN by about 3 wheel ticks."),
        GestureCase(id: "scroll-02", spec: "panel=A;axis=v;dir=up;ticks=3",
                    instruction: "Scroll Panel A UP by about 3 wheel ticks."),
        GestureCase(id: "scroll-03", spec: "panel=A;axis=h;dir=right;ticks=3",
                    instruction: "Scroll Panel A to the RIGHT by about 3 wheel ticks."),
        GestureCase(id: "scroll-04", spec: "panel=A;axis=h;dir=left;ticks=3",
                    instruction: "Scroll Panel A to the LEFT by about 3 wheel ticks."),
        GestureCase(id: "scroll-05", spec: "panel=A;axis=both;dir=down;dir2=right;ticks=3",
                    instruction: "Scroll Panel A DOWN and to the RIGHT at the same time (about 3 ticks each)."),
        GestureCase(id: "scroll-06", spec: "panel=B;axis=v;dir=down;ticks=3",
                    instruction: "Scroll Panel B DOWN by about 3 wheel ticks. Panel A must not move."),
        GestureCase(id: "scroll-07", spec: "panel=A;axis=v;dir=down;ticks=1",
                    instruction: "Scroll Panel A DOWN by a small amount (about 1 wheel tick)."),
        GestureCase(id: "scroll-08", spec: "panel=A;axis=v;dir=down;ticks=20;flow=saturate",
                    instruction: "Scroll Panel A DOWN by many ticks (about 20) until it saturates at the bottom."),
        GestureCase(id: "scroll-09", spec: "flow=zero_delta",
                    instruction: "Attempt a scroll with a ZERO delta (0, 0). The page must not scroll."),
        GestureCase(id: "scroll-10", spec: "flow=invalid_args",
                    instruction: "Attempt a scroll with INVALID arguments (e.g. an unknown direction). No wheel event may arrive."),
    ]

    static func cases(forSuite suite: String) -> [GestureCase]? {
        switch suite {
        case "multiclick": return multiclickCases
        case "drag": return dragCases
        case "scroll": return scrollCases
        default: return nil
        }
    }

    static func isGesture(suite: String) -> Bool {
        return cases(forSuite: suite) != nil
    }
}

// MARK: - Layout model (pure; the views compute concrete rects from it)

struct GestureZone {
    var name: String      // "START" | "END" | "WAYPOINT" | "TARGET" | "zoneA" | "zoneB"
    var x: Double, y: Double, w: Double, h: Double
}

enum GestureLayout {
    // Canvas is 860 x 300 (same on both platforms; view height may differ slightly).
    static func zones(for spec: String) -> [GestureZone] {
        func zone(_ name: String, _ x: Double, _ y: Double, _ w: Double, _ h: Double) -> GestureZone {
            return GestureZone(name: name, x: x, y: y, w: w, h: h)
        }
        let axis = field(spec, "axis")
        let dir = field(spec, "dir")
        let span = field(spec, "span")
        let path = field(spec, "path")
        let flow = field(spec, "flow")
        let release = field(spec, "release")
        if axis == "h" {
            if dir == "rl" {
                return [zone("START", 640, 100, 160, 100), zone("END", 60, 100, 160, 100)]
            }
            return [zone("START", 60, 100, 160, 100), zone("END", 640, 100, 160, 100)]
        }
        if axis == "v" {
            if dir == "bt" {
                return [zone("START", 350, 180, 160, 100), zone("END", 350, 20, 160, 100)]
            }
            return [zone("START", 350, 20, 160, 100), zone("END", 350, 180, 160, 100)]
        }
        if path == "polyline" {
            return [zone("START", 40, 110, 140, 90), zone("WAYPOINT", 360, 20, 140, 80), zone("END", 680, 190, 140, 90)]
        }
        if path == "curve" {
            return [zone("START", 40, 200, 130, 80), zone("END", 690, 20, 130, 80)]
        }
        if span == "short" {
            return [zone("START", 300, 110, 120, 80), zone("END", 440, 110, 120, 80)]
        }
        if span == "long" {
            return [zone("START", 30, 110, 120, 80), zone("END", 710, 110, 120, 80)]
        }
        if release == "inner_edge" {
            return [zone("START", 80, 110, 140, 90), zone("END", 600, 110, 180, 90)]
        }
        if flow == "text_select" { return [] }
        return [zone("TARGET", 280, 90, 300, 120)]
    }

    static func field(_ spec: String, _ key: String) -> String {
        for pair in spec.split(separator: ";") {
            let kv = pair.split(separator: "=", maxSplits: 1)
            if kv.count == 2 && kv[0] == key { return String(kv[1]) }
        }
        return ""
    }
}

// MARK: - Wheel delta normalization (contract: positive = content moves down/right)
//
// Raw native deltas are logged verbatim in evidence. The contract value used
// by judges is defined EXPLICITLY per platform:
//   macOS:  NSEvent.scrollWheel deltaY > 0 scrolls toward the TOP of the
//           document, deltaX > 0 toward the LEFT. contract = -raw.
//   Windows: WM_MOUSEWHEEL positive delta = wheel forward = scroll UP;
//           WM_MOUSEHWHEEL positive = RIGHT. contract dy = -raw, dx = +raw.
// Per-case platform expected differences are surfaced, never hidden.

struct WheelContract {
    var dx: Double  // positive = content moves right
    var dy: Double  // positive = content moves down

    static func normalize(platform: String, rawDx: Double, rawDy: Double) -> WheelContract {
        if platform == "macos" {
            return WheelContract(dx: -rawDx, dy: -rawDy)
        }
        // windows
        return WheelContract(dx: rawDx, dy: -rawDy)
    }
}

