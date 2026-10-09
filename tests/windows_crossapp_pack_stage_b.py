"""Additional Stage B contracts; CC only. No change to Stage A's 49 methods.

All evidence below is deliberately synthetic owned test data, NOT GUI actions.
"""
import codecs
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "acceptance-fixture" / "crossapp"))
from crossapp_pack import prepare_owned_pack
from crossapp_artifacts import check_artifacts, check_notepad_bytes
from crossapp_browser_check import check_browser_export
from crossapp_io import OwnedIO
from crossapp_tasks import task_text


class StageB(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="crossapp-stage-b-unit-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "new-pack"

    def prepare(self):
        self.manifest = prepare_owned_pack(str(self.root))
        self.oracle = json.loads((self.root / "supervisor/oracle.json").read_bytes())

    def result(self, number):
        report = check_artifacts(str(self.root))
        self.assertIs(report["gui_verified"], False)
        self.assertIs(report["ownership_verified"], False)
        self.assertIs(report["action_trial_gate_satisfied"], False)
        return report["cases"][number - 1]

    def browser_data(self):
        case = self.manifest["cases"][3]
        oracle = self.oracle["crossapp-04"]
        details = [
            ("download_gate", dict(confirmed=True)),
            ("scroll_y", dict(position=1100)), ("scroll_x", dict(position=900)),
            ("form_submit", dict(text=oracle["expected_form_text"], choice="Teal")),
            ("drag_start", dict(x=10, y=20)), ("drag_motion", dict(x=100, y=20)),
            ("drop", dict(x=150, y=20, target="drop-target")),
            ("drag_end", dict(dropped=True)), ("export_requested", {})]
        return dict(schema="windows-crossapp-browser-events-v1",
                    **{k: case[k] for k in ("case_id", "run_id", "trial_id", "nonce")},
                    events_truncated=False, untrusted_application_data=True,
                    gui_verified=True, download_confirmation=True,
                    events=[dict(index=i, elapsed_ms=i * 100, kind=k, trusted=True, **d)
                            for i, (k, d) in enumerate(details, 1)],
                    final=dict(text=oracle["expected_form_text"], choice="Teal", dropped=True))

    def test_empty_nonce_asset_is_not_a_result(self):
        self.prepare()
        case = self.manifest["cases"][0]
        self.assertIn(case["nonce"], case["initial_document"])
        self.assertEqual((self.root / case["initial_document"]).read_bytes(), b"")
        self.assertFalse((self.root / self.oracle["crossapp-01"]["output_path"]).exists())
        self.assertEqual(self.result(1)["state"], "needs_evidence")

    def test_reuse_requires_distinct_outer_inner_identity_binding(self):
        self.prepare()
        for number in (5, 6):
            with self.subTest(number=number):
                case = self.manifest["cases"][number - 1]
                reuse = case["reuse"]
                self.assertIs(reuse["pack_nonce_is_fixture_nonce"], False)
                self.assertEqual(reuse["binding_status"], "needs_preflight")
                self.assertEqual(reuse["inner_bindings"], [])
                for k in ("actual_fixture_run_id", "actual_fixture_pid", "actual_fixture_creation_time",
                          "actual_fixture_hwnds", "actual_inner_case_id", "actual_inner_trial", "actual_inner_nonce"):
                    self.assertIsNone(reuse[k], k)
                task = (self.root / case["agent_task"]).read_text(encoding="utf-8")
                self.assertIn("needs_preflight / needs_evidence; STOP", task)
                self.assertIn("has its OWN case/trial/nonce labels", task)
                self.assertEqual(self.result(number)["state"], "needs_evidence")

    def test_calculator_does_not_require_a_nonce_ui(self):
        self.prepare()
        task = (self.root / self.manifest["cases"][1]["agent_task"]).read_text(encoding="utf-8")
        self.assertIn("Calculator has NO nonce display", task)
        self.assertIn("exact newly owned HWND", task)
        self.assertIn("needs_ownership", task)

    def test_tampered_expected_cannot_make_wrong_text_match(self):
        self.prepare()
        item = self.oracle["crossapp-01"]
        raw = b"forged\n"
        (self.root / item["output_path"]).write_bytes(raw)
        item["expected_text"] = item["payload"] = "forged\n"
        (self.root / "supervisor/oracle.json").write_bytes(json.dumps(self.oracle).encode())
        self.assertEqual(self.result(1)["state"], "unknown")
        self.assertEqual((self.root / item["output_path"]).read_bytes(), raw)

    def test_utf32_and_bomless_utf16_not_equivalent(self):
        expected = "ABC\r\n"
        for raw in (codecs.BOM_UTF32_LE + expected.encode("utf-32-le"),
                    codecs.BOM_UTF32_BE + expected.encode("utf-32-be"), expected.encode("utf-16-le")):
            with self.subTest(raw=raw):
                result = check_notepad_bytes(raw, expected)
                self.assertNotEqual(result["state"], "artifact_match")
                self.assertIs(result["gui_verified"], False)

    def test_rechecks_reparse_after_io_object_created(self):
        self.root.mkdir()
        target = self.root / "artifact.txt"
        target.write_bytes(b"preserve")
        io = OwnedIO(self.root)
        real = os.lstat
        def changed(path, *args, **kwargs):
            s = real(path, *args, **kwargs)
            if Path(path) == target:
                values = {k: getattr(s, k) for k in dir(s) if k.startswith("st_")}
                values["st_file_attributes"] = values.get("st_file_attributes", 0) | 0x400
                return SimpleNamespace(**values)
            return s
        with mock.patch("os.lstat", side_effect=changed):
            with self.assertRaises(ValueError): io.read("artifact.txt", 100)
        self.assertEqual(target.read_bytes(), b"preserve")

    def test_file_write_never_overwrites(self):
        self.root.mkdir()
        target = self.root / "artifact.txt"
        target.write_bytes(b"preserve")
        with self.assertRaises(FileExistsError): OwnedIO(self.root).write_new("artifact.txt", b"changed")
        self.assertEqual(target.read_bytes(), b"preserve")

    def test_duplicate_private_json_key_rejected(self):
        self.prepare()
        file = self.root / "supervisor/manifest.json"
        file.write_bytes(b'{"schema":"one","schema":"two"}')
        with self.assertRaises(ValueError): check_artifacts(str(self.root))

    def test_synthetic_browser_match_never_proves_gui(self):
        self.prepare()
        raw = json.dumps(self.browser_data()).encode()
        path = self.root / self.oracle["crossapp-04"]["event_export_path"]
        path.write_bytes(raw)
        result = self.result(4)
        self.assertEqual(result["state"], "artifact_match")
        self.assertIs(result["gui_verified"], False)
        self.assertIs(result["application_data_only"], True)
        self.assertEqual(path.read_bytes(), raw)

    def test_browser_rejects_missing_motion_identity_and_retries(self):
        self.prepare()
        data = self.browser_data()
        variants = []
        wrong_identity = copy.deepcopy(data); wrong_identity["nonce"] = "another-trial"
        variants.append(wrong_identity)
        missing_motion = copy.deepcopy(data); missing_motion["events"][5]["kind"] = "scroll_y"
        missing_motion["events"][5]["position"] = 20; variants.append(missing_motion)
        retry = copy.deepcopy(data); retry["events"][2].update(kind="form_submit", text="wrong", choice="Teal")
        variants.append(retry)
        wrong_first = copy.deepcopy(data); wrong_first["events"][3]["text"] = "wrong"
        variants.append(wrong_first)
        for item in variants:
            with self.subTest(events=item["events"]):
                state, _ = check_browser_export(item, self.manifest["cases"][3], self.oracle["crossapp-04"])
                self.assertIn(state, ("unknown", "mismatch"))

    def test_browser_malformed_types_are_unknown_not_crash(self):
        self.prepare()
        for field, value in (("kind", []), ("elapsed_ms", 10 ** 400), ("index", True), ("trusted", "true")):
            with self.subTest(field=field):
                data = self.browser_data(); data["events"][0][field] = value
                state, _ = check_browser_export(data, self.manifest["cases"][3], self.oracle["crossapp-04"])
                self.assertEqual(state, "unknown")

    def test_focus_wrapper_has_ordered_same_session_pause_resume_prelude(self):
        # Static task contract only: no preparation, session or GUI is launched.
        original = (REPO / "acceptance-fixture/tasks/windows-focus.md").read_bytes().decode("utf-8")
        case = dict(case_id="crossapp-06", application="owned native focus fixture",
                    run_id="outer-run", trial_id="outer-trial", nonce="outer-nonce",
                    reuse=dict(suite="focus"))
        task = task_text(case, {}, self.root, original)
        embedded = task.split("--- BEGIN UNCHANGED ORIGINAL TASK ---\n", 1)[1].split(
            "\n--- END UNCHANGED ORIGINAL TASK ---", 1)[0]
        self.assertEqual(embedded, original)
        prelude = task.split("### crossapp-06 session prelude", 1)[1].split(
            "--- BEGIN UNCHANGED ORIGINAL TASK ---", 1)[0]
        ordered = ("after embedded general step 1", "before ANY target input",
                   "1. Call computer_pause", "2. Check the actual paused reply",
                   "3. Call computer_resume", "4. Check the actual resume reply",
                   "5. Call computer_observe", "continue embedded general step 2")
        positions = [prelude.index(text) for text in ordered]
        self.assertEqual(positions, sorted(positions))
        for required in ("future GUI authorization", "supervisor has confirmed ownership of A/B",
                         "SAME session", "No Observe while paused", "No desktop input while paused",
                         "Any error/unknown", "computer_close immediately, then STOP",
                         "NO tools after focus-10's final computer_close", "No held-cancel test",
                         "Self-report is NOT GUI verification"):
            self.assertIn(required, prelude)
        self.assertNotIn("requires separately approved", prelude)
        for cid, suite, source in (("crossapp-05", "drag", "drag.md"),
                                   ("crossapp-06", "focus", "windows-focus.md")):
            with self.subTest(case=cid):
                original_task = (REPO / "acceptance-fixture/tasks" / source).read_bytes().decode("utf-8")
                native = dict(case, case_id=cid, reuse=dict(suite=suite))
                rendered = task_text(native, {}, self.root, original_task)
                wrapper = rendered.split("--- BEGIN UNCHANGED ORIGINAL TASK ---", 1)[0]
                self.assertIn("ONE shared Describe/Open", wrapper)
                self.assertIn("embedded original task controls case progression", wrapper)
                self.assertIn("After original final Close: text report ONLY, ZERO additional tools", wrapper)
                self.assertNotIn("At the end Observe, Close", wrapper)
                self.assertIn(original_task, rendered)
                if suite == "drag": self.assertNotIn("### crossapp-06 session prelude", wrapper)

    def test_browser_malformed_json_preserved(self):
        self.prepare()
        path = self.root / self.oracle["crossapp-04"]["event_export_path"]
        for raw in (b'{"events":', b'{"schema":"a","schema":"b"}', b'{"number":NaN}'):
            with self.subTest(raw=raw):
                path.write_bytes(raw)
                self.assertEqual(self.result(4)["state"], "unknown")
                self.assertEqual(path.read_bytes(), raw)


if __name__ == "__main__":
    unittest.main()
