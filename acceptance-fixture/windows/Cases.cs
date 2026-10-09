//
//  Layered case catalog + contract logic (PURE — no WinForms, no UI, no I/O).
//  Shared contract with macos/Cases.swift; keep both equivalent at the
//  UTF-16 level. Verify with tools/check-cases-parity.py.
//
//  Suites:
//    baseline     10 clear short texts — pure-vision copy target
//    punctuation   6 full/half-width punctuation mixes — pure vision
//    emoji         6 very short texts, one common emoji each — pure vision
//    known-input  10 explicit payloads handed to the agent as text
//    legacy        original single sample (default when --suite omitted)
//
//  expected_text is the GUI-check target: payload with CRLF normalized to LF
//  (runtime contract). matched is ALWAYS exact UTF-16 code-unit comparison
//  (C# string ordinal equality is code-unit-wise; kept explicit here).
//
//  NOTE: must stay C# 5 compatible (in-box csc.exe v4.0.30319).
//

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;

namespace AcceptanceFixture
{
    enum SuiteKind
    {
        Legacy,
        Baseline,
        Punctuation,
        Emoji,
        KnownInput
    }

    static class SuiteNames
    {
        public const string Legacy = "legacy";
        public const string Baseline = "baseline";
        public const string Punctuation = "punctuation";
        public const string Emoji = "emoji";
        public const string KnownInput = "known-input";

        public static bool TryParse(string s, out SuiteKind kind)
        {
            switch (s)
            {
                case Legacy: kind = SuiteKind.Legacy; return true;
                case Baseline: kind = SuiteKind.Baseline; return true;
                case Punctuation: kind = SuiteKind.Punctuation; return true;
                case Emoji: kind = SuiteKind.Emoji; return true;
                case KnownInput: kind = SuiteKind.KnownInput; return true;
                default: kind = SuiteKind.Legacy; return false;
            }
        }

        public static string Name(SuiteKind kind)
        {
            switch (kind)
            {
                case SuiteKind.Legacy: return Legacy;
                case SuiteKind.Baseline: return Baseline;
                case SuiteKind.Punctuation: return Punctuation;
                case SuiteKind.Emoji: return Emoji;
                case SuiteKind.KnownInput: return KnownInput;
                default: return Legacy;
            }
        }

        // Fixed order for export manifests / self-test reporting.
        public static readonly SuiteKind[] Ordered = new SuiteKind[]
        {
            SuiteKind.Baseline, SuiteKind.Punctuation, SuiteKind.Emoji,
            SuiteKind.KnownInput, SuiteKind.Legacy
        };
    }

    sealed class FixtureCase
    {
        public readonly string Id;
        public readonly string Payload;
        // GUI-check target: payload normalized CRLF -> LF (runtime contract).
        // Derived here so both platforms can never diverge on it.
        public readonly string ExpectedText;

        public FixtureCase(string id, string payload)
        {
            Id = id;
            Payload = payload;
            ExpectedText = CaseContract.NormalizeCRLF(payload);
        }
    }

    static class CaseContract
    {
        public static string NormalizeCRLF(string s)
        {
            return s.Replace("\r\n", "\n");
        }

        // Exact UTF-16 code-unit comparison — no Trim, no Unicode normalization.
        public static bool Utf16ExactMatch(string actual, string expected)
        {
            if (actual == null || expected == null) return false;
            if (actual.Length != expected.Length) return false;
            for (int i = 0; i < actual.Length; i++)
            {
                if (actual[i] != expected[i]) return false;
            }
            return true;
        }

        public static string Utf16Hex(string s)
        {
            var sb = new StringBuilder();
            for (int i = 0; i < s.Length; i++)
            {
                if (i > 0) sb.Append(' ');
                sb.Append(((int)s[i]).ToString("X4"));
            }
            return sb.ToString();
        }
    }

    static class CaseCatalog
    {
        // Historical legacy sample — MUST NOT change (regression-checked below).
        public const string LegacySample = "你好，世界 🌍 | Unicode 测试 ✅ | 本文档共 3 段（含本段）";

