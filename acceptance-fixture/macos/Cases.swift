//
//  Layered case catalog + contract logic (PURE — no AppKit, no UI, no I/O).
//  Shared contract with windows/Cases.cs; keep both byte-for-byte equivalent
//  at the UTF-16 level. Verify with tools/check-cases-parity.py.
//
//  Suites:
//    baseline     10 clear short texts — pure-vision copy target
//    punctuation   6 full/half-width punctuation mixes — pure vision
//    emoji         6 very short texts, one common emoji each — pure vision
//    known-input  10 explicit payloads handed to the agent as text
//    legacy        original single sample (default when --suite omitted)
//
//  expected_text is the GUI-check target: payload with CRLF normalized to LF
//  (runtime contract). matched is ALWAYS exact UTF-16 code-unit comparison —
//  never Swift String == (canonical equivalence would accept NFC vs NFD).
//

import Foundation

// MARK: - Suite

enum Suite: String {
    case legacy
    case baseline
    case punctuation
    case emoji
    case knownInput = "known-input"

    // Fixed order for export manifests / self-test reporting.
    static let ordered: [Suite] = [.baseline, .punctuation, .emoji, .knownInput, .legacy]
}

// MARK: - Case model

struct FixtureCase {
    let id: String
    let payload: String
    // GUI-check target: payload normalized CRLF -> LF (runtime contract).
    // Derived here so both platforms can never diverge on it.
    let expectedText: String

    init(id: String, payload: String) {
        self.id = id
        self.payload = payload
        self.expectedText = CaseContract.normalizeCRLF(payload)
    }
}

// MARK: - Contract primitives

enum CaseContract {
    static func normalizeCRLF(_ s: String) -> String {
        s.replacingOccurrences(of: "\r\n", with: "\n")
    }

    // Exact UTF-16 code-unit comparison. NEVER use String == here: Swift
    // String equality is canonical-equivalent and would treat NFC == NFD.
    static func utf16ExactMatch(_ actual: String, _ expected: String) -> Bool {
        Array(actual.utf16) == Array(expected.utf16)
    }

    static func utf16Hex(_ s: String) -> String {
        s.utf16.map { String(format: "%04X", $0) }.joined(separator: " ")
    }
}

// MARK: - Catalog

enum CaseCatalog {
    // Historical legacy sample — MUST NOT change (regression-checked below).
    static let legacySample = "你好，世界 🌍 | Unicode 测试 ✅ | 本文档共 3 段（含本段）"

    // baseline: 10 short, clear, distinct texts; common Chinese/ASCII/digits;
    // no emoji, no special spaces, no full/half-width punctuation ambiguity;
    // single U+0020 as the only separator; lengths include 12 and 34 UTF-16
    // units; max <= 40. For screenshot-only copying.
    static let baselineCases: [FixtureCase] = [
        FixtureCase(id: "baseline-01", payload: "computer use acceptance test"),
        FixtureCase(id: "baseline-02", payload: "原生输入验收 2026"),
        FixtureCase(id: "baseline-03", payload: "键盘鼠标截图点击输入检查"),
        FixtureCase(id: "baseline-04", payload: "RPA flow runs every morning"),
        FixtureCase(id: "baseline-05", payload: "这是长度为三十四个字符的普通中文验收样例文字全部常见字形逐字核对无误"),
        FixtureCase(id: "baseline-06", payload: "verify keystroke injection accuracy"),
        FixtureCase(id: "baseline-07", payload: "第二组普通文字抄写检查通过"),
        FixtureCase(id: "baseline-08", payload: "视觉验收普通文字样例抄写"),
        FixtureCase(id: "baseline-09", payload: "screen 1080p resolution check"),
        FixtureCase(id: "baseline-10", payload: "输入延迟与准确率统计报告"),
    ]

    // punctuation: 6 short texts, each isolating one full/half-width pair;
    // never mixed with emoji. Special spaces are EXCLUDED here (visually
    // indistinguishable -> belongs to known-input, not the vision matrix).
    static let punctuationCases: [FixtureCase] = [
        FixtureCase(id: "punctuation-01", payload: "括号测试（全角内容）结束"),
        FixtureCase(id: "punctuation-02", payload: "code (alpha) beta 42 done"),
        FixtureCase(id: "punctuation-03", payload: "第一项，第二项，第三项完毕"),
        FixtureCase(id: "punctuation-04", payload: "one, two, three, four, five"),
        FixtureCase(id: "punctuation-05", payload: "管道 | 分隔 | 符号 | 测试"),
        FixtureCase(id: "punctuation-06", payload: "引号 “全角” 与 \"半角\" 对比"),
    ]

