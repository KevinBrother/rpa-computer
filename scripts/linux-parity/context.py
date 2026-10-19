"""Scenario resources and whitebox assertions, never a substitute input path."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from common import Deadline, Failure, require, spawn, stop_owned
from protocol import Computer
from rpc import Rpc
from diagnostics import HEARTBEAT_LIMIT_S, capture_rejection, process_snapshot, read_fixture_sample
from process_ledger import identity
from fixture_log import BoundedFixtureLog

HERE = Path(__file__).resolve().parent


class Context:
    def __init__(self, args, evidence, deadline):
        self.args, self.ev, self.deadline = args, evidence, deadline
        self.fixture = None
        self.fixture_log = None
        self.rpc = None
        self.computer = None
        self.state_path = evidence.directory / "fixture-live.json"
        self.before = None
        self.assertions = 0
        self.cleanups = []
        self.phase = "setup"
        self.wait_label = None
        self.last_fresh_sample = None
        self.diagnostic_serial = 0
        self.owned_identities = {"fixture": None, "host": None, "runner": None}

    def start(self):
        command = [sys.executable, str(HERE / "fixture.py"), "--state", str(self.state_path)]
        if getattr(self.args, "fixture_tracebacks", False):
            command.append("--tracebacks")
        self.fixture_log = BoundedFixtureLog(self.ev.directory / "fixture-stderr.log")
        self.fixture = spawn(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                             stderr=subprocess.PIPE, bufsize=0)
        self.fixture_log.start(self.fixture.stderr)
        self.owned_identities["fixture"] = identity(self.fixture.pid)
        self.owned_identities["runner"] = identity(os.getpid())
        self.wait(lambda s: s["ready"], "fixture_ready")
        self.before = self.snapshot("x11-before.json")
        self.require_idle(self.before)
        self.ev.json("fixture-before.json", self.state())
        self.rpc = Rpc(self.args.host, self.deadline, self.ev, self.args.request_timeout)
        self.owned_identities["host"] = identity(self.rpc.process.pid)
        self.ev.json("owned-processes-start.json", {role: process_snapshot(expected)
                                                   for role, expected in self.owned_identities.items()})
        self.phase = "host_initialize"
        self.computer = Computer(self.rpc, self.ev)
        self.computer.initialize()
        self.phase = "scenario"

    def rejected(self, sample, reason):
        self.diagnostic_serial += 1
        transport = None
        if self.rpc is not None:
            transport = {"pid": self.rpc.process.pid, "returncode": self.rpc.process.poll(),
                         "eof": self.rpc.eof, "poisoned": self.rpc.poisoned,
                         "pending": [{"id": rid, "deadline_monotonic": item[0], "method": item[1]}
                                     for rid, item in self.rpc.pending.items()]}
        try:
            capture_rejection(self.ev, sample, self.diagnostic_serial, reason,
                              self.last_fresh_sample, self.phase, self.wait_label,
                              self.owned_identities, transport)
            if self.fixture_log is not None:
                self.ev.event("fixture_stderr_at_rejection", **self.fixture_log.snapshot())
        except Exception as exc:
            # A diagnostic failure must neither erase the original rejection nor
            # become permission to retry/read a refreshed fixture as if it passed.
            try:
                self.ev.event("fixture_diagnostic_failed", reason=reason, error_type=type(exc).__name__,
                              age_s=sample.read["age_s"], revision=sample.read["revision"])
            except OSError:
                pass

    def state(self):
        require(self.fixture.poll() is None, "owned fixture exited")
        sample = read_fixture_sample(self.state_path, self.fixture.pid)
        state = sample.state
        if state.get("callback_errors"):
            self.rejected(sample, "callback_errors")
            raise Failure("fixture callback failed; inspect fixture-rejected diagnostics")
        if sample.read["age_s"] >= HEARTBEAT_LIMIT_S:
            self.rejected(sample, "heartbeat_stale")
            raise Failure("fixture heartbeat stale")
        self.last_fresh_sample = sample
        return state

    def wait(self, predicate, label, timeout=3, stable=0.1):
        previous_label = self.wait_label
        self.wait_label = label
        try:
            end = time.monotonic() + self.deadline.left(timeout)
            good_since = None
            last = None
            while time.monotonic() < end:
                self.deadline.left()
                try:
                    last = self.state()
                except FileNotFoundError:
                    # Only readiness may see a missing file. Malformed/stale state
                    # is never swallowed as an eventual success.
                    if label != "fixture_ready":
                        raise
                if last is not None and predicate(last):
                    if good_since is None:
                        good_since = time.monotonic()
                    if time.monotonic() - good_since >= stable:
                        self.assertions += 1
                        self.ev.event("assertion", label=label, state=last)
                        return last
                else:
                    good_since = None
                self.deadline.sleep(0.025)
            self.ev.event("assertion_failed", label=label, state=last)
            raise Failure(f"fixture predicate deadline: {label}")

        finally:
            self.wait_label = previous_label

    def check(self, condition, label, **detail):
        self.ev.event("check", label=label, passed=bool(condition), **detail)
        require(condition, label)
        self.assertions += 1

    def snapshot(self, name):
        proc = spawn([sys.executable, str(HERE / "x11_snapshot.py")], stdin=subprocess.DEVNULL,
                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        try:
            output, _ = proc.communicate(timeout=self.deadline.left(4))
            require(proc.returncode == 0, "read-only Xlib snapshot failed")
            require(len(output) < 1024 * 1024, "Xlib snapshot exceeds bound")
            data = json.loads(output)
            self.ev.json(name, data)
            return data
        finally:
            stop_owned(proc, grace=0.1)

    @staticmethod
    def require_idle(snapshot):
        require(not snapshot["pressed_keycodes"] and snapshot["pointer"]["buttons_mask"] == 0,
                "X server has held input")
        # Lock modifiers are environmental state, not necessarily currently held.

    def target(self, name):
        s = self.state()
        r = s["controls"][name]
        x, y = r["x"] + r["width"] // 2, r["y"] + r["height"] // 2
        require(r["width"] > 10 and r["height"] > 10, "unmapped/small target")
        require(0 <= x < s["root_size"][0] and 0 <= y < s["root_size"][1], "target outside root")
        return self.computer.point(x, y)

    def click(self, name):
        self.computer.observe()
        self.computer.step({"kind": "click", "position": self.target(name), "count": 1})
        if name in ("entry", "text"):
            self.wait(lambda s: s["focus"] == s["controls"][name]["widget"], name + "_focused")

    def text(self, text, method="native"):
        return self.computer.step({"kind": "text_input", "method": method, "text": text})

    def chord(self, key, modifiers=None):
        return self.computer.step({"kind": "key_chord", "modifiers": modifiers or ["ctrl"], "key": key})

    def held_clear(self, label):
        self.wait(lambda s: not s["pressed_keys"] and not s["pressed_buttons"], label, stable=0.25)
        self.require_idle(self.snapshot(label + "-x11.json"))

    def close_resources(self, cleanup_deadline):
        """Collect cleanup failures rather than masking the original case failure."""
        self.phase = "cleanup_close"
        self.deadline = cleanup_deadline
        if self.rpc:
            self.rpc.deadline = cleanup_deadline
        try:
            if self.computer and self.computer.sid:
                require(not self.rpc.poisoned and not self.rpc.eof,
                        "cannot confirm close on poisoned/ended transport")
                self.computer.close()
        except Exception as exc:
            self.cleanups.append({"phase": "close", "error": type(exc).__name__, "detail": str(exc)})
        try:
            if self.rpc:
                rc = self.rpc.eof_and_wait(seconds=6)
                require(rc == 0, "host nonzero EOF exit")
        except Exception as exc:
            self.cleanups.append({"phase": "eof", "error": type(exc).__name__, "detail": str(exc)})
        finally:
            if self.rpc:
                self.rpc.dispose()
        try:
            if self.fixture and self.before:
                self.phase = "cleanup_post_host"
                # Save post-host state even when idle/restoration assertions fail.
                after = self.snapshot("x11-after.json")
                self.ev.json("fixture-after.json", self.state())
                self.require_idle(after)
                self.wait(lambda s: not s["pressed_keys"] and not s["pressed_buttons"],
                          "post_host_release", stable=0.25)
                self.check(after["mapping"] == self.before["mapping"], "core_xkb_projection_restored")
                self.check(after["pointer"]["mask"] & 0xff == self.before["pointer"]["mask"] & 0xff,
                           "modifier_state_restored")
                self.ev.json("fixture-after.json", self.state())
        except Exception as exc:
            self.cleanups.append({"phase": "post_host", "error": type(exc).__name__, "detail": str(exc)})
            try:
                self.ev.json("fixture-after-failure.json", self.state())
            except Exception:
                pass
        finally:
            try:
                stop_owned(self.fixture)
            finally:
                if self.fixture_log is not None:
                    self.ev.json("fixture-stderr-summary.json", self.fixture_log.finish())
        self.ev.json("cleanup.json", {"errors": self.cleanups})