        // baseline: 10 short, clear, distinct texts; common Chinese/ASCII/digits;
        // no emoji, no special spaces, no full/half-width punctuation ambiguity;
        // single U+0020 as the only separator; lengths include 12 and 34 UTF-16
        // units; max <= 40. For screenshot-only copying.
        public static readonly FixtureCase[] BaselineCases = new FixtureCase[]
        {
            new FixtureCase("baseline-01", "computer use acceptance test"),
            new FixtureCase("baseline-02", "原生输入验收 2026"),
            new FixtureCase("baseline-03", "键盘鼠标截图点击输入检查"),
            new FixtureCase("baseline-04", "RPA flow runs every morning"),
            new FixtureCase("baseline-05", "这是长度为三十四个字符的普通中文验收样例文字全部常见字形逐字核对无误"),
            new FixtureCase("baseline-06", "verify keystroke injection accuracy"),
            new FixtureCase("baseline-07", "第二组普通文字抄写检查通过"),
            new FixtureCase("baseline-08", "视觉验收普通文字样例抄写"),
            new FixtureCase("baseline-09", "screen 1080p resolution check"),
            new FixtureCase("baseline-10", "输入延迟与准确率统计报告"),
        };

        // punctuation: 6 short texts, each isolating one full/half-width pair;
        // never mixed with emoji. Special spaces are EXCLUDED here (visually
        // indistinguishable -> belongs to known-input, not the vision matrix).
        public static readonly FixtureCase[] PunctuationCases = new FixtureCase[]
        {
            new FixtureCase("punctuation-01", "括号测试（全角内容）结束"),
            new FixtureCase("punctuation-02", "code (alpha) beta 42 done"),
            new FixtureCase("punctuation-03", "第一项，第二项，第三项完毕"),
            new FixtureCase("punctuation-04", "one, two, three, four, five"),
            new FixtureCase("punctuation-05", "管道 | 分隔 | 符号 | 测试"),
            new FixtureCase("punctuation-06", "引号 “全角” 与 \"半角\" 对比"),
        };

        // emoji: 6 very short texts, exactly one common distinct emoji each.
        // WinForms may render these monochrome (Segoe UI Emoji) — a screenshot
        // can therefore NOT always uniquely determine the code point.
        public static readonly FixtureCase[] EmojiCases = new FixtureCase[]
        {
            new FixtureCase("emoji-01", "状态 ✅"),
            new FixtureCase("emoji-02", "地球 🌍"),
            new FixtureCase("emoji-03", "火箭 🚀"),
            new FixtureCase("emoji-04", "微笑 😀"),
            new FixtureCase("emoji-05", "铃铛 🔔"),
            new FixtureCase("emoji-06", "书本 📚"),
        };

        // known-input: 10 explicit payloads handed to the agent as ORIGINAL TEXT
        // (never visual inference). task_payload vs expected_text are separate:
        // expected is the CRLF->LF normalized check target.
        public static readonly FixtureCase[] KnownInputCases = new FixtureCase[]
        {
            new FixtureCase("known-01", "Order 6642 shipped via DHL Express"),
            new FixtureCase("known-02", "键盘输入验收测试样例文本"),                       // 12 UTF-16 units
            new FixtureCase("known-03", "分布式原生输入验收涵盖中文长文本逐字精确比对三十四字样例内容全部完毕"), // 34 UTF-16 units
            new FixtureCase("known-04", LegacySample),                                    // historical 38-scalar sample
            new FixtureCase("known-05", "注意：括号（全角）与问号？感叹号！"),
            new FixtureCase("known-06", "NBSP\u00A0分隔\u00A0样例"),
            new FixtureCase("known-07", "🦄 独角兽 non-BMP 样例"),
            new FixtureCase("known-08", "resume\u0301 和 cafe\u0301 组合音符样例"),        // e + combining acute, deliberately NOT precomposed
            new FixtureCase("known-09", "第一行\r\n第二行\r\n第三行"),                     // CRLF; expected uses LF only
            new FixtureCase("known-10", "姓名\t部门\t工号"),
        };

        public static FixtureCase[] CasesFor(SuiteKind suite)
        {
            switch (suite)
            {
                case SuiteKind.Legacy:
                    return new FixtureCase[] { new FixtureCase("legacy-sample", LegacySample) };
                case SuiteKind.Baseline: return BaselineCases;
                case SuiteKind.Punctuation: return PunctuationCases;
                case SuiteKind.Emoji: return EmojiCases;
                case SuiteKind.KnownInput: return KnownInputCases;
                default: return new FixtureCase[0];
            }
        }

        public static int TotalFor(SuiteKind suite) { return CasesFor(suite).Length; }
    }

    // MARK: - Self-test (pure; no window, no screenshot, no input)

    sealed class SelfTestResult
    {
        public readonly string Name;
        public readonly bool Passed;
        public readonly string Detail;
        public SelfTestResult(string name, bool passed, string detail)
        {
            Name = name; Passed = passed; Detail = detail ?? "";
        }
    }

