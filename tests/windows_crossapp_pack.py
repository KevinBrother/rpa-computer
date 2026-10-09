"""Stage A executable requirements. CC runs on Windows; author only py_compile.

All API calls reach explicit missing-capability stubs. The call() adapter reports
that genuine NotImplementedError as a named unittest FAIL, not a skip or success.
TemporaryDirectory cleanup belongs ONLY to these synthetic owned test fixtures;
the future preparation/checker APIs must not delete or launch anything.
"""
import codecs
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "acceptance-fixture" / "crossapp"))
import crossapp_artifacts as artifacts
import crossapp_pack as pack
import crossapp_paths as paths

TOOLS = {"computer_" + n for n in ("describe", "open", "observe", "step", "get_step", "pause", "resume", "close")}
CASE_IDS = ["crossapp-%02d" % n for n in range(1, 7)]
EXPECTED = "ASCII: Hello 123\r\nUnicode: 中文 Ω 😀 e\u0301\r\nTrailing space: END \r\n"


class Requirements(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="crossapp-unit-owned-")
        self.addCleanup(self.tmp.cleanup)
        self.parent = Path(self.tmp.name)
        self.root = self.parent / "new-pack"

    def call(self, fn, *args):
        try:
            return fn(*args)
        except NotImplementedError as exc:
            self.fail("missing capability from %s: %s" % (fn.__name__, exc))

    def prepared(self):
        manifest = self.call(pack.prepare_owned_pack, str(self.root))
        oracle = json.loads((self.root / "supervisor" / "oracle.json").read_bytes())
        return manifest, oracle

    def safe(self, relative):
        return self.call(paths.resolve_owned_path, str(self.root), relative)

    def case(self, manifest, number):
        return next(c for c in manifest["cases"] if c["case_id"] == "crossapp-%02d" % number)

    def artifact_result(self, number):
        report = self.call(artifacts.check_artifacts, str(self.root))
        self.assertIs(report["gui_verified"], False)
        self.assertIs(report["action_trial_gate_satisfied"], False)
        return next(c for c in report["cases"] if c["case_id"] == "crossapp-%02d" % number)

    def fake_entry(self, entry, *, symlink=False, reparse=False):
        """No real links/mklink privileges/CLI needed: lstat facts of our own path."""
        real = os.lstat
        def inspect(path, *args, **kwargs):
            result = real(path, *args, **kwargs)
            if isinstance(path, (str, bytes, os.PathLike)) and Path(path) == entry:
                attrs = {k: getattr(result, k) for k in dir(result) if k.startswith("st_")}
                if symlink: attrs["st_mode"] = stat.S_IFLNK | 0o777
                if reparse: attrs["st_file_attributes"] = attrs.get("st_file_attributes", 0) | 0x400
                return SimpleNamespace(**attrs)
            return result
        return mock.patch.object(paths.os, "lstat", side_effect=inspect)

    def test_new_absolute_root_validation_does_not_create(self):
        self.assertEqual(self.call(paths.validate_new_root, str(self.root)), self.root)
        self.assertFalse(self.root.exists())

    def test_existing_empty_root_rejected(self):
        self.root.mkdir()
        with self.assertRaises(ValueError): self.call(paths.validate_new_root, str(self.root))
        self.assertEqual(list(self.root.iterdir()), [])

    def test_existing_nonempty_root_preserved(self):
        self.root.mkdir(); marker = self.root / "unknown.txt"; marker.write_bytes(b"DO NOT DELETE")
        with self.assertRaises(ValueError): self.call(pack.prepare_owned_pack, str(self.root))
        self.assertEqual(marker.read_bytes(), b"DO NOT DELETE")

    def test_relative_root_rejected(self):
        with self.assertRaises(ValueError): self.call(paths.validate_new_root, "relative-pack")

    def test_unc_root_rejected(self):
        with self.assertRaises(ValueError): self.call(paths.validate_new_root, r"\\server\share\pack")

    def test_device_namespace_rejected(self):
        with self.assertRaises(ValueError): self.call(paths.validate_new_root, r"\\?\C:\pack")

    def test_drive_relative_rejected(self):
        with self.assertRaises(ValueError): self.call(paths.validate_new_root, "C:pack")

    def test_root_traversal_not_normalized_away(self):
        with self.assertRaises(ValueError): self.call(paths.validate_new_root, str(self.parent / "x" / ".." / "pack"))

    def test_root_symlink_ancestor_rejected_without_link_privilege(self):
        with self.fake_entry(self.parent, symlink=True):
            with self.assertRaises(ValueError): self.call(paths.validate_new_root, str(self.root))

    def test_root_junction_ancestor_rejected_without_mklink(self):
        with self.fake_entry(self.parent, reparse=True):
            with self.assertRaises(ValueError): self.call(paths.validate_new_root, str(self.root))

    def test_resolve_inside_root(self):
        self.root.mkdir(); (self.root / "results").mkdir()
        self.assertEqual(self.safe("results/file.txt"), self.root / "results" / "file.txt")

    def test_resolve_traversal_both_separators(self):
        self.root.mkdir()
        for name in ("../outside.txt", r"..\outside.txt", "results/../outside.txt"):
            with self.subTest(name=name), self.assertRaises(ValueError): self.safe(name)

    def test_resolve_absolute_and_unc_rejected(self):
        self.root.mkdir()
        for name in (str(self.parent / "outside.txt"), r"C:\outside.txt", r"\\server\share\a", "C:relative.txt"):
            with self.subTest(name=name), self.assertRaises(ValueError): self.safe(name)

    def test_resolve_ads_reserved_trailing_alias_rejected(self):
        self.root.mkdir()
        for name in ("file.txt:stream", "NUL", "CON.txt", "name.", "name ", "LPT1.txt"):
            with self.subTest(name=name), self.assertRaises(ValueError): self.safe(name)

    def test_resolve_symlink_leaf_rejected(self):
        self.root.mkdir(); leaf = self.root / "link.txt"; leaf.write_bytes(b"owned synthetic link stand-in")
        with self.fake_entry(leaf, symlink=True):
            with self.assertRaises(ValueError): self.safe("link.txt")

    def test_resolve_junction_component_rejected(self):
        self.root.mkdir(); sub = self.root / "junction"; sub.mkdir()
        with self.fake_entry(sub, reparse=True):
            with self.assertRaises(ValueError): self.safe("junction/file.txt")

    def test_six_cases_unique_and_initially_unverified(self):
        m, _ = self.prepared()
        self.assertEqual(m["schema"], "windows-crossapp-pack-v1")
        self.assertEqual([c["case_id"] for c in m["cases"]], CASE_IDS)
        self.assertIs(m["gui_verified"], False); self.assertIs(m["action_trial_gate_satisfied"], False)
        for field in ("run_id", "trial_id", "nonce"):
            values = [c[field] for c in m["cases"]]
            self.assertEqual(len(set(values)), 6)
            self.assertTrue(all(isinstance(v, str) and len(v) >= 12 for v in values))
        for c in m["cases"]:
            self.assertIn(c["status"], ("pending", "needs_evidence"))
            self.assertEqual(set(c["allowed_tools"]), TOOLS)

    def test_no_fabricated_window_ownership(self):
        m, _ = self.prepared()
        for c in m["cases"]:
            own = c["ownership"]
            self.assertEqual(own["status"], "needs_preflight")
            self.assertIsNone(own["owned"])
            for field in ("exe_path", "exe_sha256", "pid", "creation_time", "hwnd", "interactive_session_id", "visible_title_nonce"):
                self.assertIsNone(own[field], field)
        self.assertIn(24332, m["protected_user_pids"])

    def test_agent_supervisor_zones_do_not_leak(self):
        m, _ = self.prepared()
        self.assertEqual(json.loads((self.root / "supervisor" / "manifest.json").read_bytes()), m)
        files = sorted(p.relative_to(self.root / "agent").as_posix() for p in (self.root / "agent").rglob("*") if p.is_file())
        self.assertEqual(files, [cid + "/task.md" for cid in CASE_IDS])
        for c in m["cases"]:
            task = (self.root / c["agent_task"]).read_text(encoding="utf-8")
            self.assertEqual(c["agent_task"], "agent/" + c["case_id"] + "/task.md")
            self.assertIn(c["nonce"], task); self.assertIn(c["trial_id"], task)
            self.assertNotIn(str(self.root / "supervisor"), task)
            self.assertNotIn("supervisor/oracle.json", task); self.assertNotIn("expected_utf16_hex", task)
            self.assertNotRegex(task, r"(?im)^\s*(?:python|powershell|cmd\.exe)\s")
            for tool in TOOLS: self.assertIn(tool, task)
            for gate in ("STOP", "computer_close", "CDP", "a11y", "shell"):
                self.assertIn(gate, task)

    def test_separate_preparations_do_not_reuse_nonce(self):
        one = self.call(pack.prepare_owned_pack, str(self.root))
        two = self.call(pack.prepare_owned_pack, str(self.parent / "other-pack"))
        self.assertTrue({c["nonce"] for c in one["cases"]}.isdisjoint(c["nonce"] for c in two["cases"]))

    def test_prepare_does_not_launch_apps_or_network(self):
        with mock.patch("subprocess.Popen", side_effect=AssertionError("preparer launched a process")), mock.patch("os.system", side_effect=AssertionError("shell launch")), mock.patch("socket.socket", side_effect=AssertionError("network/listener")):
            self.call(pack.prepare_owned_pack, str(self.root))

    def test_notepad_platform_expected_is_explicit_crlf(self):
        m, o = self.prepared(); c = self.case(m, 1); item = o["crossapp-01"]
        expected = "ASCII: Hello 123\r\nUnicode: 中文 Ω 😀 e\u0301\r\nNonce: " + c["nonce"] + "\r\nTrailing space: END \r\n"
        self.assertEqual(item["payload"], expected); self.assertEqual(item["expected_text"], expected)
        self.assertEqual(item["newline_policy"], "windows-crlf-exact-v1")
        self.assertTrue(item["output_path"].startswith("results/crossapp-01/"))
        task = (self.root / c["agent_task"]).read_text(encoding="utf-8")
        self.assertIn("text_input", task); self.assertIn("key_chord", task)
        self.assertIn("24332", task)

    def test_strict_utf8_match_preserves_raw_hash(self):
        raw = EXPECTED.encode("utf-8")
        r = self.call(artifacts.check_notepad_bytes, raw, EXPECTED)
        self.assertEqual(r["state"], "artifact_match"); self.assertEqual(r["actual_text"], EXPECTED)
        self.assertEqual(r["actual_sha256"], hashlib.sha256(raw).hexdigest()); self.assertIs(r["gui_verified"], False)

    def test_explicit_utf_boms(self):
        for bom, encoding, label in ((codecs.BOM_UTF8, "utf-8", "utf-8-sig"), (codecs.BOM_UTF16_LE, "utf-16-le", "utf-16-le-bom"), (codecs.BOM_UTF16_BE, "utf-16-be", "utf-16-be-bom")):
            with self.subTest(encoding=encoding):
                raw = bom + EXPECTED.encode(encoding)
                r = self.call(artifacts.check_notepad_bytes, raw, EXPECTED)
                self.assertEqual(r["state"], "artifact_match"); self.assertEqual(r["encoding"], label)
                self.assertEqual(r["actual_text"], EXPECTED); self.assertIs(r["gui_verified"], False)

    def test_lf_is_not_crlf(self):
        actual = EXPECTED.replace("\r\n", "\n")
        r = self.call(artifacts.check_notepad_bytes, actual.encode("utf-8"), EXPECTED)
        self.assertEqual(r["state"], "mismatch"); self.assertEqual(r["actual_text"], actual)

    def test_mixed_newlines_and_extra_cr_rejected(self):
        for actual in (EXPECTED.replace("\r\n", "\n", 1), EXPECTED.replace("\r\n", "\r\r\n"), EXPECTED + "\n"):
            with self.subTest(actual=repr(actual)):
                self.assertEqual(self.call(artifacts.check_notepad_bytes, actual.encode("utf-8"), EXPECTED)["state"], "mismatch")

    def test_nfc_does_not_equal_nfd(self):
        actual = EXPECTED.replace("e\u0301", "é")
        r = self.call(artifacts.check_notepad_bytes, actual.encode("utf-8"), EXPECTED)
        self.assertEqual(r["state"], "mismatch"); self.assertEqual(r["actual_text"], actual)

    def test_missing_trailing_space_not_trimmed(self):
        actual = EXPECTED.replace("END \r\n", "END\r\n")
        self.assertEqual(self.call(artifacts.check_notepad_bytes, actual.encode("utf-8"), EXPECTED)["state"], "mismatch")

    def test_invalid_encoding_is_unknown_not_lossy_match(self):
        for raw in (b"\xff", codecs.BOM_UTF16_LE + b"\x00", codecs.BOM_UTF16_LE + b"\x00\xd8"):
            with self.subTest(raw=raw):
                r = self.call(artifacts.check_notepad_bytes, raw, EXPECTED)
                self.assertEqual(r["state"], "unknown"); self.assertIs(r["gui_verified"], False)

    def test_file_checker_must_not_universal_newline_decode(self):
        _, o = self.prepared(); item = o["crossapp-01"]; target = self.root / item["output_path"]
        target.write_bytes(item["expected_text"].encode("utf-8"))
        self.assertEqual(self.artifact_result(1)["state"], "artifact_match")
        raw = item["expected_text"].replace("\r\n", "\n").encode("utf-8"); target.write_bytes(raw)
        self.assertEqual(self.artifact_result(1)["state"], "mismatch")
        self.assertEqual(target.read_bytes(), raw)

    def test_calculator_three_distinct_observations_no_self_pass(self):
        m, _ = self.prepared(); c = self.case(m, 2)
        observations = c["observations"]
        self.assertEqual(len(observations), 3)
        self.assertEqual(len({x["trial_id"] for x in observations}), 3)
        for item in observations:
            self.assertEqual(item["expected_visible_text"], "200")
            self.assertIs(item["requires_fresh_observation"], True)
            self.assertEqual(item["status"], "needs_evidence")
        self.assertEqual(self.artifact_result(2)["state"], "needs_evidence")

    def test_explorer_64_files_and_empty_destination(self):
        m, o = self.prepared(); c = self.case(m, 3); item = o["crossapp-03"]
        source = self.root / item["source"]; destination = self.root / item["destination"]
        self.assertEqual(sorted(p.name for p in source.iterdir()), ["item-%02d.txt" % i for i in range(1, 65)])
        self.assertEqual(list(destination.iterdir()), []); self.assertEqual(item["move_file"], "item-37.txt")
        for name, digest in item["sha256_by_name"].items():
            data = (source / name).read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), digest)
            self.assertIn(c["nonce"].encode("ascii"), data)
        self.assertEqual(len(item["sha256_by_name"]), 64)

    def test_explorer_copy_not_move_is_mismatch(self):
        _, o = self.prepared(); item = o["crossapp-03"]; name = item["move_file"]
        src, dst = self.root / item["source"] / name, self.root / item["destination"] / name
        dst.write_bytes(src.read_bytes())
        self.assertEqual(self.artifact_result(3)["state"], "mismatch")
        self.assertTrue(src.exists())

    def test_explorer_final_move_is_only_artifact_match(self):
        _, o = self.prepared(); item = o["crossapp-03"]; name = item["move_file"]
        (self.root / item["source"] / name).rename(self.root / item["destination"] / name)
        r = self.artifact_result(3)
        self.assertEqual(r["state"], "artifact_match"); self.assertIs(r["drag_verified"], False)

    def test_explorer_unknown_file_is_preserved_and_reported(self):
        _, o = self.prepared(); item = o["crossapp-03"]
        unknown = self.root / item["destination"] / "unknown.txt"; unknown.write_bytes(b"unknown preserve")
        self.assertEqual(self.artifact_result(3)["state"], "mismatch")
        self.assertEqual(unknown.read_bytes(), b"unknown preserve")

    def test_explorer_modified_hash_mismatches(self):
        _, o = self.prepared(); item = o["crossapp-03"]
        (self.root / item["source"] / "item-01.txt").write_bytes(b"changed")
        self.assertEqual(self.artifact_result(3)["state"], "mismatch")

    def test_browser_offline_scope_and_future_profile_gate(self):
        m, _ = self.prepared(); c = self.case(m, 4)
        self.assertEqual(c["browser_profile_status"], "needs_supervisor_configuration")
        self.assertEqual(c["download_directory_status"], "needs_supervisor_confirmation")
        html = (self.root / c["page_path"]).read_text(encoding="utf-8")
        self.assertIn(c["nonce"], html); self.assertIn(c["case_id"], html)
        for marker in ('id="vertical-course"', 'id="horizontal-course"', 'id="entry-form"', 'id="drag-source"', 'id="drop-target"', 'id="visible-result"'):
            self.assertIn(marker, html)
        self.assertNotRegex(html, r"(?i)(?:https?://|wss?://|<script[^>]+src\s*=|<link[^>]+href\s*=|@import|fetch\s*\(|XMLHttpRequest|WebSocket)")
        self.assertEqual(self.artifact_result(4)["state"], "needs_evidence")

    def test_browser_model_self_claim_never_certifies_gui(self):
        m, o = self.prepared(); c = self.case(m, 4)
        target = self.root / o["crossapp-04"]["event_export_path"]
        target.write_bytes(json.dumps(dict(case_id=c["case_id"], nonce=c["nonce"], gui_verified=True, model_says="all passed", events=[])).encode("utf-8"))
        self.assertIn(self.artifact_result(4)["state"], ("unknown", "mismatch", "needs_evidence"))

    def test_reused_native_tasks_preserve_scope_and_source_hash(self):
        m, _ = self.prepared()
        for number, suite, path in ((5, "drag", "acceptance-fixture/tasks/drag.md"), (6, "focus", "acceptance-fixture/tasks/windows-focus.md")):
            c = self.case(m, number)
            self.assertEqual(c["reuse"]["suite"], suite)
            self.assertEqual(c["reuse"]["source_sha256"], hashlib.sha256((REPO / path).read_bytes()).hexdigest())
            self.assertIs(c["counts_as_new_external_app"], False)
            task = (self.root / c["agent_task"]).read_text(encoding="utf-8")
            self.assertIn((REPO / path).read_text(encoding="utf-8"), task)
            self.assertNotIn(path, task)  # Task is embedded; never tell GLM to open its source.
            self.assertEqual(self.artifact_result(number)["state"], "needs_evidence")

    def test_canvas_curve_requirement_not_endpoint_click(self):
        m, _ = self.prepared(); c = self.case(m, 5)
        self.assertIn("drag-10", c["reuse"]["required_case_ids"])
        self.assertEqual(c["reuse"]["curve_min_horizontal_reversals"], 2)
        self.assertIs(c["reuse"]["requires_down_motion_up"], True)

    def test_unprepared_checker_root_rejected(self):
        self.root.mkdir()
        with self.assertRaises(ValueError): self.call(artifacts.check_artifacts, str(self.root))

    def test_checker_manifest_escape_rejected_without_reading_outside(self):
        _, _ = self.prepared()
        outside = self.parent / "outside.txt"; outside.write_bytes(b"outside sentinel")
        p = self.root / "supervisor" / "oracle.json"; o = json.loads(p.read_bytes())
        o["crossapp-01"]["output_path"] = "../outside.txt"; p.write_bytes(json.dumps(o).encode("utf-8"))
        with self.assertRaises(ValueError): self.call(artifacts.check_artifacts, str(self.root))
        self.assertEqual(outside.read_bytes(), b"outside sentinel")

    def test_checker_read_only_and_no_manifest_self_certification(self):
        m, _ = self.prepared(); p = self.root / "supervisor" / "manifest.json"
        m["gui_verified"] = True; m["cases"][1]["status"] = "pass"; p.write_bytes(json.dumps(m).encode("utf-8"))
        before = {x.relative_to(self.root).as_posix(): x.read_bytes() for x in self.root.rglob("*") if x.is_file()}
        result = self.call(artifacts.check_artifacts, str(self.root))
        self.assertIs(result["gui_verified"], False); self.assertIs(result["action_trial_gate_satisfied"], False)
        self.assertNotEqual(next(c for c in result["cases"] if c["case_id"] == "crossapp-02")["state"], "artifact_match")
        after = {x.relative_to(self.root).as_posix(): x.read_bytes() for x in self.root.rglob("*") if x.is_file()}
        self.assertEqual(after, before)


    def test_existing_root_file_rejected_without_overwrite(self):
        self.root.write_bytes(b"existing file")
        with self.assertRaises(ValueError): self.call(pack.prepare_owned_pack, str(self.root))
        self.assertEqual(self.root.read_bytes(), b"existing file")

    def test_missing_parent_is_not_created_outside_owned_root(self):
        root = self.parent / "missing-parent" / "pack"
        with self.assertRaises(ValueError): self.call(paths.validate_new_root, str(root))
        self.assertFalse(root.parent.exists())

    def test_resolve_reparse_root_rejected(self):
        self.root.mkdir()
        with self.fake_entry(self.root, reparse=True):
            with self.assertRaises(ValueError): self.safe("file.txt")

    def test_browser_export_bounded_and_never_trusted_gui(self):
        _, o = self.prepared()
        p = self.root / o["crossapp-04"]["event_export_path"]
        raw = b" " * (256 * 1024 + 1); p.write_bytes(raw)
        self.assertEqual(self.artifact_result(4)["state"], "unknown")
        self.assertEqual(p.read_bytes(), raw)

    def test_notepad_oversize_is_unknown(self):
        r = self.call(artifacts.check_notepad_bytes, b"x" * (64 * 1024 + 1), EXPECTED)
        self.assertEqual(r["state"], "unknown"); self.assertIs(r["gui_verified"], False)

    def test_checker_reparse_artifact_rejected(self):
        _, o = self.prepared(); p = self.root / o["crossapp-01"]["output_path"]
        p.write_bytes(b"owned test placeholder")
        with self.fake_entry(p, reparse=True):
            with self.assertRaises(ValueError): self.call(artifacts.check_artifacts, str(self.root))


if __name__ == "__main__":
    unittest.main()
