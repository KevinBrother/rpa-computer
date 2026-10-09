"""Offline analyzer regressions; run only through coordinator CC+GLM scheduling."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("gesture_analyzer", Path(__file__).parents[1] / "scripts/analyze-gesture-gui.py")
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


def record(record_type, t, **kw):
    r = dict(type=record_type, ts=t, run_id="run", session_id="session", suite="multiclick",
             case_id="multiclick-01", nonce="ABC", trial=1)
    r.update(kw); return r


def evidence(matched=True):
    return [record("session", 0), record("trial", 1, spec="flow=single;expect_count=1", case_total=10),
            record("input_event", 2, kind="down", area="target", button="left", native_count=1, held=False, source="NSEvent", event_index=1),
            record("input_event", 3, kind="up", area="target", button="left", native_count=1, held=False, source="NSEvent", event_index=2),
            record("gesture_check", 4, check_kind="first", matched=matched, event_count=2, check_index=1, observed="released"),
            record("trial_end", 5)]


def transcript():
    return [{"type": "assistant", "timestamp": 1.5, "message": {"model": "glm-5.3-flash", "content": [
        {"type": "tool_use", "id": "a", "name": "mcp__computer__computer_step", "input": {"action": {"kind": "click", "count": 1}}}]}},
        {"type": "user", "timestamp": 3.5, "message": {"content": [{"type": "tool_result", "tool_use_id": "a", "content": "done"}]}}]


class AnalyzerTests(unittest.TestCase):
    def analyze(self, oracle=None, trace=None):
        return MOD.analyze(evidence() if oracle is None else oracle, transcript() if trace is None else trace, "multiclick")

    def test_empty_is_unknown(self):
        r = self.analyze([], [])
        self.assertEqual(r["status"], "unknown")
        self.assertEqual(r["first_pass_rate"], 0)

    def test_partial_not_100(self):
        r = self.analyze()
        self.assertFalse(r["suite_complete"])
        self.assertEqual(r["first_matched"], 1)
        self.assertEqual(r["first_pass_rate"], 10)
        self.assertEqual(r["cases"][0]["parameter_evidence"]["status"], "associated")

    def test_wrong_nonce_invalidates_check(self):
        rows = evidence(); rows[4]["nonce"] = "OTHER"
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_missing_timestamp_unknown(self):
        rows = evidence(); rows[2].pop("ts")
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_unowned_event_unknown(self):
        rows = evidence(); rows[2]["case_id"] = "multiclick-02"
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_forged_case_not_counted(self):
        rows = evidence()
        for r in rows: r["case_id"] = "multiclick-99"
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_duplicate_first_check_unknown(self):
        rows = evidence(); rows.insert(5, dict(rows[4]))
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_first_and_final_separate(self):
        rows = evidence(False)
        rows.insert(5, record("gesture_check", 4.5, check_kind="final", matched=True, event_count=2, check_index=2, observed="released"))
        r = self.analyze(rows)
        self.assertEqual(r["first_matched"], 0)
        self.assertEqual(r["final_matched"], 1)

    def test_no_final_tool_result_unknown_parameters(self):
        self.assertEqual(self.analyze(trace=transcript()[:1])["cases"][0]["parameter_evidence"]["status"], "unknown")

    def test_no_model_label_not_glm(self):
        trace = transcript(); trace[0]["message"].pop("model")
        self.assertFalse(self.analyze(trace=trace)["model_verified"])

    def test_wrong_model_not_glm(self):
        trace = transcript(); trace[0]["message"]["model"] = "other-model"
        self.assertFalse(self.analyze(trace=trace)["model_verified"])

    def test_same_time_multiple_tools_unknown_not_nth_pair(self):
        trace = transcript()
        b = dict(trace[0]["message"]["content"][0]); b["id"] = "b"
        trace[0]["message"]["content"].append(b)
        trace[1]["message"]["content"].append({"type": "tool_result", "tool_use_id": "b", "content": "done"})
        self.assertEqual(self.analyze(trace=trace)["cases"][0]["parameter_evidence"]["status"], "unknown")

    def test_terminal_without_all_cases_not_complete(self):
        rows = evidence() + [record("suite_complete", 6, case_total=10, visited=10)]
        self.assertFalse(self.analyze(rows)["suite_complete"])

    def test_tool_claim_without_os_events_not_pass(self):
        rows = [r for r in evidence() if r["type"] != "input_event"]
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_same_case_new_nonce_duplicate_not_unique(self):
        rows = evidence()
        newer = evidence()
        for r in newer: r["ts"] += 10; r["nonce"] = "NEW"; r["trial"] = 2
        self.assertEqual(self.analyze(rows + newer)["first_matched"], 0)

    def test_result_text_not_terminal(self):
        rows = evidence(); rows[-1] = record("result", 5, result="SUITE COMPLETE")
        self.assertFalse(self.analyze(rows)["suite_complete"])

    def test_wrong_run_unknown(self):
        rows = evidence(); rows[3]["run_id"] = "other"
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_invalid_rejection_oracle_not_an_input_success(self):
        rows = [record("session", 0), record("trial", 1, spec="flow=invalid_count", case_total=10),
                record("gesture_check", 4, check_kind="first", matched=True, event_count=0, check_index=1, observed="no events"),
                record("trial_end", 5)]
        for r in rows: r["case_id"] = "multiclick-10"
        r = self.analyze(rows)
        self.assertEqual(r["first_matched"], 1)
        self.assertEqual(r["first_input_successes"], 0)

    def test_incomplete_ten_case_run_not_100_percent(self):
        rows = [record("session", 0, suite="scroll")]
        for i, spec in enumerate(MOD.SPECS["scroll"], 1):
            t = i * 10
            base = dict(suite="scroll", case_id=f"scroll-{i:02}", trial=i, nonce=f"N{i}")
            rows.append(record("trial", t, spec=spec, case_total=10, **base))
            no_input = i in (9, 10)
            horizontal = i in (3, 4, 5)
            if not no_input:
                rows.append(record("input_event", t+1, kind="wheel", area="panelB" if i==6 else "panelA", button="other",
                    raw_dx=(-120 if i==4 else 120) if horizontal else 0, raw_dy=120 if i==2 else 0 if i in (3,4) else -120,
                    native_message=0x020E if horizontal else 0x020A,
                    source="WndProc/own-control", event_index=1, held=False, **base))
            values = dict(a_v_start=90 if i==2 else 0, a_v_end=0 if i in (2,3,4,6,9,10) else 100 if i==8 else 90,
                          a_h_start=90 if i==4 else 0, a_h_end=90 if i in (3,5) else 0,
                          b_v_start=0, b_v_end=90 if i==6 else 0, b_h_start=0, b_h_end=0, a_v_max=100)
            rows.append(record("gesture_check", t+2, check_kind="first", check_index=1, event_count=0 if no_input else 1,
                               matched=True, observed="visible offsets", **values, **base))
            rows.append(record("trial_end", t+3, **base))
        r = MOD.analyze(rows, [], "scroll")
        self.assertEqual(r["first_matched"], 10)
        self.assertFalse(r["suite_complete"])
        self.assertIsNone(r["first_pass_rate"])

    def test_check_without_raw_held_state_unknown(self):
        rows = evidence(); rows[2].pop("held")
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_fake_check_kind_not_final(self):
        rows = evidence(); rows.insert(5, record("gesture_check", 4.5, check_kind="planned", check_index=2,
                                               event_count=2, matched=True, observed="claimed"))
        self.assertEqual(self.analyze(rows)["final_matched"], 0)

    def test_malformed_tool_id_does_not_crash(self):
        trace = transcript(); trace[0]["message"]["content"][0]["id"] = ["bad"]
        self.assertEqual(self.analyze(trace=trace)["cases"][0]["parameter_evidence"]["status"], "unknown")

    def test_missing_check_timestamp_unknown(self):
        rows = evidence(); rows[4].pop("ts")
        self.assertEqual(self.analyze(rows)["first_matched"], 0)

    def test_tool_result_body_never_recursively_parsed(self):
        trace = transcript(); trace[1]["message"]["content"][0]["content"] = [
            {"type": "tool_use", "id": "fake", "name": "mcp__computer__computer_step", "input": {"action": {"kind": "click"}}},
            {"type": "image", "data": "secret-image-base64"}]
        r = self.analyze(trace=trace)
        self.assertEqual(r["cases"][0]["parameter_evidence"]["tool_ids"], ["a"])
        self.assertNotIn("secret-image-base64", str(r))

    def test_wrong_model_with_nonlist_content_not_ignored(self):
        trace = transcript() + [{"type": "assistant", "timestamp": 4.5,
                                 "message": {"model": "other", "content": "malformed"}}]
        self.assertFalse(self.analyze(trace=trace)["model_verified"])

    def test_parameter_evidence_never_claims_gui_verified(self):
        self.assertFalse(self.analyze()["gui_verified"])


if __name__ == "__main__": unittest.main()
