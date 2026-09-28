#!/usr/bin/env python3
"""Black-box MCP protocol conformance harness for `computer-host` (stdio).

Pure Python 3 stdlib. Spawns the compiled host as a subprocess and drives
newline-delimited JSON-RPC over stdin/stdout, exactly as an MCP client would.
It validates protocol shape and lifecycle semantics — it does NOT validate
real screenshots or real desktop input.

SAFETY MODEL
------------
* DEFAULT mode (--mock-required): refuses to run unless the host binary was
  started with an explicit mock/test backend flag. This harness can only
  prove "the mock image flows through the protocol" — it must never be used
  to claim real screenshot capability.
* REAL GUI mode requires BOTH --allow-real-gui AND a host started without
  the mock flag. Even then, this harness only performs non-input protocol
  probes (initialize/list/describe) unless --with-input is also given.
  Interactive desktop tests are scheduled by the coordinator, not by QA.

Usage:
  python3 tests/mcp_protocol.py --host ./target/release/computer-host \
      --host-arg --mock-backend            # safe, default
  python3 tests/mcp_protocol.py --host ... --allow-real-gui   # diagnostics only
  python3 tests/mcp_protocol.py --host ... --allow-real-gui --with-input  # coordinator only
  python3 tests/mcp_protocol.py --host ... --allow-real-gui \
      --verify-native-lock                 # coordinator only: native double-host
                                           # lock check, observe-level (NO step/input)

Exit code 0 = all selected checks passed; 1 = failures; 2 = usage/safety refusal.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import queue
import struct
import subprocess
import sys
import threading
import time
from typing import Any, Dict, List, Optional, Tuple

MCP_PROTOCOL_VERSION = "2024-11-05"

FAILURES: List[str] = []
PASSES: List[str] = []
SKIPS: List[str] = []


def check(name: str, ok: bool, detail: str = "") -> bool:
    if ok:
        PASSES.append(name)
    else:
        FAILURES.append(f"{name}: {detail}")
    return ok


def skip(name: str, reason: str) -> None:
    """Record an explicit, honest SKIP. A scenario that cannot prove its
    property under the current backend is reported as unverified — it is
    NEVER silently dropped and NEVER turned into a fake pass."""
    SKIPS.append(f"{name}: {reason}")
    print(f"SKIP {name}: {reason}")


class StdoutReader(threading.Thread):
    """Reads stdout lines without blocking the main thread, so we can
    implement timeouts and detect protocol pollution."""

    def __init__(self, stream):
        super().__init__(daemon=True)
        self.stream = stream
        self.lines: "queue.Queue[bytes]" = queue.Queue()

    def run(self) -> None:
        for raw in self.stream:
            self.lines.put(raw)

    def read_line(self, timeout: float) -> Optional[bytes]:
        try:
            return self.lines.get(timeout=timeout)
        except queue.Empty:
            return None


class StartupFailure(RuntimeError):
    """The host process did not complete MCP initialize at startup.

    Carries the process exit code (None = still alive but unresponsive)
    and the captured stderr tail so reports are diagnostic, not a bare
    stack trace."""


class Host:
    def __init__(self, argv: List[str], env: Optional[dict] = None):
        self.argv = argv
        self.proc = subprocess.Popen(
            argv,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )
        assert self.proc.stdout is not None
        self.reader = StdoutReader(self.proc.stdout)
        self.reader.start()
        self.stderr_tail: List[bytes] = []
        self._stderr_thread = threading.Thread(target=self._drain_stderr, daemon=True)
        self._stderr_thread.start()
        self._next_id = 1

    def _drain_stderr(self) -> None:
        assert self.proc.stderr is not None
        for raw in self.proc.stderr:
            self.stderr_tail.append(raw)
            if len(self.stderr_tail) > 200:
                self.stderr_tail.pop(0)

    def send(self, obj: Dict[str, Any]) -> None:
        assert self.proc.stdin is not None
        line = json.dumps(obj).encode("utf-8") + b"\n"
        self.proc.stdin.write(line)
        self.proc.stdin.flush()

    def send_raw(self, data: bytes) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(data)
        self.proc.stdin.flush()

    def request(self, method: str, params: Optional[dict] = None, timeout: float = 10.0) -> Dict[str, Any]:
        rid = self._next_id
        self._next_id += 1
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}})
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            raw = self.reader.read_line(timeout=max(0.05, deadline - time.monotonic()))
            if raw is None:
                break
            msg = self._parse_line(raw)
            if msg is None:
                continue
            # Skip notifications; match our id.
            if msg.get("id") == rid:
                return msg
        raise TimeoutError(f"no response for {method} (id={rid}) within {timeout}s")

    def notify(self, method: str, params: Optional[dict] = None) -> None:
        self.send({"jsonrpc": "2.0", "method": method, "params": params or {}})

    def _parse_line(self, raw: bytes) -> Optional[Dict[str, Any]]:
        stripped = raw.strip()
        if not stripped:
            return None
        try:
            msg = json.loads(stripped.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            check(
                "stdout carries only JSON-RPC frames",
                False,
                f"non-JSON line on stdout: {stripped[:200]!r}",
            )
            return None
        if not isinstance(msg, dict):
            check("stdout frames are JSON objects", False, f"got {type(msg)}")
            return None
        return msg

    def close_stdin(self) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.close()

    def wait(self, timeout: float) -> Optional[int]:
        try:
            return self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            return None

    def kill(self) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass

    def stderr_text(self) -> str:
        return b"".join(self.stderr_tail).decode("utf-8", "replace")


def initialize(h: Host) -> Dict[str, Any]:
    try:
        resp = h.request(
            "initialize",
            {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "qa-mcp-harness", "version": "0.1.0"},
            },
        )
    except TimeoutError as e:
        # Surface the host's own startup diagnostics: exit code (None =
        # alive but unresponsive) and the stderr tail. A bare TimeoutError
        # stack trace is not actionable evidence.
        rc = h.proc.poll()
        raise StartupFailure(
            f"host did not answer initialize within bound: {e}; "
            f"process rc={rc} (None=still alive); "
            f"stderr tail: {h.stderr_text()[-800:]}"
        ) from e
    return resp


def expect_result(name: str, resp: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    if "error" in resp:
        check(name, False, f"unexpected JSON-RPC error: {resp['error']}")
        return None
    result = resp.get("result")
    if not isinstance(result, dict):
        check(name, False, f"missing result object: {resp}")
        return None
    check(name, True)
    return result


def expect_error(name: str, resp: Dict[str, Any], codes: Optional[List[int]] = None) -> Optional[Dict[str, Any]]:
    err = resp.get("error")
    if not isinstance(err, dict):
        check(name, False, f"expected JSON-RPC error, got: {resp}")
        return None
    if "code" not in err or "message" not in err:
        check(name, False, f"error lacks code/message: {err}")
        return None
    if codes is not None and err["code"] not in codes:
        check(name, False, f"error code {err['code']} not in {codes}")
        return None
    check(name, True)
    return err


def tool_names(tools: List[Dict[str, Any]]) -> List[str]:
    return [t.get("name", "") for t in tools]


EXPECTED_TOOLS = {
    "computer_describe",
    "computer_open",
    "computer_observe",
    "computer_step",
    "computer_get_step",
    "computer_pause",
    "computer_resume",
    "computer_close",
}


def png_dimensions(data: bytes) -> Tuple[int, int]:
    """Return (width, height) from PNG IHDR; raises on invalid PNG."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    if data[12:16] != b"IHDR":
        raise ValueError("missing IHDR")
    width, height = struct.unpack(">II", data[16:24])
    return width, height


