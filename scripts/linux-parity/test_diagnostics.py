"""CC-only regression tests for stale-fixture evidence; no Tk/X11/process launch.

Written before the diagnostic fix. Independent CC must execute these; the author
only reads/compiles code. Synthetic files test the harness, not product parity.
"""
import hashlib
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from common import Deadline, Evidence, Failure
from context import Context
from diagnostics import process_snapshot, save_shutdown
from fixture_log import BoundedFixtureLog


class StaleEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.ev = Evidence(Path(self.temp.name) / "case")
        self.ctx = Context(SimpleNamespace(), self.ev, Deadline(10))
        self.ctx.fixture = SimpleNamespace(pid=654321, poll=lambda: None)
        self.value = {"schema": "linux-parity-fixture-v1", "pid": 654321,
                      "ready": True, "revision": 84, "monotonic": 10.0, "seq": 2,
                      "clicks": 1, "entry": "", "pressed_keys": {}, "pressed_buttons": [],
                      "callback_errors": []}

    def tearDown(self):
        self.temp.cleanup()

    def publish(self, **updates):
        self.value.update(updates)
        raw = (json.dumps(self.value, indent=3) + "\n").encode()
        self.ctx.state_path.write_bytes(raw)
        return raw

    def reports(self):
        return sorted(self.ev.directory.glob("fixture-rejected-*.json"))

    def test_stale_saves_exact_rejected_bytes_age_and_revision_and_still_fails(self):
        original = self.publish()
        with patch("context.time.monotonic", return_value=11.25):
            with self.assertRaisesRegex(Failure, "fixture heartbeat stale"):
                self.ctx.state()
        paths = self.reports()
        self.assertEqual(len(paths), 1)
        report = json.loads(paths[0].read_text())
        self.assertEqual(report["reason"], "heartbeat_stale")
        self.assertEqual(report["read"]["age_s"], 1.25)
        self.assertEqual(report["read"]["revision"], 84)
        self.assertEqual(report["read"]["threshold_s"], 1.0)
        self.assertEqual(report["raw"]["sha256"], hashlib.sha256(original).hexdigest())
        self.assertEqual((self.ev.directory / report["raw"]["file"]).read_bytes(), original)
        self.assertEqual(self.ctx.state_path.read_bytes(), original)
        self.assertEqual(report["acceptance"], "REJECTED; diagnostics never satisfy fixture assertions")

    def test_threshold_is_not_relaxed(self):
        self.publish()
        with patch("context.time.monotonic", return_value=10.999):
            self.assertEqual(self.ctx.state()["revision"], 84)
        with patch("context.time.monotonic", return_value=11.0):
            with self.assertRaisesRegex(Failure, "fixture heartbeat stale"):
                self.ctx.state()

    def test_last_fresh_read_and_later_cleanup_rejection_remain_distinct(self):
        raw = self.publish()
        with patch("context.time.monotonic", return_value=10.25):
            self.ctx.state()
        with patch("context.time.monotonic", return_value=11.25):
            with self.assertRaises(Failure):
                self.ctx.state()
        first = self.reports()[0].read_bytes()
        self.publish(revision=85, ready=False, monotonic=10.5)
        self.ctx.phase = "cleanup_post_host"
        with patch("context.time.monotonic", return_value=12.0):
            with self.assertRaises(Failure):
                self.ctx.state()
        reports = [json.loads(p.read_text()) for p in self.reports()]
        self.assertEqual(len(reports), 2)
        self.assertEqual(self.reports()[0].read_bytes(), first)
        self.assertEqual(reports[0]["last_fresh_read"]["revision"], 84)
        self.assertEqual(reports[0]["last_fresh_read"]["read_finished_monotonic"], 10.25)
        self.assertEqual(reports[1]["phase"], "cleanup_post_host")
        self.assertEqual((self.ev.directory / reports[0]["raw"]["file"]).read_bytes(), raw)

    def test_wait_does_not_retry_or_accept_a_stale_matching_predicate(self):
        self.publish()
        with patch("context.time.monotonic", return_value=12.0), patch.object(self.ctx.deadline, "sleep") as sleep:
            with self.assertRaisesRegex(Failure, "fixture heartbeat stale"):
                self.ctx.wait(lambda state: state["clicks"] == 1, "clicked")
        sleep.assert_not_called()
        self.assertEqual(self.ctx.assertions, 0)
        self.assertEqual(json.loads(self.reports()[0].read_text())["wait_label"], "clicked")

    def test_diagnostics_error_does_not_mask_staleness_or_make_it_pass(self):
        self.publish()
        with patch("context.time.monotonic", return_value=12.0), \
                patch("context.capture_rejection", side_effect=OSError("owned disk unavailable")):
            with self.assertRaisesRegex(Failure, "fixture heartbeat stale"):
                self.ctx.state()
        self.assertEqual(self.ctx.assertions, 0)
        self.assertIn("fixture_diagnostic_failed", (self.ev.directory / "events.jsonl").read_text())

    def test_capture_callback_errors_before_raising_too(self):
        self.publish(callback_errors=[{"type": "RuntimeError", "message": "owned callback"}])
        with patch("context.time.monotonic", return_value=10.5):
            with self.assertRaisesRegex(Failure, "fixture callback failed"):
                self.ctx.state()
        self.assertEqual(json.loads(self.reports()[0].read_text())["reason"], "callback_errors")

    def test_shutdown_has_separate_artifact_and_does_not_refresh_live_state(self):
        original = self.publish()
        shutdown = {**self.value, "ready": False, "revision": 85, "monotonic": 15.0}
        save_shutdown(self.ctx.state_path, shutdown)
        self.assertEqual(self.ctx.state_path.read_bytes(), original)
        self.assertEqual(json.loads((self.ev.directory / "fixture-shutdown.json").read_text()), shutdown)


class OwnedProcTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.proc = Path(self.temp.name)
        self.expected = {"pid": 42, "pgrp": 42, "start_ticks": 999}
        self.dir = self.proc / "42"
        self.dir.mkdir()
        # /proc stat fields #3 onwards; comm deliberately includes ')' and spaces.
        fields = ["S", "1", "42"] + ["0"] * 16 + ["999", "1000", "2"]
        (self.dir / "stat").write_text("42 (owned ) fixture) " + " ".join(fields))
        (self.dir / "status").write_text("State:\tS (sleeping)\nThreads:\t1\nSecret:\tDO_NOT_COPY\n")
        (self.dir / "wchan").write_text("do_poll.constprop.0\n")
        (self.dir / "schedstat").write_text("1000 2000 3\n")
        (self.dir / "io").write_text("rchar: 12\nwchar: 34\n")
        (self.dir / "environ").write_text("SECRET_ENV=NEVER_READ")
        (self.dir / "cmdline").write_text("SECRET_ARG=NEVER_READ")
        (self.dir / "task").mkdir()

    def tearDown(self):
        self.temp.cleanup()

    def test_only_identity_matched_owned_process_files_are_reported(self):
        result = process_snapshot(self.expected, proc_root=self.proc)
        self.assertEqual(result["status"], "collected")
        self.assertEqual(result["stat"]["start_ticks"], 999)
        self.assertEqual(result["files"]["wchan"]["text"].strip(), "do_poll.constprop.0")
        serialized = json.dumps(result)
        for secret in ("SECRET_ENV", "SECRET_ARG", "DO_NOT_COPY"):
            self.assertNotIn(secret, serialized)

    def test_unowned_or_recycled_pid_is_never_inspected(self):
        self.assertEqual(process_snapshot(None, proc_root=self.proc)["status"], "not_authorized")
        wrong = {**self.expected, "start_ticks": 1}
        result = process_snapshot(wrong, proc_root=self.proc)
        self.assertEqual(result["status"], "identity_mismatch")
        self.assertNotIn("files", result)

    def test_exited_or_permission_denied_process_is_diagnostic_not_evidence_of_idle(self):
        missing = process_snapshot({**self.expected, "pid": 43}, proc_root=self.proc)
        self.assertEqual(missing["status"], "unavailable")
        with patch("diagnostics.read_bounded", side_effect=PermissionError("denied")):
            denied = process_snapshot(self.expected, proc_root=self.proc)
        self.assertEqual(denied["status"], "unavailable")
        self.assertEqual(denied["error_type"], "PermissionError")


class FixtureLogTests(unittest.TestCase):
    def test_owned_stderr_is_bounded_and_excess_is_drained(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture-stderr.log"
            log = BoundedFixtureLog(path, limit=16)
            log.drain(io.BytesIO(b"owned Tcl diagnostic" * 4))
            self.assertEqual(path.read_bytes(), (b"owned Tcl diagnostic" * 4)[:16])
            meta = log.snapshot()
            self.assertEqual(meta["bytes_received"], 80)
            self.assertEqual(meta["bytes_saved"], 16)
            self.assertEqual(meta["bytes_dropped"], 64)
            self.assertTrue(meta["complete"])

    def test_log_never_overwrites_previous_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture-stderr.log"
            path.write_bytes(b"prior diagnostic")
            with self.assertRaises(FileExistsError):
                BoundedFixtureLog(path)
            self.assertEqual(path.read_bytes(), b"prior diagnostic")


if __name__ == "__main__":
    unittest.main()
