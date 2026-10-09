#!/usr/bin/env python3
"""Cross-platform case-catalog parity checker (coordinator tool).

Verifies that macos/Cases.swift and windows/Cases.cs embed the SAME case
catalog at the UTF-16 level, and that the catalog satisfies the layered
fixture spec. Optionally compares against a runtime --export-cases manifest
produced by a built binary.

Usage:
  python3 tools/check-cases-parity.py [runtime-manifest.json]

Exit code 0 = parity + spec OK; 1 = failure (details on stdout).
"""
import json
import re
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SWIFT = ROOT / "macos" / "Cases.swift"
CS = ROOT / "windows" / "Cases.cs"

SUITE_ORDER = ["baseline", "punctuation", "emoji", "known-input", "legacy"]
EXPECTED_TOTALS = {"baseline": 10, "punctuation": 6, "emoji": 6, "known-input": 10, "legacy": 1}


def decode_swift(s: str) -> str:
    out = []
    i = 0
    while i < len(s):
        c = s[i]
        if c == "\\":
            n = s[i + 1]
            if n == "r":
                out.append("\r"); i += 2
            elif n == "n":
                out.append("\n"); i += 2
            elif n == "t":
                out.append("\t"); i += 2
            elif n == '"':
                out.append('"'); i += 2
            elif n == "\\":
                out.append("\\"); i += 2
            elif n == "u" and s[i + 2] == "{":
                j = s.index("}", i)
                out.append(chr(int(s[i + 3:j], 16))); i = j + 1
            else:
                raise ValueError(f"unknown swift escape at {s[i:i+8]!r}")
        else:
            out.append(c); i += 1
    return "".join(out)


def decode_cs(s: str) -> str:
    simple = {"r": "\r", "n": "\n", "t": "\t", '"': '"', "\\": "\\", "0": "\0"}
    out = []
    i = 0
    while i < len(s):
        c = s[i]
        if c == "\\":
            n = s[i + 1]
            if n in simple:
                out.append(simple[n]); i += 2
            elif n == "u":
                out.append(chr(int(s[i + 2:i + 6], 16))); i += 6
            else:
                raise ValueError(f"unknown cs escape at {s[i:i+8]!r}")
        else:
            out.append(c); i += 1
    return "".join(out)


def extract_swift_tables(src: str):
    tables = {}
    lm = re.search(r'static let legacySample = "((?:[^"\\]|\\.)*)"', src)
    assert lm, "legacySample not found in swift"
    legacy_payload = decode_swift(lm.group(1))
    for m in re.finditer(
        r"static let (\w+Cases): \[FixtureCase\] = \[(.*?)\n    \]", src, re.S
    ):
        name, body = m.group(1), m.group(2)
        cases = []
        for cm in re.finditer(r'FixtureCase\(id: "((?:[^"\\]|\\.)*)", payload: "((?:[^"\\]|\\.)*)"\)', body):
            cases.append((decode_swift(cm.group(1)), decode_swift(cm.group(2))))
        # known-04 references the legacySample constant instead of a literal
        merged = []
        for cm in re.finditer(
            r'FixtureCase\(id: "((?:[^"\\]|\\.)*)", payload: (?:"((?:[^"\\]|\\.)*)"|legacySample)\)', body):
            cid = decode_swift(cm.group(1))
            payload = legacy_payload if cm.group(2) is None else decode_swift(cm.group(2))
            merged.append((cid, payload))
        tables[name] = merged
    tables["legacy"] = [("legacy-sample", legacy_payload)]
    return tables


def extract_cs_tables(src: str):
    tables = {}
    lm = re.search(r'public const string LegacySample = "((?:[^"\\]|\\.)*)";', src)
    assert lm, "LegacySample not found in cs"
    legacy_payload = decode_cs(lm.group(1))
    for m in re.finditer(
        r"public static readonly FixtureCase\[\] (\w+Cases) = new FixtureCase\[\]\n\s*\{(.*?)\n        \};",
        src, re.S,
    ):
        name, body = m.group(1), m.group(2)
        merged = []
        for cm in re.finditer(
            r'new FixtureCase\("((?:[^"\\]|\\.)*)", (?:"((?:[^"\\]|\\.)*)"|LegacySample)\)', body):
            cid = decode_cs(cm.group(1))
            payload = legacy_payload if cm.group(2) is None else decode_cs(cm.group(2))
            merged.append((cid, payload))
        tables[name] = merged
    tables["legacy"] = [("legacy-sample", legacy_payload)]
    return tables