    // emoji: 6 very short texts, exactly one common distinct emoji each.
    // WinForms may render these monochrome (Segoe UI Emoji) — a screenshot
    // can therefore NOT always uniquely determine the code point on Windows.
    static let emojiCases: [FixtureCase] = [
        FixtureCase(id: "emoji-01", payload: "状态 ✅"),
        FixtureCase(id: "emoji-02", payload: "地球 🌍"),
        FixtureCase(id: "emoji-03", payload: "火箭 🚀"),
        FixtureCase(id: "emoji-04", payload: "微笑 😀"),
        FixtureCase(id: "emoji-05", payload: "铃铛 🔔"),
        FixtureCase(id: "emoji-06", payload: "书本 📚"),
    ]

    // known-input: 10 explicit payloads handed to the agent as ORIGINAL TEXT
    // (never visual inference). task_payload vs expected_text are separate:
    // expected is the CRLF->LF normalized check target.
    static let knownInputCases: [FixtureCase] = [
        FixtureCase(id: "known-01", payload: "Order 6642 shipped via DHL Express"),
        FixtureCase(id: "known-02", payload: "键盘输入验收测试样例文本"),                       // 12 UTF-16 units
        FixtureCase(id: "known-03", payload: "分布式原生输入验收涵盖中文长文本逐字精确比对三十四字样例内容全部完毕"), // 34 UTF-16 units
        FixtureCase(id: "known-04", payload: legacySample),                                  // historical 38-scalar sample
        FixtureCase(id: "known-05", payload: "注意：括号（全角）与问号？感叹号！"),
        FixtureCase(id: "known-06", payload: "NBSP\u{00A0}分隔\u{00A0}样例"),
        FixtureCase(id: "known-07", payload: "🦄 独角兽 non-BMP 样例"),
        FixtureCase(id: "known-08", payload: "resume\u{0301} 和 cafe\u{0301} 组合音符样例"), // e + combining acute, deliberately NOT precomposed
        FixtureCase(id: "known-09", payload: "第一行\r\n第二行\r\n第三行"),                    // CRLF; expected uses LF only
        FixtureCase(id: "known-10", payload: "姓名\t部门\t工号"),
    ]

    static func cases(for suite: Suite) -> [FixtureCase] {
        switch suite {
        case .legacy:     return [FixtureCase(id: "legacy-sample", payload: legacySample)]
        case .baseline:   return baselineCases
        case .punctuation: return punctuationCases
        case .emoji:      return emojiCases
        case .knownInput: return knownInputCases
        }
    }

    static func total(for suite: Suite) -> Int { cases(for: suite).count }
}

// MARK: - Window placement on the CG main display (pure)

struct ScreenInfo {
    let displayID: Int
    let x: Double
    let y: Double
    let w: Double
    let h: Double
}

enum ScreenPlacement {
    struct PlacedFrame {
        let x: Double
        let y: Double
        let w: Double
        let h: Double
    }

    // Center the window on the screen whose displayID matches CGMainDisplayID
    // (AppKit global coordinates, origin bottom-left). Returns nil — caller
    // must fail loudly — when the main display has no matching NSScreen or is
    // too small to hold the full window frame (no silent off-screen fallback,
    // no clipping).
    static func chooseFrame(screens: [ScreenInfo], mainDisplayID: Int,
                            windowWidth: Double, windowHeight: Double) -> PlacedFrame? {
        guard let s = screens.first(where: { $0.displayID == mainDisplayID }) else { return nil }
        guard s.w >= windowWidth && s.h >= windowHeight else { return nil }
        return PlacedFrame(x: s.x + (s.w - windowWidth) / 2,
                           y: s.y + (s.h - windowHeight) / 2,
                           w: windowWidth, h: windowHeight)
    }

    static func fullyInside(_ f: PlacedFrame, _ s: ScreenInfo) -> Bool {
        f.x >= s.x && f.y >= s.y && f.x + f.w <= s.x + s.w && f.y + f.h <= s.y + s.h
    }
}

// MARK: - Self-test (pure; no window, no screenshot, no input)

struct SelfTestResult {
    let name: String
    let passed: Bool
    let detail: String
}