def find_image_block(result: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    for block in result.get("content", []):
        if isinstance(block, dict) and block.get("type") == "image":
            return block
    return None


def find_text_json(result: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    for block in result.get("content", []):
        if isinstance(block, dict) and block.get("type") == "text":
            try:
                return json.loads(block.get("text", ""))
            except json.JSONDecodeError:
                continue
    return None


def call_tool(h: Host, name: str, args: dict, timeout: float = 20.0) -> Dict[str, Any]:
    resp = h.request("tools/call", {"name": name, "arguments": args}, timeout=timeout)
    result = expect_result(f"tools/call {name} returns a result envelope", resp)
    if result is None:
        raise RuntimeError(f"tools/call {name} had no result: {resp}")
    return result


def scenario_lifecycle(h: Host, with_input: bool) -> None:
    resp = initialize(h)
    init = expect_result("initialize succeeds", resp)
    if init is None:
        return
    check(
        "initialize returns protocolVersion",
        isinstance(init.get("protocolVersion"), str),
        f"got {init.get('protocolVersion')!r}",
    )
    caps = init.get("capabilities", {})
    check("initialize advertises tools capability", "tools" in caps, f"caps={caps}")

    h.notify("notifications/initialized")

    ping = h.request("ping")
    expect_result("ping succeeds", ping)

    listed = expect_result("tools/list succeeds", h.request("tools/list"))
    if listed is None:
        return
    tools = listed.get("tools", [])
    names = set(tool_names(tools))
    check(
        "exactly the contract tools are exposed",
        names == EXPECTED_TOOLS,
        f"missing={EXPECTED_TOOLS - names} extra={names - EXPECTED_TOOLS}",
    )
    for t in tools:
        check(
            f"tool {t.get('name')} has inputSchema object",
            isinstance(t.get("inputSchema"), dict),
            f"schema={t.get('inputSchema')!r}",
        )

    # --- describe (diagnostics, no input) ---
    desc = call_tool(h, "computer_describe", {})
    check(
        "computer_describe is not a tool-level error",
        not desc.get("isError", False),
        f"{desc}",
    )

    # --- open/observe round trip (captures screen; no input) ---
    opened = call_tool(h, "computer_open", {"max_width": 960, "max_height": 540})
    if check("computer_open succeeds", not opened.get("isError", False), f"{opened}"):
        meta = find_text_json(opened) or {}
        session_id = meta.get("session_id")
        check("open returns session_id", isinstance(session_id, str) and bool(session_id), f"{meta}")

        obs = call_tool(h, "computer_observe", {"session_id": session_id})
        if check("computer_observe succeeds", not obs.get("isError", False), f"{obs}"):
            img = find_image_block(obs)
            check("observe returns an image content block", img is not None, f"{obs.get('content')}")
            om = find_text_json(obs) or {}
            if img is not None:
                check(
                    "image block mimeType is image/png",
                    img.get("mimeType") == "image/png",
                    f"{img.get('mimeType')!r}",
                )
                raw = b""
                try:
                    raw = base64.b64decode(img.get("data", ""), validate=True)
                    check("image data is valid base64", True)
                except Exception as e:  # noqa: BLE001
                    check("image data is valid base64", False, str(e))
                if raw:
                    try:
                        w, hh = png_dimensions(raw)
                        check(
                            "metadata width_px/height_px match actual PNG",
                            (om.get("width_px"), om.get("height_px")) == (w, hh),
                            f"meta=({om.get('width_px')},{om.get('height_px')}) png=({w},{hh})",
                        )
                        check(
                            "observation dimensions are positive",
                            w > 0 and hh > 0,
                            f"{w}x{hh}",
                        )
                    except ValueError as e:
                        check("image decodes as PNG with IHDR", False, str(e))
            for field in ("observation_id", "session_id", "surface_id",
                          "geometry_version", "input_sequence"):
                check(f"observation metadata has {field}", field in om, f"{om}")

            if with_input:
                scenario_step(h, session_id, om)

            # get_step on unknown request must be an error, not a fabrication
            g = call_tool(h, "computer_get_step",
                          {"session_id": session_id, "request_id": "qa-no-such-request"})
            check(
                "get_step of unknown request_id is an error",
                bool(g.get("isError", False)),
                f"{g}",
            )

            closed = call_tool(h, "computer_close", {"session_id": session_id})
            check("computer_close succeeds", not closed.get("isError", False), f"{closed}")
            # Idempotent close: either success with an explicit terminal
            # state, or a structured session-scoped error — never a fresh
            # claim of "released" it did not perform, never a crash.
            closed2 = call_tool(h, "computer_close", {"session_id": session_id})
            c2meta = find_text_json(closed2) or {}
            if closed2.get("isError", False):
                blob = json.dumps(c2meta).lower()
                check(
                    "second close error is session-scoped",
                    ("session" in blob) or ("closed" in blob) or ("not_found" in blob),
                    f"{closed2}",
                )
            else:
                state = str(c2meta.get("state", ""))
                check(
                    "second close reports explicit terminal state",
                    state in ("closed", "faulted"),
                    f"state={state!r} in {closed2}",
                )


def scenario_step(h: Host, session_id: str, obs_meta: Dict[str, Any]) -> None:
    obs_id = obs_meta.get("observation_id")
    # A harmless, fully reversible input probe: move the pointer by 0px-ish
    # (move to current-ish position is still real input; coordinator-gated).
    step = call_tool(h, "computer_step", {
        "session_id": session_id,
        "request_id": "qa-protocol-step-1",
        "based_on": obs_id,
        "action": {"kind": "move", "position": [1, 1]},
    })
    check("probe step is not a tool-level crash", isinstance(step.get("isError", False), bool), f"{step}")
    meta = find_text_json(step) or {}
    for field in ("request_id", "input_outcome", "observation_outcome", "cleanup_outcome"):
        check(f"step result has {field}", field in meta, f"{meta}")
    # Dedup: replay identical request must not re-dispatch. We cannot observe
    # dispatch counts over MCP, but the replayed reply must carry the same
    # recorded outcome.
    replay = call_tool(h, "computer_step", {
        "session_id": session_id,
        "request_id": "qa-protocol-step-1",
        "based_on": obs_id,
        "action": {"kind": "move", "position": [1, 1]},
    })
    rmeta = find_text_json(replay) or {}
    check(
        "replayed request_id returns consistent outcome",
        rmeta.get("input_outcome") == meta.get("input_outcome"),
        f"first={meta.get('input_outcome')} replay={rmeta.get('input_outcome')}",
    )
    # Same id, different payload must conflict.
    conflict = call_tool(h, "computer_step", {
        "session_id": session_id,
        "request_id": "qa-protocol-step-1",
        "based_on": obs_id,
        "action": {"kind": "move", "position": [2, 2]},
    })
    check(
        "same request_id with different payload is rejected",
        bool(conflict.get("isError", False)),
        f"{conflict}",
    )
    # Pause must block further input.
    call_tool(h, "computer_pause", {"session_id": session_id})
    paused = call_tool(h, "computer_step", {
        "session_id": session_id,
        "request_id": "qa-protocol-step-2",
        "based_on": obs_id,
        "action": {"kind": "move", "position": [3, 3]},
    })
    check("step while paused is rejected", bool(paused.get("isError", False)), f"{paused}")
    call_tool(h, "computer_resume", {"session_id": session_id})


def scenario_protocol_errors(h: Host) -> None:
    # Unknown tool
    resp = h.request("tools/call", {"name": "computer_nuke", "arguments": {}})
    if "error" in resp:
        check("unknown tool yields JSON-RPC error OR tool error", True)
    else:
        result = resp.get("result", {})
        check(
            "unknown tool yields isError tool result",
            bool(result.get("isError", False)),
            f"{resp}",
        )

    # Unknown method
    resp = h.request("qa/no_such_method")
    expect_error("unknown method -> -32601", resp, codes=[-32601])

    # Malformed JSON line -> parse error (-32700)
    h.send_raw(b"{not json\n")
    raw = h.reader.read_line(timeout=5.0)
    if raw is None:
        check("malformed JSON gets -32700 response", False, "no response")
    else:
        msg = h._parse_line(raw)
        if msg is None:
            check("malformed JSON gets -32700 response", False, "unparseable reply")
        else:
            err = msg.get("error", {})
            check(
                "malformed JSON gets -32700 response",
                err.get("code") == -32700,
                f"{msg}",
            )

    # tools/call with wrong arg types must be a structured error, not a crash.
    resp = h.request("tools/call", {"name": "computer_observe", "arguments": {"session_id": 123}})
    result = resp.get("result", {})
    check(
        "invalid arguments rejected without crash",
        "error" in resp or bool(result.get("isError", False)),
        f"{resp}",
    )

    # After all that, the host must still answer a ping (no wedged reader).
    try:
        ping = h.request("ping", timeout=5.0)
        check("host still responsive after protocol errors", "result" in ping, f"{ping}")
    except TimeoutError as e:
        check("host still responsive after protocol errors", False, str(e))


def scenario_eof_cleanup(argv: List[str], env: Optional[dict]) -> None:
    """Fresh host: open a session, then close stdin (EOF). The host must
    shut down cleanly (contract: EOF triggers cleanup + shutdown)."""
    h = Host(argv, env=env)
    try:
        initialize(h)
        h.notify("notifications/initialized")
        opened = call_tool(h, "computer_open", {})
        session_ok = not opened.get("isError", False)
        check("EOF scenario: session opened", session_ok, f"{opened}")
        h.close_stdin()
        rc = h.wait(timeout=15.0)
        if rc is None:
            check("host exits after stdin EOF", False, "still running after 15s")
            h.kill()
        else:
            check("host exits after stdin EOF", True, f"rc={rc}")
            check(
                "host exit code after EOF is 0",
                rc == 0,
                f"rc={rc}; stderr tail: {h.stderr_text()[-500:]}",
            )
    finally:
        h.kill()


def scenario_cancel_notification(argv: List[str], env: Optional[dict],
                                 selftest: bool = False) -> None:
    """Cancellation notification during a long wait must be honored: the
    pending tools/call must complete (cancelled) rather than hang forever.

    selftest mode (isolated fake fixture): the fixture answers immediately,
    so only the in-bound completion + post-cancel responsiveness are
    asserted; the cancelled-outcome assertion applies to real hosts."""
    h = Host(argv, env=env)
    try:
        initialize(h)
        h.notify("notifications/initialized")
        opened = call_tool(h, "computer_open", {})
        if opened.get("isError", False):
            check("cancel scenario: session opened", False, f"{opened}")
            return
        meta = find_text_json(opened) or {}
        session_id = meta.get("session_id")

        rid = h._next_id
        h._next_id += 1
        # Long observe wait; then cancel it. The fake fixture answers
        # immediately; a real host honors the cancellation notification and
        # completes early with a cancelled outcome. Both complete in bound.
        h.send({"jsonrpc": "2.0", "id": rid, "method": "tools/call", "params": {
            "name": "computer_observe",
            "arguments": {"session_id": session_id, "wait_ms": 3000},
        }})
        h.notify("notifications/cancelled", {"requestId": rid, "reason": "qa-cancel-probe"})
        raw = h.reader.read_line(timeout=10.0)
        if raw is None:
            check("cancelled request completes within bound", False, "hung >10s")
        else:
            msg = h._parse_line(raw)
            if msg is None or msg.get("id") != rid:
                check("cancelled request completes within bound", False, f"unexpected {msg}")
            else:
                # Either a JSON-RPC error or a tool result with cancelled/
                # isError is acceptable; what matters is it did not hang and
                # did not report success-after-cancel.
                if "error" in msg:
                    check("cancelled request completes within bound", True)
                else:
                    result = msg.get("result", {})
                    rmeta = find_text_json(result) or {}
                    outcome = json.dumps(rmeta)
                    if selftest:
                        # Fixture answers before the notification arrives; a
                        # clean success is expected and proves nothing about
                        # cancellation (asserted against real hosts only).
                        check("cancelled request completes within bound (fixture immediate reply)", True)
                    else:
                        check(
                            "cancelled request is not reported as clean success",
                            bool(result.get("isError", False)) or "cancel" in outcome.lower(),
                            f"{msg}",
                        )
        # Host still responsive afterwards.
        ping = h.request("ping", timeout=5.0)
        check("host responsive after cancellation", "result" in ping, f"{ping}")
        call_tool(h, "computer_close", {"session_id": session_id})
    finally:
        h.kill()


def scenario_second_host_exclusion(argv: List[str], env: Optional[dict],
                                   allow_process_refusal: bool = False) -> None:
    """A second independent host process on the same desktop must fail with
    lease_conflict (cross-process exclusion), not silently take over.

    NOTE: this asserts the NATIVE desktop lock. A mock backend does not
    take the real lease by design, so callers must skip this scenario
    under mock (see main) rather than let it fake-pass here.

    allow_process_refusal (native-lock verification mode only): a second
    host that exits NONZERO at startup with an accurate lease_conflict on
    stderr is a valid refusal — the harness must not force it to stay up
    and answer initialize with a tool error. A generic timeout is NEVER
    accepted as proof of the lock; if the second process is alive but
    unresponsive and no accurate refusal is seen, that is an environment
    failure, not a pass."""
    h1 = Host(argv, env=env)
    h2: Optional[Host] = None
    try:
        try:
            initialize(h1)
        except StartupFailure as e:
            # Environment-level failure (e.g. locked/sleeping display,
            # host cannot bind): report the host's own diagnostics, never
            # a stack trace, never a fake pass.
            check("exclusion scenario: first host started", False, f"{e}")
            return
        h1.notify("notifications/initialized")
        opened = call_tool(h1, "computer_open", {})
        if opened.get("isError", False):
            check("exclusion scenario: first session opened", False, f"{opened}")
            return
        h2 = Host(argv, env=env)
        try:
            initialize(h2)
            h2.notify("notifications/initialized")
            opened2 = call_tool(h2, "computer_open", {})
            second = find_text_json(opened2) or {}
            err_blob = json.dumps(opened2)
            check(
                "second host open is refused (lease_conflict)",
                bool(opened2.get("isError", False))
                and ("lease" in err_blob or "conflict" in err_blob or second.get("state") == "closed"),
                f"{opened2}",
            )
        except StartupFailure as e:
            if allow_process_refusal:
                _second_host_startup_assessment(h2, e)
            else:
                check("second host open is refused (lease_conflict)", False,
                      f"second host startup: {e}")
        finally:
            h2.kill()
        meta = find_text_json(opened) or {}
        call_tool(h1, "computer_close", {"session_id": meta.get("session_id")})
    finally:
        h1.kill()
        if h2 is not None:
            h2.kill()


def _second_host_startup_assessment(h2: Host, startup_err: StartupFailure) -> None:
    """Classify a non-answering second host in native-lock verification mode.

    Only an ACCURATE refusal counts as a pass:
      * process exited nonzero AND stderr mentions lease/conflict, or
      * process exited zero AND stderr mentions lease/conflict (clean refuse).
    A wedged-but-alive process, or a dead process with no lease_conflict
    evidence, is an environment failure — never a lock pass. A generic
    timeout alone is NOT proof of the lock.
    """
    time.sleep(0.3)  # let a just-exited process settle
    rc = h2.proc.poll()
    stderr_tail = h2.stderr_text()[-2000:]
    blob = stderr_tail.lower()
    accurate = ("lease" in blob) or ("conflict" in blob)
    if rc is not None and rc != 0 and accurate:
        check("second host open is refused (lease_conflict)", True,
              f"second host exited rc={rc} with accurate lease_conflict stderr")
    elif rc == 0 and accurate:
        check("second host open is refused (lease_conflict)", True,
              "second host exited 0 with accurate lease_conflict stderr")
    elif rc is not None:
        check("second host open is refused (lease_conflict)", False,
              f"second host exited rc={rc} WITHOUT accurate lease_conflict stderr "
              f"(environment/host failure, not a lock pass): {startup_err}")
    else:
        # The second process is alive but did not answer initialize. The
        # lock MIGHT be held (host waiting on a locked console) — but a
        # generic timeout is NOT proof. Report environment-blocked, never
        # a lock pass.
        check("second host open is refused (lease_conflict)", False,
              f"second host is ALIVE but unresponsive — a generic timeout is not "
              f"proof of the lock (locked/sleeping display, wedged reader, or a "
              f"host that does not implement the native lease): {startup_err}")


def verify_native_lock(host_argv: List[str], env: Optional[dict]) -> int:
    """Coordinator-gated NATIVE double-host lock verification.

    Runs ONLY the cross-process exclusion scenario against a real-desktop
    host. It performs initialize + open/observe-level protocol calls and
    computer_close — it NEVER sends computer_step or any input. This is
    the only sanctioned way to prove native exclusivity; the mock suite
    explicitly skips that scenario instead of fake-passing it.

    Environment preconditions (checked honestly, never assumed):
      * the desktop must NOT be locked / at a credential prompt — a host
        that cannot capture because the console is locked is an
        ENVIRONMENT failure, not a lock pass;
      * the host must actually start and answer initialize.

    Usage (coordinator only, on a machine whose desktop may be observed):
      python3 tests/mcp_protocol.py --host /path/to/computer-host \
          --allow-real-gui --verify-native-lock
    """
    print("native-lock verification: REAL desktop, observe-level only (no step/input)")
    scenario_second_host_exclusion(host_argv, env, allow_process_refusal=True)
    print(f"\n=== native-lock verification: {len(PASSES)} passed, "
          f"{len(FAILURES)} failed, {len(SKIPS)} skipped ===")
    for f in FAILURES:
        print(f"FAIL {f}")
    return 1 if FAILURES else 0


def main(argv: List[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--host", required=True, help="path to computer-host binary")
    ap.add_argument("--host-arg", action="append", default=[],
                    help="extra argument passed to the host (repeatable), e.g. --host-arg --mock-backend")
    ap.add_argument("--mock-required", action="store_true", default=True,
                    help="(default) refuse to run unless a mock backend flag is present")
    ap.add_argument("--allow-real-gui", action="store_true",
                    help="permit running against the REAL desktop backend (diagnostics only unless --with-input)")
    ap.add_argument("--with-input", action="store_true",
                    help="include a real input probe step (coordinator-scheduled runs only)")
    ap.add_argument("--selftest", action="store_true",
                    help="run against an isolated fake-host fixture: skip scenarios that require "
                         "product-only behavior (cross-process lease) and require zero failures")
    ap.add_argument("--verify-native-lock", action="store_true",
                    help="coordinator-only: run ONLY the native double-host lock check against a "
                         "real desktop host (requires --allow-real-gui; observe-level, no step/input)")
    args = ap.parse_args(argv)

    host_argv = [args.host] + list(args.host_arg)
    # A mock/test backend is indicated by an explicit host flag. The flag
    # values accepted here must match what the transport worker implements;
    # until then the harness treats the mock as "claimed" and the report
    # records the mismatch instead of silently assuming safety.
    mock_flags = {"--mock-backend", "--test-backend", "--backend=mock"}
    using_mock = any(a in mock_flags or a.startswith("--backend=mock") for a in args.host_arg)
    if os.environ.get("QA_HARNESS_SELFTEST") == "1":
        using_mock = True  # self-test against the fake python host only
    if args.selftest:
        using_mock = True

    if not using_mock and not args.allow_real_gui:
        print(
            "REFUSAL: host appears to use the REAL desktop backend. Re-run with an\n"
            "explicit mock flag (--host-arg --mock-backend) for safe protocol tests,\n"
            "or pass --allow-real-gui (diagnostics only) under coordinator control.",
            file=sys.stderr,
        )
        return 2
    if using_mock and args.allow_real_gui:
        print("note: mock backend in use; --allow-real-gui ignored", file=sys.stderr)
    if args.with_input and not args.allow_real_gui:
        print("REFUSAL: --with-input requires --allow-real-gui (real desktop).", file=sys.stderr)
        return 2

    if not os.path.exists(args.host):
        print(f"host binary not found: {args.host}", file=sys.stderr)
        return 2

    env = os.environ.copy()

    if args.verify_native_lock:
        # Native lock verification is a SEPARATE, coordinator-gated mode:
        # real desktop backend required, observe-level protocol only.
        if using_mock:
            print("REFUSAL: --verify-native-lock requires the REAL desktop backend "
                  "(no mock flag); the mock does not hold the native lock.",
                  file=sys.stderr)
            return 2
        if not args.allow_real_gui:
            print("REFUSAL: --verify-native-lock requires --allow-real-gui under coordinator control.",
                  file=sys.stderr)
            return 2
        if args.with_input:
            print("REFUSAL: --verify-native-lock is observe-level only; --with-input is forbidden here.",
                  file=sys.stderr)
            return 2
        return verify_native_lock(host_argv, env)

    h = Host(host_argv, env=env)
    try:
        scenario_lifecycle(h, with_input=args.with_input)
        scenario_protocol_errors(h)
    finally:
        h.kill()

    scenario_eof_cleanup(host_argv, env)
    scenario_cancel_notification(host_argv, env, selftest=args.selftest)
    if args.selftest:
        # The cross-process lease is product behavior; an isolated fake-host
        # fixture has no lease, so this scenario cannot pass in selftest and
        # is skipped explicitly (it still runs in normal mode against hosts
        # that implement the lease).
        skip("second host open is refused (lease_conflict)",
             "isolated fake-host fixture has no cross-process lease; native "
             "exclusivity is UNVERIFIED in selftest")
    elif using_mock:
        # The mock backend deliberately does NOT take the real desktop lock,
        # so it cannot prove native cross-process exclusivity — running the
        # scenario would produce a FAKE pass. Skip explicitly; the property
        # is verified separately by the coordinator via --verify-native-lock.
        skip("second host open is refused (lease_conflict)",
             "mock backend holds no native desktop lock by design; native exclusivity "
             "is UNVERIFIED here — coordinator verifies it via "
             "--allow-real-gui --verify-native-lock")
    else:
        scenario_second_host_exclusion(host_argv, env)

    print(f"\n=== MCP protocol harness: {len(PASSES)} passed, "
          f"{len(FAILURES)} failed, {len(SKIPS)} skipped ===")
    for f in FAILURES:
        print(f"FAIL {f}")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