SWIFT_TABLE_TO_SUITE = {
    "baselineCases": "baseline",
    "punctuationCases": "punctuation",
    "emojiCases": "emoji",
    "knownInputCases": "known-input",
    "legacy": "legacy",
}


def normalize(tables):
    suite_of = {
        "baselineCases": "baseline",
        "punctuationCases": "punctuation",
        "emojiCases": "emoji",
        "knownInputCases": "known-input",
        "BaselineCases": "baseline",
        "PunctuationCases": "punctuation",
        "EmojiCases": "emoji",
        "KnownInputCases": "known-input",
        "legacy": "legacy",
    }
    out = {}
    for name, cases in tables.items():
        out[suite_of[name]] = [(cid, payload) for cid, payload in cases]
    return out


def utf16(s):
    return s.encode("utf-16-le")


def check_spec(catalog, errors):
    for suite, total in EXPECTED_TOTALS.items():
        cases = catalog.get(suite, [])
        if len(cases) != total:
            errors.append(f"{suite}: expected {total} cases, got {len(cases)}")
    ids = [cid for cases in catalog.values() for cid, _ in cases]
    if len(set(ids)) != len(ids):
        dupes = [i for i in ids if ids.count(i) > 1]
        errors.append(f"duplicate case ids: {set(dupes)}")
    for suite, cases in catalog.items():
        for cid, payload in cases:
            if not payload:
                errors.append(f"{cid}: empty payload")
            expected = payload.replace("\r\n", "\n")
            if cid == "known-09":
                if "\r\n" not in payload:
                    errors.append(f"{cid}: expected CRLF in payload")
            elif payload != expected:
                errors.append(f"{cid}: payload must equal expected (no CRLF outside known-09)")
    if "baseline" in catalog:
        lens = sorted({len(p) for _, p in catalog["baseline"]})
        if max(lens) > 40:
            errors.append(f"baseline max length {max(lens)} > 40")
        if 12 not in lens or 34 not in lens:
            errors.append(f"baseline must include 12 and 34 lengths; got {lens}")
        for cid, p in catalog["baseline"]:
            if any(ch != " " and unicodedata.category(ch).startswith("Z") for ch in p):
                errors.append(f"{cid}: non-plain-space whitespace")
            if any(0x1F000 <= ord(ch) <= 0x10FFFF or 0x2190 <= ord(ch) <= 0x2BFF for ch in p):
                errors.append(f"{cid}: emoji/symbol in baseline")
    if "known-input" in catalog:
        d = dict(catalog["known-input"])
        if len(d.get("known-02", "")) != 12:
            errors.append("known-02 must be 12 UTF-16 units")
        if len(d.get("known-03", "")) != 34:
            errors.append("known-03 must be 34 UTF-16 units")
        if " " not in d.get("known-06", ""):
            errors.append("known-06 must contain NBSP U+00A0")
        if "́" not in d.get("known-08", "") or "é" in d.get("known-08", ""):
            errors.append("known-08 must contain e+U+0301 and no precomposed U+00E9")
        if not any(ord(ch) > 0xFFFF for ch in d.get("known-07", "")):
            errors.append("known-07 must contain a non-BMP scalar")
        if "\t" not in d.get("known-10", ""):
            errors.append("known-10 must contain a tab")



GESTURE_SUITES = ["multiclick", "drag", "scroll"]

def gesture_catalogs(errors):
    sw = (ROOT / "macos" / "GestureCases.swift").read_text(encoding="utf-8")
    cs = (ROOT / "windows" / "GestureCases.cs").read_text(encoding="utf-8")
    sr = re.findall(r'GestureCase\(id: "([^"\\]*(?:\\.[^"\\]*)*)", spec: "([^"\\]*(?:\\.[^"\\]*)*)",\s*instruction: "([^"\\]*(?:\\.[^"\\]*)*)"\)', sw)
    cr = re.findall(r'new GestureCase\("([^"\\]*(?:\\.[^"\\]*)*)", "([^"\\]*(?:\\.[^"\\]*)*)", "([^"\\]*(?:\\.[^"\\]*)*)"\)', cs)
    source = {suite: [] for suite in GESTURE_SUITES}
    for suite in GESTURE_SUITES:
        a = [tuple(decode_swift(v) for v in row) for row in sr if row[0].startswith(suite + "-")]
        b = [tuple(decode_cs(v) for v in row) for row in cr if row[0].startswith(suite + "-")]
        source[suite] = a
        expected_ids = [f"{suite}-{i:02}" for i in range(1, 11)]
        if [row[0] for row in a] != expected_ids or [row[0] for row in b] != expected_ids:
            errors.append(f"{suite}: must contain exactly 10 ordered unique case IDs")
        if [[utf16(v) for v in row] for row in a] != [[utf16(v) for v in row] for row in b]:
            errors.append(f"{suite}: id/spec/instruction source parity mismatch")
    return source

