#!/usr/bin/env python3
"""TDD tests for scripts/analyze-layered-gui.py.

All fixtures are synthetic (no GUI, no deployment, no real oracle data).
Run: python3 tests/layered_gui_analyzer.py
"""
from __future__ import annotations

import importlib.util
import json
import os
import tempfile
import unittest

_HERE = os.path.dirname(os.path.abspath(__file__))
_SCRIPT = os.path.join(_HERE, "..", "scripts", "analyze-layered-gui.py")

_spec = importlib.util.spec_from_file_location("analyze_layered_gui", _SCRIPT)
mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mod)


# ------------------------------------------------------------ builders

def ts(sec=0, ms=0):
    """Windows 'o' style ISO timestamp with fractional seconds."""
    return "2026-09-30T10:00:%02d.%03dZ" % (sec % 60, ms)


def oracle_line(obj):
    return json.dumps(obj, ensure_ascii=False)


def trial(suite, case_id, t, expected_text=None, case_index=0, case_total=1, trial_i=0, nonce="N"):
    ev = {"type": "trial", "ts": t, "trial": trial_i, "nonce": nonce,
          "suite": suite, "case_id": case_id, "case_index": case_index, "case_total": case_total}
    if expected_text is not None:
        ev["expected_text"] = expected_text
    return ev


def click(suite, case_id, t, hit, trial_i=0, nonce="N"):
    return {"type": "hit" if hit else "wrong", "ts": t, "trial": trial_i, "nonce": nonce,
            "suite": suite, "case_id": case_id, "slot": 1, "target_slot": 1 if hit else 2}


def check(suite, case_id, t, expected, actual, matched=None, trial_i=0, nonce="N",
          actual_hex=None, expected_hex=None):
    if matched is None:
        matched = (expected == actual)
    if actual_hex is None:
        actual_hex = mod.utf16_code_units_hex(actual)
    if expected_hex is None:
        expected_hex = mod.utf16_code_units_hex(expected)
    return {"type": "text_check", "ts": t, "trial": trial_i, "nonce": nonce, "suite": suite,
            "case_id": case_id, "matched": matched, "expected_text": expected,
            "actual_text": actual, "actual_utf16_hex": actual_hex, "expected_utf16_hex": expected_hex}


def text_input_action(text):
    return {"kind": "text_input", "text": text}


def write_transcript(path, entries):
    """entries: ('assistant', model, [(tool_use_id, action_or_None)], ts)
    or ('user', [(tool_use_id, content_or_None)], ts) or raw dict."""
    lines = []
    for e in entries:
        if e[0] == "assistant":
            _, model, tus, t = e
            content = []
            for tid, action in tus:
                inp = {"action": action} if action is not None else {}
                content.append({"type": "tool_use", "id": tid, "name": "computer_step", "input": inp})
            lines.append(json.dumps({"type": "assistant", "timestamp": t,
                                     "message": {"model": model, "content": content}}))
        elif e[0] == "user":
            _, pairs, t = e
            content = []
            for tid, c in pairs:
                blk = {"type": "tool_result", "tool_use_id": tid}
                if c is not None:
                    blk["content"] = c
                content.append(blk)
            lines.append(json.dumps({"type": "user", "timestamp": t, "message": {"content": content}}))
        else:
            lines.append(json.dumps(e))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")


def write_oracle(path, events, bom=False):
    with open(path, "wb") as fh:
        if bom:
            fh.write(b"\xef\xbb\xbf")
        for ev in events:
            fh.write(oracle_line(ev).encode("utf-8") + b"\r\n")


def write_catalog(path, cases):
    with open(path, "w", encoding="utf-8") as fh:
        json.dump({"cases": cases}, fh, ensure_ascii=False)


def run_analyzer(tmp, oracle_events, transcript_entries, catalog_cases=None, bom=False):
    o = os.path.join(tmp, "oracle.jsonl")
    t = os.path.join(tmp, "transcript.jsonl")
    out = os.path.join(tmp, "report.json")
    write_oracle(o, oracle_events, bom=bom)
    write_transcript(t, transcript_entries)
    argv = ["--oracle", o, "--transcript", t, "--output", out]
    if catalog_cases is not None:
        cat = os.path.join(tmp, "catalog.json")
        write_catalog(cat, catalog_cases)
        argv += ["--catalog", cat]
    rc = mod.main(argv)
    assert rc == 0
    with open(out, encoding="utf-8") as fh:
        return json.load(fh)


def tool_results_for(ids, at=None, content="ok"):
    return ("user", [(tid, content) for tid in ids], at or ts(sec=1, ms=500))