    static class SelfTest
    {
        private static bool ContainsScalar(string s, int cp)
        {
            // Handles both BMP code points and surrogate-pair-encoded ones.
            return s.IndexOf(char.ConvertFromUtf32(cp), StringComparison.Ordinal) >= 0;
        }

        private static bool HasEmojiScalar(string s)
        {
            // Emoji-scalar detector: any non-BMP scalar (high surrogate), the
            // variation selector U+FE0F, or BMP symbol blocks U+2600-U+27BF
            // (includes U+2705). Full-width / CJK punctuation (U+3000-U+303F,
            // U+FF00-U+FFEF) and General Punctuation (U+2018-U+201D) are NOT
            // emoji and must stay allowed.
            for (int i = 0; i < s.Length; i++)
            {
                char ch = s[i];
                if ((ch >= 0xD800 && ch <= 0xDBFF) || ch == 0xFE0F
                    || (ch >= 0x2600 && ch <= 0x27BF)) return true;
            }
            return false;
        }

        public static List<SelfTestResult> Run()
        {
            var r = new List<SelfTestResult>();
            Action<string, bool, string> add = (name, ok, detail) => r.Add(new SelfTestResult(name, ok, detail));
            Action<string, bool> add0 = (name, ok) => r.Add(new SelfTestResult(name, ok, ""));

            // -- case counts --
            add("count-baseline", CaseCatalog.TotalFor(SuiteKind.Baseline) == 10,
                "expected 10, got " + CaseCatalog.TotalFor(SuiteKind.Baseline));
            add("count-punctuation", CaseCatalog.TotalFor(SuiteKind.Punctuation) == 6,
                "expected 6, got " + CaseCatalog.TotalFor(SuiteKind.Punctuation));
            add("count-emoji", CaseCatalog.TotalFor(SuiteKind.Emoji) == 6,
                "expected 6, got " + CaseCatalog.TotalFor(SuiteKind.Emoji));
            add("count-known-input", CaseCatalog.TotalFor(SuiteKind.KnownInput) == 10,
                "expected 10, got " + CaseCatalog.TotalFor(SuiteKind.KnownInput));
            add0("count-legacy", CaseCatalog.TotalFor(SuiteKind.Legacy) == 1);

            // -- unique IDs / nonempty across ALL suites --
            var all = new List<FixtureCase>();
            foreach (var s in SuiteNames.Ordered) all.AddRange(CaseCatalog.CasesFor(s));
            var ids = new HashSet<string>();
            foreach (var c in all) ids.Add(c.Id);
            add0("ids-unique", ids.Count == all.Count);
            add0("payloads-nonempty", all.All(c => !string.IsNullOrEmpty(c.Payload)));
            add0("expected-nonempty", all.All(c => !string.IsNullOrEmpty(c.ExpectedText)));

            // -- normalization contract --
            add0("crlf-normalization-holds",
                all.All(c => CaseContract.NormalizeCRLF(c.Payload) == c.ExpectedText));
            FixtureCase crlfCase = null;
            foreach (var c in CaseCatalog.KnownInputCases) if (c.Id == "known-09") crlfCase = c;
            add0("crlf-case-payload-vs-expected-differ",
                crlfCase != null && crlfCase.Payload.Contains("\r\n")
                    && !crlfCase.ExpectedText.Contains("\r") && crlfCase.Payload != crlfCase.ExpectedText);
            add0("crlf-case-expected-has-LF", crlfCase != null && crlfCase.ExpectedText.Contains("\n"));
            add0("non-crlf-payload-equals-expected",
                all.All(c => c.Payload.Contains("\r\n") || c.Payload == c.ExpectedText));
            // Provable counterexample: the CRLF case is the ONLY allowed
            // payload != expected divergence (expected is CRLF->LF normalized).
            add0("crlf-equality-counterexample",
                crlfCase != null && crlfCase.Payload != crlfCase.ExpectedText
                    && all.Where(c => c != crlfCase).All(c => c.Payload == c.ExpectedText));

            // -- known-input feature coverage --
            FixtureCase k02 = FindKnown("known-02"), k03 = FindKnown("known-03"),
                        k06 = FindKnown("known-06"), k07 = FindKnown("known-07"),
                        k08 = FindKnown("known-08"), k10 = FindKnown("known-10"),
                        k01 = FindKnown("known-01");
            add0("known-zh12-length", k02 != null && k02.Payload.Length == 12);
            add0("known-zh34-length", k03 != null && k03.Payload.Length == 34);
            // Scalar count = UTF-16 units minus well-formed surrogate PAIRS.
            // Only HIGH surrogates count pairs; counting the whole D800-DFFF
            // range double-counts each pair and yields 37 instead of 38.
            int legacyHigh = CaseCatalog.LegacySample.Count(ch => ch >= 0xD800 && ch <= 0xDBFF);
            int legacyLow = CaseCatalog.LegacySample.Count(ch => ch >= 0xDC00 && ch <= 0xDFFF);
            int legacyScalars = CaseCatalog.LegacySample.Length - legacyHigh;
            add("known-legacy-38-scalars", legacyHigh == legacyLow && legacyScalars == 38,
                "got " + legacyScalars + " scalars (" + CaseCatalog.LegacySample.Length + " UTF-16 units, "
                + legacyHigh + " surrogate pairs)");
            // BMP + non-BMP regression: legacy sample contains both a BMP emoji
            // (U+2705) and a non-BMP emoji (U+1F30D via one surrogate pair).
            add0("known-legacy-bmp-nonbmp-mix",
                ContainsScalar(CaseCatalog.LegacySample, 0x2705)
                    && ContainsScalar(CaseCatalog.LegacySample, 0x1F30D)
                    && legacyHigh == 1);
            add0("known-nbsp-present", k06 != null && ContainsScalar(k06.Payload, 0x00A0));
            add0("known-combining-present",
                k08 != null && ContainsScalar(k08.Payload, 0x0301) && !ContainsScalar(k08.Payload, 0x00E9));
            add0("known-nonbmp-present", k07 != null && k07.Payload.ToCharArray().Any(ch => ch >= 0xD800 && ch <= 0xDFFF));
            add0("known-tab-present", k10 != null && k10.Payload.Contains("\t"));
            add0("known-ascii-present", k01 != null && k01.Payload.All(ch => ch < 128));

            // -- legacy retained exactly (literal spelled out here to catch edits) --
            add0("legacy-sample-retained",
                CaseContract.Utf16ExactMatch(
                    CaseCatalog.LegacySample,
                    "你好，世界 🌍 | Unicode 测试 ✅ | 本文档共 3 段（含本段）"));

            // -- baseline constraints --
            var baseline = CaseCatalog.BaselineCases;
            int maxLen = baseline.Max(c => c.Payload.Length);
            add("baseline-maxlen-40", maxLen <= 40, "max " + maxLen);
            bool onlyPlainSpaces = baseline.All(c => c.Payload.All(
                ch => ch == ' ' || !char.IsWhiteSpace(ch)));
            add0("baseline-only-plain-spaces", onlyPlainSpaces);
            bool noEmojiNoSymbols = baseline.All(c => c.Payload.All(
                ch => ch < 0xD800 && !(ch >= 0x2190 && ch <= 0x2BFF)));
            add0("baseline-no-emoji-or-symbols", noEmojiNoSymbols);
            var lengths = new HashSet<int>(baseline.Select(c => c.Payload.Length));
            add("baseline-has-12-and-34", lengths.Contains(12) && lengths.Contains(34),
                "lengths " + string.Join(",", lengths.OrderBy(x => x)));
            add0("baseline-all-distinct",
                new HashSet<string>(baseline.Select(c => c.Payload)).Count == baseline.Length);

            // -- punctuation: no emoji / variation selectors --
            var punctOffenders = new List<string>();
            foreach (var c in CaseCatalog.PunctuationCases)
                if (HasEmojiScalar(c.Payload)) punctOffenders.Add(c.Id);
            add("punctuation-no-emoji", punctOffenders.Count == 0,
                punctOffenders.Count == 0 ? "" : "emoji in " + string.Join(",", punctOffenders));
            // Detector must not be vacuous: flags BMP emoji (U+2705), non-BMP
            // emoji (U+1F30D) and VS16, but ACCEPTS full-width/CJK punctuation
            // (U+FF08, U+FF0C, U+FF1F, U+FF01) and plain text.
            add0("punctuation-no-emoji-detector-not-vacuous",
                HasEmojiScalar("x\u2705y") && HasEmojiScalar("x\uD83C\uDF0Dy")
                    && HasEmojiScalar("x\uFE0Fy")
                    && !HasEmojiScalar("\uFF08全角\uFF09\uFF0C\uFF1F\uFF01")
                    && !HasEmojiScalar("code (alpha) beta"));

            // -- emoji: one known distinct emoji scalar per case --
            int[] emojiScalars = { 0x2705, 0x1F30D, 0x1F680, 0x1F600, 0x1F514, 0x1F4DA };
            var found = new List<int>();
            bool eachHasOne = true;
            foreach (var c in CaseCatalog.EmojiCases)
            {
                int hit = -1;
                foreach (int cp in emojiScalars)
                {
                    if (ContainsScalar(c.Payload, cp)) { hit = cp; break; }
                }
                if (hit < 0) { eachHasOne = false; }
                else found.Add(hit);
            }
            add("emoji-one-known-each", eachHasOne && new HashSet<int>(found).Count == 6,
                "found " + string.Join(",", found.Select(x => x.ToString("x4"))));

            // -- suite name parsing --
            foreach (var s in SuiteNames.Ordered)
            {
                SuiteKind parsed;
                add0("suite-parse-" + SuiteNames.Name(s),
                    SuiteNames.TryParse(SuiteNames.Name(s), out parsed) && parsed == s);
            }
            SuiteKind ignored;
            add0("suite-parse-rejects-unknown",
                !SuiteNames.TryParse("foo", out ignored) && !SuiteNames.TryParse("Legacy", out ignored));

            // -- UTF-16 exact comparison semantics --
            add0("utf16-exact-accept-identical", CaseContract.Utf16ExactMatch("abc", "abc"));
            add0("utf16-exact-accept-emoji", CaseContract.Utf16ExactMatch("状态 ✅", "状态 ✅"));
            add0("utf16-exact-rejects-nfc-nfd",
                !CaseContract.Utf16ExactMatch("caf\u00E9", "cafe\u0301"));
            add0("utf16-exact-rejects-trailing-space",
                !CaseContract.Utf16ExactMatch("abc", "abc "));
            add0("utf16-exact-rejects-crlf-vs-lf",
                !CaseContract.Utf16ExactMatch("a\r\nb", "a\nb"));
            add0("utf16-exact-rejects-nbsp-vs-space",
                !CaseContract.Utf16ExactMatch("a\u00A0b", "a b"));

            return r;
        }