def check_gesture_manifest(manifest, source, errors):
    suites = manifest.get("suites", {})
    if set(suites) != set(SUITE_ORDER + GESTURE_SUITES):
        errors.append("runtime suite set must retain old 33 + new 30 cases")
    for suite, expected in source.items():
        group = suites.get(suite, {})
        cases = group.get("cases", [])
        runtime = [(c.get("id", ""), c.get("spec", ""), c.get("instruction", "")) for c in cases]
        if group.get("total") != 10 or [[utf16(v) for v in row] for row in runtime] != [[utf16(v) for v in row] for row in expected]:
            errors.append(f"{suite}: runtime gesture catalog != source")
        for c in cases:
            notes = c.get("platform_expected", {})
            if set(notes) != {"macos", "windows"}:
                errors.append(f"{c.get('id')}: missing explicit per-platform expected metadata")


def main():
    errors = []
    swift_tables = extract_swift_tables(SWIFT.read_text(encoding="utf-8"))
    cs_tables = extract_cs_tables(CS.read_text(encoding="utf-8"))
    swift_cat = normalize(swift_tables)
    cs_cat = normalize(cs_tables)

    if set(swift_cat) != set(SUITE_ORDER):
        errors.append(f"swift suites mismatch: {sorted(swift_cat)}")
    if set(cs_cat) != set(SUITE_ORDER):
        errors.append(f"cs suites mismatch: {sorted(cs_cat)}")

    for suite in SUITE_ORDER:
        sw = swift_cat.get(suite, [])
        cs = cs_cat.get(suite, [])
        if len(sw) != len(cs):
            errors.append(f"{suite}: swift has {len(sw)} cases, cs has {len(cs)}")
            continue
        for (sid, sp), (cid, cp) in zip(sw, cs):
            if sid != cid:
                errors.append(f"{suite}: id mismatch {sid!r} vs {cid!r}")
            if utf16(sp) != utf16(cp):
                errors.append(f"{sid}: payload differs between swift and cs at UTF-16 level")

    check_spec(swift_cat, errors)
    gestures = gesture_catalogs(errors)

    if len(sys.argv) > 1:
        manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8-sig"))
        check_gesture_manifest(manifest, gestures, errors)
        for suite in SUITE_ORDER:
            runtime = [(c["id"], c["task_payload"], c["expected_text"])
                       for c in manifest["suites"][suite]["cases"]]
            if len(runtime) != EXPECTED_TOTALS[suite]:
                errors.append(f"runtime manifest {suite}: {len(runtime)} cases")
            for cid, payload, expected in runtime:
                src = dict(swift_cat[suite]).get(cid)
                if src is None:
                    errors.append(f"runtime case {cid} not in swift catalog")
                    continue
                if utf16(payload) != utf16(src):
                    errors.append(f"{cid}: runtime payload != source")
                if utf16(expected) != utf16(src.replace("\r\n", "\n")):
                    errors.append(f"{cid}: runtime expected_text != normalized source")

    if len(sys.argv) > 2:
        other = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8-sig"))
        check_gesture_manifest(other, gestures, errors)
        if other.get("suites") != manifest.get("suites"):
            errors.append("Mac/Windows runtime exports differ (including retained text suites)")

    if errors:
        print("PARITY/SPEC FAILURES:")
        for e in errors:
            print("  FAIL:", e)
        return 1
    total = sum(EXPECTED_TOTALS.values()) + 30
    print(f"PARITY OK: swift == cs at UTF-16 level; spec satisfied; {total} cases across {len(SUITE_ORDER) + len(GESTURE_SUITES)} suites")
    if len(sys.argv) > 1:
        print(f"runtime manifest match: OK ({sys.argv[1]})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