def simple_case(events, payload, oracle_expected, oracle_actual, cat_payload,
                case_id="c1", p_at=None, c_at=None, catalog=True):
    """Trial + proven-pairing-shaped fixture for one exact-pass case."""
    events.append(trial("S1", case_id, p_at or ts(sec=0)))
    events.append(check("S1", case_id, c_at or ts(sec=2), oracle_expected, oracle_actual))
    entries = [
        ("assistant", "glm-5.3-flash", [("tu_" + case_id, text_input_action(payload))], p_at or ts(sec=1)),
        tool_results_for(["tu_" + case_id]),
    ]
    catalog = [{"suite": "S1", "case_id": case_id, "task_payload": cat_payload,
                "expected_text": oracle_expected}] if catalog else None
    return events, entries, catalog


# ------------------------------------------------------------ tests

class LayeredAnalyzerTests(unittest.TestCase):
    def test_exact_success_and_bom(self):
        """Exact success end to end; oracle has a UTF-8 BOM; Windows 7-digit
        fractional timestamps parse; pairing proven via trial interval."""
        with tempfile.TemporaryDirectory() as tmp:
            e1 = "héllo"
            events = [{"type": "session", "ts": ts(0), "suite": "S1"}]
            events.append(trial("S1", "c1", ts(sec=0), expected_text=e1, case_index=1, case_total=2))
            events.append(click("S1", "c1", ts(sec=2), hit=True))
            events.append(check("S1", "c1", ts(sec=3), e1, e1))
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e1))], ts(sec=1)),
                tool_results_for(["tu1"], at=ts(sec=1, ms=900)),
            ]
            catalog = [{"suite": "S1", "case_id": "c1", "task_payload": e1, "expected_text": e1}]
            report = run_analyzer(tmp, events, transcript, catalog, bom=True)
            case = report["cases"]["S1::c1"]
            rec = case["attempt_records"][0]
            self.assertEqual(rec["verdict"], "exact")
            self.assertEqual(rec["alignment"], "proven")
            self.assertIs(case["first_attempt_exact"], True)
            self.assertIs(case["final_exact"], True)
            self.assertEqual(case["hit_count"], 1)
            self.assertEqual(case["attempts"], 1)
            self.assertFalse(case["missing"])
            self.assertIs(report["model_check"]["ok"], True)
            self.assertEqual(report["suites"]["S1"]["first_attempt_exact"], 1)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 1)
            self.assertEqual(report["suites"]["S1"]["case_count"], 1)
            self.assertEqual(report["oracle_stats"]["proven_pairs"], 1)
            self.assertIn("does NOT replace a strict tool-policy audit", report["disclaimer"])

    def test_visual_wrong_click(self):
        """Text is exact but the click hit the wrong slot: wrong event is
        recorded, hit_count stays 0, text exactness is not washed away."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "c2", ts(sec=0), expected_text=e),
                click("S1", "c2", ts(sec=2), hit=False),
                check("S1", "c2", ts(sec=3), e, e),
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c2", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c2"]
            self.assertEqual(case["attempt_records"][0]["verdict"], "exact")
            self.assertEqual(case["hit_count"], 0)
            self.assertEqual(case["wrong_count"], 1)
            self.assertEqual(report["suites"]["S1"]["hit_count"], 0)
            self.assertEqual(report["suites"]["S1"]["wrong_count"], 1)

    def test_input_divergence_app_vs_payload(self):
        """payload == expected but the application ended with different text.
        F1: final_exact must stay False."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [trial("S1", "c3", ts(sec=0)), check("S1", "c3", ts(sec=2), e, "abd")]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c3", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c3"]
            rec = case["attempt_records"][0]
            self.assertEqual(rec["verdict"], "input_divergence")
            self.assertTrue(rec["layer1_payload_vs_expected"])
            self.assertFalse(rec["layer2_actual_vs_normalized_payload"])
            self.assertFalse(rec["layer3_expected_vs_actual"])
            self.assertIs(case["final_exact"], False)
            classes = [f["class"] for f in report["suites"]["S1"]["failures"]]
            self.assertIn("input_divergence", classes)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 0)

    def test_known_crlf_raw_sent_app_lf_is_exact(self):
        """Known case: catalog task_payload contains CRLF; the agent sends the
        RAW CRLF string (layer1 exact raw match), the application shows LF
        (layer2 via normalize_newlines), oracle expected/actual are LF ->
        full exact pass."""
        with tempfile.TemporaryDirectory() as tmp:
            events, entries, catalog = simple_case(
                [], "a\r\nb", "a\nb", "a\nb", "a\r\nb", case_id="c4")
            report = run_analyzer(tmp, events, entries, catalog)
            case = report["cases"]["S1::c4"]
            rec = case["attempt_records"][0]
            self.assertEqual(rec["expected_source"], "catalog.task_payload(raw, exact)")
            self.assertIs(rec["raw_payload_vs_task_payload"], True)
            self.assertIs(rec["normalized_payload_vs_task_payload"], True)
            self.assertTrue(rec["layer2_actual_vs_normalized_payload"])
            self.assertEqual(rec["verdict"], "exact")
            self.assertIs(case["final_exact"], True)

    def test_known_crlf_lf_sent_reports_raw_deviation(self):
        """Agent sent LF instead of the raw CRLF task payload: layer1 (raw,
        exact) fails, exact must be False, raw_task_payload_deviation is
        reported even though the app text ends up correct."""
        with tempfile.TemporaryDirectory() as tmp:
            events, entries, catalog = simple_case(
                [], "a\nb", "a\nb", "a\nb", "a\r\nb", case_id="c4b")
            report = run_analyzer(tmp, events, entries, catalog)
            case = report["cases"]["S1::c4b"]
            rec = case["attempt_records"][0]
            self.assertIs(rec["raw_payload_vs_task_payload"], False)
            self.assertIs(rec["normalized_payload_vs_task_payload"], True)
            self.assertFalse(rec["layer1_payload_vs_expected"])
            self.assertEqual(rec["verdict"], "payload_mismatch")
            self.assertIs(case["final_exact"], False)
            self.assertIn("raw_task_payload_deviation",
                          [f["class"] for f in case["failures"]])
            self.assertEqual(report["suites"]["S1"]["final_exact"], 0)

    def test_nbsp_difference_not_normalized(self):
        """NBSP (U+00A0) vs regular space must NOT be normalized away."""
        with tempfile.TemporaryDirectory() as tmp:
            expected = "a b"
            events = [trial("S1", "c5", ts(sec=0)), check("S1", "c5", ts(sec=2), expected, "a b")]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(expected))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c5", "task_payload": expected, "expected_text": expected}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c5"]
            rec = case["attempt_records"][0]
            self.assertIs(case["final_exact"], False)
            self.assertEqual(rec["verdict"], "input_divergence")
            self.assertIn(" ", json.dumps(report, ensure_ascii=False))

    def test_nfc_nfd_difference_not_normalized(self):
        """NFC é vs NFD (e + combining acute) must NOT be normalized."""
        with tempfile.TemporaryDirectory() as tmp:
            expected = "café"          # NFC
            actual = "café"           # NFD
            events = [trial("S1", "c6", ts(sec=0)), check("S1", "c6", ts(sec=2), expected, actual)]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(expected))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c6", "task_payload": expected, "expected_text": expected}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c6"]
            self.assertIs(case["final_exact"], False)
            rec = case["attempt_records"][0]
            self.assertEqual(rec["verdict"], "input_divergence")
            self.assertEqual(rec["utf16_hex_status"], "ok")

    def test_incomplete_case_missing_from_oracle(self):
        """A catalog case with zero oracle events must be reported missing,
        never counted as passed."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [trial("S1", "c1", ts(sec=0), expected_text=e), check("S1", "c1", ts(sec=2), e, e)]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [
                {"suite": "S1", "case_id": "c1", "task_payload": e, "expected_text": e},
                {"suite": "S1", "case_id": "c7", "task_payload": "zzz", "expected_text": "zzz"},
            ]
            report = run_analyzer(tmp, events, transcript, catalog)
            missing_case = report["cases"]["S1::c7"]
            self.assertTrue(missing_case["missing"])
            self.assertIsNone(missing_case["final_exact"])
            self.assertIn("c7", report["suites"]["S1"]["missing"])
            self.assertEqual(report["suites"]["S1"]["case_count"], 2)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 1)

    def test_retry_does_not_improve_first_pass(self):
        """Two provable attempts in one case: first fails, second passes.
        attempts=2, first_attempt_exact=False, final_exact=True."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "c8", ts(sec=0)),
                check("S1", "c8", ts(sec=1, ms=500), e, "abd"),
                check("S1", "c8", ts(sec=2, ms=500), e, e),
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                ("assistant", "glm-5.3-flash", [("tu2", text_input_action(e))], ts(sec=2)),
                tool_results_for(["tu1"], at=ts(sec=1, ms=200)),
                tool_results_for(["tu2"], at=ts(sec=2, ms=200)),
            ]
            catalog = [{"suite": "S1", "case_id": "c8", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c8"]
            self.assertEqual(case["attempts"], 2)
            self.assertIs(case["first_attempt_exact"], False)
            self.assertIs(case["final_exact"], True)
            self.assertEqual(report["suites"]["S1"]["attempts"], 2)
            self.assertEqual(report["suites"]["S1"]["first_attempt_exact"], 0)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 1)

    def test_duplicate_check_does_not_double_count(self):
        """The same check logged twice for one attempt: attempts stay 1,
        first_attempt_exact is not inflated, the duplicate cannot steal a
        later payload (there is none) and is reported unresolved."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "c9", ts(sec=0)),
                check("S1", "c9", ts(sec=2), e, e),
                check("S1", "c9", ts(sec=2), e, e),  # duplicate, same ts
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c9", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c9"]
            self.assertEqual(case["attempts"], 1)
            self.assertIs(case["first_attempt_exact"], True)
            # last check is the unresolved duplicate: final must stay unknown
            self.assertIsNone(case["final_exact"])
            self.assertEqual(len(case["unresolved_checks"]), 1)
            self.assertIn("unresolved_checks", case["alignment_flags"])
            self.assertEqual(report["oracle_stats"]["unresolved_checks"], 1)

    def test_wrong_utf16_hex_fails(self):
        """actual_utf16_hex inconsistent with actual_text: the check cannot
        be trusted, case is never exact, failure recorded."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "xyz"
            events = [trial("S1", "c10", ts(sec=0)),
                      check("S1", "c10", ts(sec=2), e, e, actual_hex="FFFF FFFF")]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c10", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c10"]
            rec = case["attempt_records"][0]
            self.assertIs(case["final_exact"], False)
            self.assertEqual(rec["verdict"], "utf16_hex_mismatch")
            self.assertIn("utf16_hex_mismatch", [f["class"] for f in case["failures"]])

    def test_orphan_check_unknown_no_payload(self):
        """A check with no candidate payload (checks > inputs in the case)
        must be reported unknown, not force-judged as injection failure and
        not silently counted as pass."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "c11", ts(sec=0)),
                check("S1", "c11", ts(sec=2), e, e),
                check("S1", "c11", ts(sec=3), e, "zzz"),  # orphan: no 2nd payload
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c11", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c11"]
            self.assertEqual(case["attempts"], 1)
            self.assertEqual(len(case["unresolved_checks"]), 1)
            # trailing unresolved check: final_exact must NOT inherit the
            # proven success (last stays unknown)
            self.assertIsNone(case["final_exact"])
            self.assertIs(case["first_attempt_exact"], True)
            self.assertEqual(report["oracle_stats"]["unresolved_checks"], 1)

    def test_ambiguous_same_timestamp_outputs_unknown(self):
        """2 same-second payloads + 2 same-second checks: multiple candidates
        cannot be proven -> unknown (attempts empty, exact None), not invented."""
        with tempfile.TemporaryDirectory() as tmp:
            events = [
                trial("S1", "c12", ts(sec=0)),
                check("S1", "c12", ts(sec=5), "aaa", "aaa"),
                check("S1", "c12", ts(sec=5), "bbb", "bbb"),
            ]
            transcript = [
                ("assistant", "glm-5.3-flash",
                 [("tu1", text_input_action("bbb")), ("tu2", text_input_action("aaa"))], ts(sec=4)),
                tool_results_for(["tu1", "tu2"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c12", "task_payload": "aaa", "expected_text": "aaa"}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c12"]
            self.assertEqual(len(case["unresolved_checks"]), 2)
            self.assertTrue(all(
                u["reason"] == "ambiguous_candidates" for u in case["unresolved_checks"]))
            self.assertIsNone(case["first_attempt_exact"])
            self.assertIsNone(case["final_exact"])
            self.assertEqual(report["suites"]["S1"]["first_attempt_exact"], 0)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 0)

    def test_wrong_model_flagged(self):
        """assistant.model must be glm-5.3-flash; anything else is a failure."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [trial("S1", "c1", ts(sec=0)), check("S1", "c1", ts(sec=2), e, e)]
            transcript = [
                ("assistant", "claude-sonnet-5", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c1", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            self.assertFalse(report["model_check"]["ok"])
            self.assertEqual(report["model_check"]["observed"], ["claude-sonnet-5"])

    def test_missing_model_field_flagged(self):
        """F4: an assistant event without a model field must fail the model
        check even when another event carries the right model."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [trial("S1", "c1", ts(sec=0)), check("S1", "c1", ts(sec=2), e, e)]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                ("assistant", None, [("tu9", {"kind": "mouse_click"})], ts(sec=1, ms=100)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c1", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            self.assertFalse(report["model_check"]["ok"])
            self.assertEqual(report["model_check"]["missing_model_assistant_events"], 1)

    def test_missing_tool_result_marked(self):
        """A text_input tool_use with no tool_result is marked missing_result
        and can never be exact."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [trial("S1", "c1", ts(sec=0)), check("S1", "c1", ts(sec=2), e, e)]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                # no tool_result for tu1
            ]
            catalog = [{"suite": "S1", "case_id": "c1", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c1"]
            rec = case["attempt_records"][0]
            self.assertEqual(rec["verdict"], "missing_result")
            self.assertIs(case["final_exact"], False)
            self.assertEqual(report["transcript_stats"]["text_input_missing_tool_result_ids"], ["tu1"])

    def test_nested_duplicate_tool_use_not_counted(self):
        """A tool_result whose content embeds a duplicated tool_use JSON must
        not inflate the top-level tool_use count; images never logged."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            nested = json.dumps({
                "type": "tool_result",
                "content": [
                    {"type": "tool_use", "id": "FAKE-tu2", "name": "computer_step",
                     "input": {"action": {"kind": "text_input", "text": "evil"}}},
                    {"type": "image", "source": {"type": "base64", "data": "QUJD"}},
                ],
            })
            events = [trial("S1", "c1", ts(sec=0)), check("S1", "c1", ts(sec=2), e, e)]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                ("user", [("tu1", [nested])], ts(sec=1, ms=500)),
            ]
            catalog = [{"suite": "S1", "case_id": "c1", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            self.assertEqual(report["transcript_stats"]["top_level_tool_use_count"], 1)
            self.assertEqual(report["transcript_stats"]["text_input_count"], 1)
            self.assertNotIn("evil", json.dumps(report))
            self.assertNotIn("QUJD", json.dumps(report))
            self.assertIs(report["cases"]["S1::c1"]["final_exact"], True)

    def test_no_catalog_unknown_case_uses_oracle_expected(self):
        """Without catalog, cases are unknown: expectation falls back to the
        oracle expected_text; payload deviation is labelled unknown_case."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [trial("S2", "u1", ts(sec=0)), check("S2", "u1", ts(sec=2), e, e)]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action("abd"))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            report = run_analyzer(tmp, events, transcript)
            case = report["cases"]["S2::u1"]
            rec = case["attempt_records"][0]
            self.assertFalse(case["in_catalog"])
            self.assertEqual(rec["expected_source"], "oracle.expected_text")
            self.assertFalse(rec["layer1_payload_vs_expected"])
            self.assertIsNone(rec["raw_payload_vs_task_payload"])
            self.assertEqual(rec["verdict"], "payload_mismatch")
            self.assertIn("payload_mismatch(unknown_case)",
                          [f["class"] for f in case["failures"]])
            self.assertEqual(report["suites"]["S2"]["unknown_cases"], ["u1"])

    def test_no_check_and_payload_without_check(self):
        """A payload with no following check (run truncated before the check)
        must not count as a pass and must not be mis-paired to another case."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [trial("S1", "c1", ts(sec=0), expected_text=e)]  # trial only, no check
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"]),
            ]
            catalog = [{"suite": "S1", "case_id": "c1", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::c1"]
            self.assertEqual(case["attempts"], 0)
            self.assertIsNone(case["final_exact"])
            self.assertIn("no_text_check", case["alignment_flags"])
            self.assertIn("payload_without_check", case["alignment_flags"])
            self.assertEqual(report["oracle_stats"]["payloads_without_check"], 1)

    # ------------------------------------------------ regressions (F2 etc.)

    def test_missed_case1_check_case2_still_success(self):
        """case1's check is missing entirely while case2 succeeds: case1 must
        keep its payload unpaired (not mis-paired into case2), case2 exact."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "c1", ts(sec=0), expected_text=e),
                trial("S1", "c2", ts(sec=5), expected_text=e),
                check("S1", "c2", ts(sec=7), e, e),
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                ("assistant", "glm-5.3-flash", [("tu2", text_input_action(e))], ts(sec=6)),
                tool_results_for(["tu1"], at=ts(sec=1, ms=200)),
                tool_results_for(["tu2"], at=ts(sec=6, ms=200)),
            ]
            catalog = [
                {"suite": "S1", "case_id": "c1", "task_payload": e, "expected_text": e},
                {"suite": "S1", "case_id": "c2", "task_payload": e, "expected_text": e},
            ]
            report = run_analyzer(tmp, events, transcript, catalog)
            c1 = report["cases"]["S1::c1"]
            c2 = report["cases"]["S1::c2"]
            self.assertEqual(c1["attempts"], 0)
            self.assertIn("payload_without_check", c1["alignment_flags"])
            self.assertIsNone(c1["final_exact"])
            self.assertIs(c2["final_exact"], True)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 1)

    def test_duplicate_check_before_next_payload_not_stolen(self):
        """Duplicate check lands before the next payload arrives: it must not
        steal the next payload (which belongs to the next case) — the
        duplicate stays unresolved and the next case stays exact."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "d1", ts(sec=0), expected_text=e),
                check("S1", "d1", ts(sec=3), e, e),
                check("S1", "d1", ts(sec=3, ms=500), e, e),  # duplicate
                trial("S1", "d2", ts(sec=4), expected_text=e),
                check("S1", "d2", ts(sec=6), e, e),
            ]
            transcript = [
                ("assistant", "glm-5.3-flash",
                 [("tu1", text_input_action(e))], ts(sec=1)),
                ("assistant", "glm-5.3-flash",
                 [("tu2", text_input_action(e))], ts(sec=5)),
                tool_results_for(["tu1", "tu2"], at=ts(sec=1, ms=200)),
            ]
            catalog = [
                {"suite": "S1", "case_id": "d1", "task_payload": e, "expected_text": e},
                {"suite": "S1", "case_id": "d2", "task_payload": e, "expected_text": e},
            ]
            report = run_analyzer(tmp, events, transcript, catalog)
            d1 = report["cases"]["S1::d1"]
            d2 = report["cases"]["S1::d2"]
            self.assertEqual(d1["attempts"], 1)
            self.assertEqual(len(d1["unresolved_checks"]), 1)
            self.assertIs(d1["first_attempt_exact"], True)
            self.assertIs(d2["final_exact"], True)
            self.assertEqual(d2["attempts"], 1)

    def test_out_of_order_and_missing_ts_stay_unknown(self):
        """Check timestamped before its payload (inverted), payload without
        timestamp, and check without oracle timestamp: all stay unknown and
        must not corrupt the healthy case."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "o1", ts(sec=0)),
                check("S1", "o1", ts(sec=0, ms=500), e, e),   # check before payload ts
                trial("S1", "o2", ts(sec=2)),
                check("S1", "o2", ts(sec=4), e, e),           # payload has no ts
                {"type": "trial", "trial": 0, "nonce": "N", "suite": "S1", "case_id": "o4",
                 "case_index": 0, "case_total": 4},           # trial without ts
                {"type": "text_check", "trial": 0, "nonce": "N", "suite": "S1",
                 "case_id": "o4", "matched": True, "expected_text": e, "actual_text": e,
                 "actual_utf16_hex": mod.utf16_code_units_hex(e),
                 "expected_utf16_hex": mod.utf16_code_units_hex(e)},  # check without ts
                trial("S1", "o3", ts(sec=3)),
                check("S1", "o3", ts(sec=4, ms=500), e, e),   # healthy
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                ("assistant", "glm-5.3-flash", [("tu2", text_input_action(e))], None),  # no ts
                ("assistant", "glm-5.3-flash", [("tu3", text_input_action(e))], ts(sec=3, ms=500)),
                ("assistant", "glm-5.3-flash", [("tu4", text_input_action(e))], ts(sec=6)),
                tool_results_for(["tu1"], at=ts(sec=1, ms=200)),
                tool_results_for(["tu2"], at=ts(sec=1, ms=300)),
                tool_results_for(["tu3"], at=ts(sec=3, ms=700)),
                tool_results_for(["tu4"], at=ts(sec=6, ms=200)),
            ]
            catalog = [
                {"suite": "S1", "case_id": "o1", "task_payload": e, "expected_text": e},
                {"suite": "S1", "case_id": "o2", "task_payload": e, "expected_text": e},
                {"suite": "S1", "case_id": "o3", "task_payload": e, "expected_text": e},
                {"suite": "S1", "case_id": "o4", "task_payload": e, "expected_text": e},
            ]
            report = run_analyzer(tmp, events, transcript, catalog)
            self.assertIsNone(report["cases"]["S1::o1"]["final_exact"])
            self.assertIsNone(report["cases"]["S1::o2"]["final_exact"])
            self.assertIsNone(report["cases"]["S1::o4"]["final_exact"])
            self.assertEqual(
                report["cases"]["S1::o4"]["unresolved_checks"][0]["reason"],
                "missing_oracle_ts")
            self.assertIs(report["cases"]["S1::o3"]["final_exact"], True)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 1)

    def test_same_second_two_candidates_unknown(self):
        """macOS second precision: two payloads in the same second as each
        other and two checks in one later second -> no provable pairing."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "m1", ts(sec=0)),
                check("S1", "m1", ts(sec=3), e, e),
                check("S1", "m1", ts(sec=3), e, e),
            ]
            transcript = [
                ("assistant", "glm-5.3-flash",
                 [("tu1", text_input_action(e)), ("tu2", text_input_action(e))], ts(sec=2)),
                tool_results_for(["tu1", "tu2"], at=ts(sec=2, ms=500)),
            ]
            catalog = [{"suite": "S1", "case_id": "m1", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::m1"]
            self.assertIsNone(case["final_exact"])
            self.assertTrue(all(u["reason"] == "ambiguous_candidates"
                                for u in case["unresolved_checks"]))

    def test_tool_result_completion_time_proves_pairing(self):
        """Two payloads share an input second, but their tool_result
        completion times straddle the first check: the first check can only
        have consumed the earlier-completed payload -> unique proven pairing."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "p1c", ts(sec=0)),
                check("S1", "p1c", ts(sec=2), "first", "first"),
                check("S1", "p1c", ts(sec=5), "second", "second"),
            ]
            transcript = [
                ("assistant", "glm-5.3-flash",
                 [("tu1", text_input_action("first")), ("tu2", text_input_action("second"))],
                 ts(sec=1)),
                ("user", [("tu1", "ok")], ts(sec=1, ms=500)),
                ("user", [("tu2", "ok")], ts(sec=3)),
            ]
            catalog = [{"suite": "S1", "case_id": "p1c", "task_payload": "first", "expected_text": "first"}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::p1c"]
            self.assertEqual(case["attempts"], 2)
            recs = case["attempt_records"]
            self.assertEqual(recs[0]["payload"], "first")
            self.assertEqual(recs[1]["payload"], "second")
            self.assertEqual(recs[0]["verdict"], "exact")
            # second check: payload "second" == task_payload "first" -> raw deviation
            self.assertEqual(recs[1]["verdict"], "payload_mismatch")
            self.assertIs(case["first_attempt_exact"], True)
            self.assertIs(case["final_exact"], False)

    def test_cross_case_duplicate_text_not_mispaired(self):
        """Identical text across cases: trial-interval assignment keeps each
        payload in its own case. A payload that arrived before the next
        trial cannot be claimed by the later case."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "y1", ts(sec=0), expected_text=e),
                check("S1", "y1", ts(sec=3), e, e),
                trial("S1", "y2", ts(sec=5), expected_text=e),
                check("S1", "y2", ts(sec=7), e, e),
            ]
            transcript = [
                ("assistant", "glm-5.3-flash",
                 [("tu1", text_input_action(e))], ts(sec=1)),
                ("assistant", "glm-5.3-flash",
                 [("tu2", text_input_action(e))], ts(sec=6)),
                tool_results_for(["tu1", "tu2"], at=ts(sec=1, ms=200)),
            ]
            catalog = [
                {"suite": "S1", "case_id": "y1", "task_payload": e, "expected_text": e},
                {"suite": "S1", "case_id": "y2", "task_payload": e, "expected_text": e},
            ]
            report = run_analyzer(tmp, events, transcript, catalog)
            self.assertIs(report["cases"]["S1::y1"]["final_exact"], True)
            self.assertIs(report["cases"]["S1::y2"]["final_exact"], True)

            # negative variant: second payload arrived before y2's trial ->
            # it belongs to y1, y2 has no candidate -> unknown, no cross-case pass
            events2 = [
                trial("S1", "y1", ts(sec=0), expected_text=e),
                check("S1", "y1", ts(sec=3), e, e),
                trial("S1", "y2", ts(sec=5), expected_text=e),
                check("S1", "y2", ts(sec=7), e, e),
            ]
            transcript2 = [
                ("assistant", "glm-5.3-flash",
                 [("tu1", text_input_action(e))], ts(sec=1)),
                ("assistant", "glm-5.3-flash",
                 [("tu2", text_input_action(e))], ts(sec=4)),
                tool_results_for(["tu1", "tu2"], at=ts(sec=1, ms=200)),
            ]
            report2 = run_analyzer(tmp, events2, transcript2, catalog)
            y1 = report2["cases"]["S1::y1"]
            y2 = report2["cases"]["S1::y2"]
            self.assertEqual(y1["attempts"], 1)
            self.assertIn("payload_without_check", y1["alignment_flags"])
            self.assertIsNone(y2["final_exact"])
            self.assertEqual(y2["unresolved_checks"][0]["reason"], "no_candidate_payload")
            self.assertEqual(report2["suites"]["S1"]["final_exact"], 1)

    def test_payload_mismatch_never_counts_as_pass(self):
        """F1 regression: wrong payload with oracle-internal expected==actual
        must NOT produce exact/pass counts (review counter-example R1)."""
        with tempfile.TemporaryDirectory() as tmp:
            events, entries, catalog = simple_case(
                [], "bbb", "aaa", "aaa", "aaa", case_id="r1")
            report = run_analyzer(tmp, events, entries, catalog)
            case = report["cases"]["S1::r1"]
            rec = case["attempt_records"][0]
            self.assertEqual(rec["verdict"], "payload_mismatch")
            self.assertIs(rec["exact"], False)
            self.assertIs(case["final_exact"], False)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 0)
            self.assertIn("raw_task_payload_deviation",
                          [f["class"] for f in case["failures"]])


    # --------------------------------------- live-run boundary regressions

    def test_partial_run_observed_vs_expected(self):
        """Live-run shape: 4 trials / 3 checks against a 10-case catalog.
        observed_case_count must count only oracle-observed cases (4);
        case_count stays the catalog expectation (10); attempts = 3."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = []
            for i in range(4):
                events.append(trial("S1", "p%d" % i, ts(sec=i * 3), expected_text=e))
            for i in range(3):
                events.append(check("S1", "p%d" % i, ts(sec=i * 3 + 2), e, e))
            transcript = []
            for i in range(3):
                transcript.append(("assistant", "glm-5.3-flash",
                                   [("tu%d" % i, text_input_action(e))], ts(sec=i * 3 + 1)))
                transcript.append(tool_results_for(["tu%d" % i], at=ts(sec=i * 3 + 1, ms=500)))
            catalog = [{"suite": "S1", "case_id": "p%d" % i, "task_payload": e,
                        "expected_text": e} for i in range(10)]
            report = run_analyzer(tmp, events, transcript, catalog)
            s = report["suites"]["S1"]
            self.assertEqual(s["observed_case_count"], 4)
            self.assertEqual(s["case_count"], 10)
            self.assertEqual(s["attempts"], 3)
            self.assertEqual(report["oracle_stats"]["proven_pairs"], 3)

    def test_first_unknown_then_success_not_upgraded(self):
        """First check unresolved (logged before any payload), retry later
        proven exact: first_attempt_exact stays None, final_exact True."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "f1", ts(sec=0)),
                check("S1", "f1", ts(sec=1), e, "zzz"),   # no candidate yet -> unknown
                check("S1", "f1", ts(sec=3), e, e),        # proven exact
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=2)),
                tool_results_for(["tu1"], at=ts(sec=2, ms=200)),
            ]
            catalog = [{"suite": "S1", "case_id": "f1", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::f1"]
            self.assertIsNone(case["first_attempt_exact"])
            self.assertIs(case["final_exact"], True)
            self.assertEqual(case["check_sequence"][0]["unresolved_reason"], "no_candidate_payload")
            self.assertIs(case["check_sequence"][1]["proven_exact"], True)
            self.assertEqual(report["suites"]["S1"]["first_attempt_exact"], 0)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 1)

    def test_first_failure_then_unknown_final_stays_unknown(self):
        """First check proven failure, trailing check unresolved:
        first=False, final=None (no inheritance from the earlier success)."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "f2", ts(sec=0)),
                check("S1", "f2", ts(sec=2), e, "abd"),    # proven failure
                check("S1", "f2", ts(sec=4), e, e),        # orphan: no 2nd payload
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"], at=ts(sec=1, ms=200)),
            ]
            catalog = [{"suite": "S1", "case_id": "f2", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::f2"]
            self.assertIs(case["first_attempt_exact"], False)
            self.assertIsNone(case["final_exact"])
            self.assertEqual(report["suites"]["S1"]["first_attempt_exact"], 0)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 0)

    def test_success_then_unknown_final_stays_unknown(self):
        """Proven exact first, trailing unresolved: first=True, final=None."""
        with tempfile.TemporaryDirectory() as tmp:
            e = "abc"
            events = [
                trial("S1", "f3", ts(sec=0)),
                check("S1", "f3", ts(sec=2), e, e),        # proven exact
                check("S1", "f3", ts(sec=4), e, e),        # orphan duplicate-style
            ]
            transcript = [
                ("assistant", "glm-5.3-flash", [("tu1", text_input_action(e))], ts(sec=1)),
                tool_results_for(["tu1"], at=ts(sec=1, ms=200)),
            ]
            catalog = [{"suite": "S1", "case_id": "f3", "task_payload": e, "expected_text": e}]
            report = run_analyzer(tmp, events, transcript, catalog)
            case = report["cases"]["S1::f3"]
            self.assertIs(case["first_attempt_exact"], True)
            self.assertIsNone(case["final_exact"])
            self.assertEqual(report["suites"]["S1"]["first_attempt_exact"], 1)
            self.assertEqual(report["suites"]["S1"]["final_exact"], 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