        private static FixtureCase FindKnown(string id)
        {
            foreach (var c in CaseCatalog.KnownInputCases) if (c.Id == id) return c;
            return null;
        }
    }

    // MARK: - Coordinator-only case export (never for the GUI agent publish dir)

    static class CaseExporter
    {
        private static string Escape(string s)
        {
            var sb = new StringBuilder(s.Length + 8);
            foreach (char c in s)
            {
                if (c == '"') sb.Append("\\\"");
                else if (c == '\\') sb.Append("\\\\");
                else if (c == '\b') sb.Append("\\b");
                else if (c == '\f') sb.Append("\\f");
                else if (c == '\n') sb.Append("\\n");
                else if (c == '\r') sb.Append("\\r");
                else if (c == '\t') sb.Append("\\t");
                else if (c < 0x20) sb.Append("\\u" + ((int)c).ToString("x4"));
                else sb.Append(c);
            }
            return sb.ToString();
        }

        private static string Q(string k, string v)
        {
            return "\"" + Escape(k) + "\":\"" + Escape(v) + "\"";
        }

        public static string ManifestJson()
        {
            var sb = new StringBuilder();
            sb.Append("{\n");
            sb.Append("  \"format\": \"acceptance-fixture-cases\",\n");
            sb.Append("  \"version\": 1,\n");
            sb.Append("  \"warning\": \"COORDINATOR-ONLY. Contains answers. Must NOT be placed in the GUI agent publish directory, cwd, MCP resources or prompts.\",\n");
            sb.Append("  \"suites\": {\n");
            for (int si = 0; si < SuiteNames.Ordered.Length; si++)
            {
                var suite = SuiteNames.Ordered[si];
                var cases = CaseCatalog.CasesFor(suite);
                sb.Append("    \"" + SuiteNames.Name(suite) + "\": {\n");
                sb.Append("      \"total\": " + cases.Length + ",\n");
                sb.Append("      \"cases\": [\n");
                for (int ci = 0; ci < cases.Length; ci++)
                {
                    var c = cases[ci];
                    sb.Append("        { " + Q("id", c.Id) + ", " + Q("task_payload", c.Payload)
                              + ", " + Q("expected_text", c.ExpectedText) + " }");
                    sb.Append(ci + 1 < cases.Length ? ",\n" : "\n");
                }
                sb.Append("      ]\n");
                sb.Append("    }");
                sb.Append(si + 1 < SuiteNames.Ordered.Length ? ",\n" : "\n");
            }
            sb.Append("  }\n");
            sb.Append("}\n");
            return sb.ToString();
        }
    }
}