enum SelfTest {
    static func run() -> [SelfTestResult] {
        var r: [SelfTestResult] = []
        func add(_ name: String, _ ok: Bool, _ detail: String = "") {
            r.append(SelfTestResult(name: name, passed: ok, detail: detail))
        }

        // -- case counts --
        for (suite, expectedCount) in [(Suite.baseline, 10), (.punctuation, 6), (.emoji, 6), (.knownInput, 10)] {
            let got = CaseCatalog.total(for: suite)
            add("count-\(suite.rawValue)", got == expectedCount, "expected \(expectedCount), got \(got)")
        }
        add("count-legacy", CaseCatalog.total(for: .legacy) == 1)

        // -- unique IDs / nonempty across ALL suites --
        let all = Suite.ordered.flatMap { CaseCatalog.cases(for: $0) }
        let ids = all.map { $0.id }
        add("ids-unique", Set(ids).count == ids.count,
            ids.count - Set(ids).count == 0 ? "all distinct" : "duplicates present")
        add("payloads-nonempty", all.allSatisfy { !$0.payload.isEmpty })
        add("expected-nonempty", all.allSatisfy { !$0.expectedText.isEmpty })

        // -- normalization contract: expected == normalizeCRLF(payload) everywhere;
        //    payload == expected except the CRLF case(s) --
        let normOk = all.allSatisfy { CaseContract.normalizeCRLF($0.payload) == $0.expectedText }
        add("crlf-normalization-holds", normOk)
        let crlfCase = CaseCatalog.knownInputCases.first { $0.id == "known-09" }
        add("crlf-case-payload-vs-expected-differ",
            crlfCase.map { $0.payload.contains("\r\n") && !$0.expectedText.contains("\r") && $0.payload != $0.expectedText } == true)
        add("crlf-case-expected-has-LF", crlfCase.map { $0.expectedText.contains("\n") } == true)
        let nonCrlfAllEqual = all.allSatisfy { $0.payload.contains("\r\n") || $0.payload == $0.expectedText }
        add("non-crlf-payload-equals-expected", nonCrlfAllEqual)

        // -- known-input feature coverage --
        func byId(_ id: String) -> FixtureCase? { CaseCatalog.knownInputCases.first { $0.id == id } }
        add("known-zh12-length", byId("known-02").map { $0.payload.utf16.count == 12 } == true)
        add("known-zh34-length", byId("known-03").map { $0.payload.utf16.count == 34 } == true)
        add("known-legacy-38-scalars", CaseCatalog.legacySample.unicodeScalars.count == 38,
            "got \(CaseCatalog.legacySample.unicodeScalars.count)")
        add("known-nbsp-present", byId("known-06").map { $0.payload.unicodeScalars.contains("\u{00A0}") } == true)
        add("known-combining-present",
            byId("known-08").map { p in
                p.payload.unicodeScalars.contains("\u{0301}") && !p.payload.unicodeScalars.contains("\u{00E9}")
            } == true)
        add("known-nonbmp-present", byId("known-07").map { $0.payload.unicodeScalars.contains { $0.value > 0xFFFF } } == true)
        add("known-tab-present", byId("known-10").map { $0.payload.contains("\t") } == true)
        add("known-ascii-present", byId("known-01").map { $0.payload.allSatisfy { $0.asciiValue != nil } } == true)

        // -- legacy retained exactly (literal spelled out here to catch edits) --
        add("legacy-sample-retained",
            CaseCatalog.legacySample == "你好，世界 🌍 | Unicode 测试 ✅ | 本文档共 3 段（含本段）")

        // -- baseline constraints --
        let baseline = CaseCatalog.baselineCases
        let maxLen = baseline.map { $0.payload.utf16.count }.max() ?? 0
        add("baseline-maxlen-40", maxLen <= 40, "max \(maxLen)")
        let onlyPlainSpaces = baseline.allSatisfy { c in
            c.payload.unicodeScalars.allSatisfy { $0 == " " || !$0.properties.isWhitespace }
        }
        add("baseline-only-plain-spaces", onlyPlainSpaces)
        let noEmojiNoSymbols = baseline.allSatisfy { c in
            c.payload.unicodeScalars.allSatisfy { !($0.value >= 0x1F000 || (0x2190...0x2BFF).contains($0.value)) }
        }
        add("baseline-no-emoji-or-symbols", noEmojiNoSymbols)
        let lengths = Set(baseline.map { $0.payload.utf16.count })
        add("baseline-has-12-and-34", lengths.contains(12) && lengths.contains(34), "lengths \(lengths.sorted())")
        let distinctBaseline = Set(baseline.map { Array($0.payload.utf16) }).count == baseline.count
        add("baseline-all-distinct", distinctBaseline)

        // -- punctuation: no emoji / variation selectors --
        let punctClean = CaseCatalog.punctuationCases.allSatisfy { c in
            c.payload.unicodeScalars.allSatisfy { $0.value < 0x1F000 && $0.value != 0xFE0F }
        }
        add("punctuation-no-emoji", punctClean)

        // -- emoji: one known distinct emoji scalar per case --
        let emojiScalars: [UInt32] = [0x2705, 0x1F30D, 0x1F680, 0x1F600, 0x1F514, 0x1F4DA]
        let found = CaseCatalog.emojiCases.map { c -> UInt32? in
            c.payload.unicodeScalars.first { emojiScalars.contains($0.value) }.map { $0.value }
        }
        add("emoji-one-known-each", found.allSatisfy { $0 != nil } && Set(found.compactMap { $0 }).count == 6,
            "found \(found.map { $0.map { String($0, radix: 16) } ?? "nil" })")

        // -- suite name parsing --
        for s in Suite.ordered {
            add("suite-parse-\(s.rawValue)", Suite(rawValue: s.rawValue) == s)
        }
        add("suite-parse-rejects-unknown", Suite(rawValue: "foo") == nil && Suite(rawValue: "Legacy") == nil)

        // -- UTF-16 exact comparison semantics --
        add("utf16-exact-accept-identical", CaseContract.utf16ExactMatch("abc", "abc"))
        add("utf16-exact-accept-emoji", CaseContract.utf16ExactMatch("状态 ✅", "状态 ✅"))
        add("utf16-exact-rejects-nfc-nfd",
            !CaseContract.utf16ExactMatch("caf\u{00E9}", "cafe\u{0301}"))
        add("utf16-exact-rejects-trailing-space",
            !CaseContract.utf16ExactMatch("abc", "abc "))
        add("utf16-exact-rejects-crlf-vs-lf",
            !CaseContract.utf16ExactMatch("a\r\nb", "a\nb"))
        add("utf16-exact-rejects-nbsp-vs-space",
            !CaseContract.utf16ExactMatch("a\u{00A0}b", "a b"))

        // -- pure window placement (main-display centering) --
        let main = ScreenInfo(displayID: 2, x: 0, y: 0, w: 1920, h: 1080)
        let right = ScreenInfo(displayID: 1, x: 1920, y: 0, w: 1920, h: 1080)
        let left = ScreenInfo(displayID: 1, x: -1920, y: 0, w: 1920, h: 1080)
        let below = ScreenInfo(displayID: 1, x: 0, y: -1080, w: 1920, h: 1080)
        let small = ScreenInfo(displayID: 2, x: 0, y: 0, w: 800, h: 600)

        if let f = ScreenPlacement.chooseFrame(screens: [right, main], mainDisplayID: 2,
                                               windowWidth: 900, windowHeight: 792) {
            add("placement-main-not-first",
                ScreenPlacement.fullyInside(f, main) && abs(f.x - 510) < 0.01 && abs(f.y - 144) < 0.01,
                "frame \(f.x),\(f.y)")
        } else { add("placement-main-not-first", false, "nil") }
        if let f = ScreenPlacement.chooseFrame(screens: [left, main], mainDisplayID: 2,
                                               windowWidth: 900, windowHeight: 792) {
            add("placement-main-not-first-neg-origin",
                ScreenPlacement.fullyInside(f, main) && abs(f.x - 510) < 0.01 && abs(f.y - 144) < 0.01,
                "frame \(f.x),\(f.y)")
        } else { add("placement-main-not-first-neg-origin", false, "nil") }
        if let f = ScreenPlacement.chooseFrame(screens: [right, main], mainDisplayID: 1,
                                               windowWidth: 900, windowHeight: 792) {
            add("placement-secondary-horizontal",
                ScreenPlacement.fullyInside(f, right) && abs(f.x - 2430) < 0.01 && f.x >= 1920,
                "frame \(f.x),\(f.y)")
        } else { add("placement-secondary-horizontal", false, "nil") }
        if let f = ScreenPlacement.chooseFrame(screens: [below, main], mainDisplayID: 1,
                                               windowWidth: 900, windowHeight: 792) {
            add("placement-vertical-neg-y",
                ScreenPlacement.fullyInside(f, below) && abs(f.y + 1080 - 144) < 0.01,
                "frame \(f.x),\(f.y)")
        } else { add("placement-vertical-neg-y", false, "nil") }
        add("placement-missing-main-fails",
            ScreenPlacement.chooseFrame(screens: [right], mainDisplayID: 2,
                                        windowWidth: 900, windowHeight: 792) == nil)
        add("placement-too-small-fails",
            ScreenPlacement.chooseFrame(screens: [small], mainDisplayID: 2,
                                        windowWidth: 900, windowHeight: 792) == nil)

        return r
    }
}

// MARK: - Coordinator-only case export (never for the GUI agent publish dir)

enum CaseExporter {
    static func manifestJSONObject() -> [String: Any] {
        var suites: [String: Any] = [:]
        for suite in Suite.ordered {
            let cases = CaseCatalog.cases(for: suite)
            suites[suite.rawValue] = [
                "total": cases.count,
                "cases": cases.map { c in
                    ["id": c.id, "task_payload": c.payload, "expected_text": c.expectedText]
                },
            ]
        }
        return [
            "format": "acceptance-fixture-cases",
            "version": 1,
            "warning": "COORDINATOR-ONLY. Contains answers. Must NOT be placed in the GUI agent publish directory, cwd, MCP resources or prompts.",
            "suites": suites,
        ]
    }

    static func manifestJSON() -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: manifestJSONObject(),
                                                     options: [.prettyPrinted, .sortedKeys]) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
