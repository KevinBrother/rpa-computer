#!/usr/bin/env python3
"""Independent transport-ingress regression harness for `computer-host`.

Protocol-only, Python 3 stdlib, standalone. Drives the REAL compiled host
binary (explicit --mock-backend is MANDATORY — this harness has no option to
run a native backend and asserts nothing about real screens or input).

Scope (per task qa-transport-ingress-1):
  T1  observe(wait_ms=3000) + matching notifications/cancelled in ONE write,
      no sleeps — 3 trials with fresh host+session; escaped string id must
      work; unknown id must not cancel a valid request.
  T2  observe,resume,pause queued back-to-back: the pre-stop resume must be
      refused; after the pause response is drained, a FRESH resume succeeds
      on the FIRST attempt (no hidden retries).
  T3  EOF/disconnect with a pending observe: bounded prompt termination
      (well below the 10 min session lifetime). TCP documented lifecycle is
      ONE authenticated connection per host process: a clean disconnect MUST
      terminate the host boundedly. (A new trial may simply start a new
      host; there is no product reconnect requirement.)
  T4  Backlog flood while observe is pending: queue overflow must be an
      EXPLICIT refusal or bounded EOF, never silently-accepted requests.
      Oversized frames (>1 MiB cap) sent under a pinned pending observe,
      generated in chunks.
  T5  Malformed jsonrpc/version frames and a payload STRING containing
      cancellation text must never behave as control for an unrelated
      in-flight request.

Every wait is bounded; all spawned processes run in their OWN process group
(start_new_session) and only those owned groups are ever signalled.

Usage:
  python3 tests/mcp_transport_regression.py --host ./target/release/computer-host \
      --transport both
Exit: 0 = all checks pass, 1 = failures, 2 = usage/safety refusal.
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import secrets
import select
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any, Dict, List, Optional, Tuple

PROTO = "2024-11-05"
OBSERVE_WAIT_MS = 3000
# Response deadline: wait_ms + slack; far below the 20s transport call
# timeout and the 600s session lifetime.
RESP_SLACK = 7.0
EOF_BOUND = 15.0          # EOF cleanup must land well below the 10min timeout
FLOOD_BOUND = 25.0        # overflow must be explicit within this bound
FLOOD_FRAMES = 70         # just above the 64-frame bounded queue
OVERSIZE_BYTES = 1_048_576 + 32
OVERSIZE_CHUNK = 65536
OVERSIZE_BATCH = 68       # bounded batch of oversized frames under backlog
WRITE_BOUND = 25.0        # flood writes must complete (or give up) within this

RESULTS: List[Tuple[bool, str, str]] = []


class WriteTimeout(Exception):
    """The peer stopped draining before the bounded write budget expired."""


def check(name: str, ok: bool, detail: str = "") -> bool:
    RESULTS.append((ok, name, detail))
    print(f"{'PASS' if ok else 'FAIL'} {name}" + (f" :: {detail[:400]}" if detail and not ok else ""))
    return ok


def _check_classifier_selftest() -> None:
    """Cheap local checks of classify_cancelled (no fake GUI evidence)."""
    cancelled_tool = {"result": {"isError": True, "content": [
        {"type": "text", "text": '{"error":{"code":"cancelled","message":"x"}}'}]}}
    validation_tool = {"result": {"isError": True, "content": [
        {"type": "text", "text": '{"error":{"code":"invalid_params","message":"bad"}}'}]}}
    capture_tool = {"result": {"isError": True, "content": [
        {"type": "text", "text": '{"error":{"code":"capture_failed"}}'}]}}
    success_tool = {"result": {"isError": False, "content": [
        {"type": "text", "text": '{"observation_id":"obs-1"}'}]}}
    wire_err = {"error": {"code": -32800, "message": "cancelled"}}
    cases = [(cancelled_tool, "cancelled"), (validation_tool, "other_error"),
             (capture_tool, "other_error"), (success_tool, "clean_success"),
             (wire_err, "cancelled")]
    bad = [f"{i}:{classify_cancelled(r)}!={want}"
           for i, (r, want) in enumerate(cases) if classify_cancelled(r) != want]
    check("classifier self-test (exact cancelled code vs generic errors)",
          not bad, "; ".join(bad))


def info(name: str, detail: str) -> None:
    """Recorded classification note, never a pass/fail distortion."""
    print(f"INFO {name} :: {detail[:400]}")


def line(obj: Dict[str, Any]) -> bytes:
    return json.dumps(obj).encode() + b"\n"


def cancelled_notification(request_id: Any) -> bytes:
    return line({"jsonrpc": "2.0", "method": "notifications/cancelled",
                 "params": {"requestId": request_id, "reason": "qa-transport-ingress"}})


def text_meta(result: Dict[str, Any]) -> Dict[str, Any]:
    for block in (result or {}).get("content", []):
        if isinstance(block, dict) and block.get("type") == "text":
            try:
                return json.loads(block.get("text", ""))
            except json.JSONDecodeError:
                continue
    return {}


def classify_cancelled(resp: Dict[str, Any]) -> str:
    """'cancelled' (explicit, structured) | 'clean_success' | 'other_error'.

    A structured JSON-RPC error only counts as cancellation when its numeric
    CODE or its message explicitly says cancelled/canceled — never merely
    because an arbitrary isError/tool-error payload happens to contain the
    substring 'cancel' somewhere (schema errors, invalid session ids, worker
    faults etc. must stay distinguishable as 'other_error').

    The REAL cancelled tool response is result.isError=true whose content
    text carries an EXACT structured code: {"error":{"code":"cancelled",...}}.
    Only that exact code (or the canceled spelling) inside meta["error"]
    counts; arbitrary bare isError never does."""
    if "error" in resp:
        err = resp["error"]
        if not isinstance(err, dict):
            return "other_error"
        code = err.get("code")
        if code in (-32800, "cancelled", "canceled"):
            return "cancelled"
        msg = str(err.get("message", "")).lower()
        if "cancelled" in msg or "canceled" in msg:
            return "cancelled"
        return "other_error"
    result = resp.get("result") or {}
    meta = text_meta(result)
    # Structured success metadata may carry an explicit cancelled state.
    if str(meta.get("state", "")).lower() in ("cancelled", "canceled"):
        return "cancelled"
    if meta.get("cancelled") is True or meta.get("canceled") is True:
        return "cancelled"
    # Exact structured error code inside the tool-result text JSON: the real
    # cancelled tool response is isError=true + {"error":{"code":"cancelled"}}.
    meta_err = meta.get("error")
    if isinstance(meta_err, dict) and str(meta_err.get("code", "")).lower() in (
            "cancelled", "canceled"):
        return "cancelled"
    # A plain isError with no structured cancelled marker is NOT proof of
    # explicit cancellation (generic validation/capture/worker errors).
    return "other_error" if result.get("isError") else "clean_success"


def deadline_write(fd: int, data: bytes, deadline: float) -> None:
    """Bounded nonblocking write of all of `data` to `fd`.

    Returns when every byte is handed to the peer's kernel buffers. Raises
    WriteTimeout if the peer stopped draining and `deadline` expired; OSError
    subclasses (BrokenPipeError, ...) propagate on a dead peer. Never blocks
    unboundedly on a wedged host."""
    view = memoryview(data)
    sent = 0
    while sent < len(view):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise WriteTimeout(f"peer not draining; {len(view) - sent} bytes unsent")
        _, writable, _ = select.select([], [fd], [], remaining)
        if not writable:
            raise WriteTimeout(f"peer not draining; {len(view) - sent} bytes unsent")
        try:
            sent += os.write(fd, view[sent:sent + OVERSIZE_CHUNK])
        except BlockingIOError:
            continue


def deadline_send(sock: socket.socket, data: bytes, deadline: float) -> None:
    """Bounded TCP write via per-call socket.send(..., MSG_DONTWAIT).

    The socket stays in BLOCKING mode the whole time so the shared blocking
    makefile("rb") reader is never touched (no setblocking/settimeout
    toggling on a socket the reader thread is actively using — such a toggle
    is a data race that can corrupt the reader into a spurious EOF). Each
    individual send call is nonblocking via MSG_DONTWAIT, and select bounds
    the wait between calls. Raises WriteTimeout when the peer stops draining
    and `deadline` expires — including the case where select keeps reporting
    writable but the send buffer never accepts a byte."""
    dontwait = getattr(socket, "MSG_DONTWAIT", None)
    if dontwait is None:
        raise RuntimeError(
            "platform lacks MSG_DONTWAIT; this harness's bounded TCP write "
            "path requires a POSIX per-call nonblocking send (this harness "
            "runs on a POSIX coordinator such as macOS, not Windows)")
    view = memoryview(data)
    sent = 0
    while sent < len(view):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise WriteTimeout(f"peer not draining; {len(view) - sent} bytes unsent")
        _, writable, _ = select.select([], [sock.fileno()], [], remaining)
        if not writable:
            raise WriteTimeout(f"peer not draining; {len(view) - sent} bytes unsent")
        n = sock.send(view[sent:sent + OVERSIZE_CHUNK], dontwait)
        if n <= 0:  # kernel accepted nothing: burn deadline, never spin forever
            time.sleep(0.005)
        sent += n


def _check_backpressure_selftest() -> None:
    """Harness-local socketpair backpressure regression (no GUI, no host):

    drain nothing on one end of a socketpair and write well past the kernel
    buffer through deadline_send with a short budget — the budget MUST
    expire as WriteTimeout (a blocking write would hang forever here), then
    a drained peer accepts a small write normally. Cleanup is bounded
    (socket.close can never block)."""
    a, b = socket.socketpair()
    try:
        a.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 65536)
        b.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 65536)
        budget_hit = False
        t0 = time.monotonic()
        try:
            deadline_send(a, b"B" * (8 * 1024 * 1024), t0 + 0.6)
        except WriteTimeout:
            budget_hit = True
        spent = time.monotonic() - t0
        check("backpressure self-test: undrained peer expires the write budget",
              budget_hit and spent < 2.0,
              f"budget_hit={budget_hit} spent={spent:.2f}s — 8MiB against a "
              "peer that never drains must expire the 0.6s budget, not hang "
              "or complete (the write budget is not actually enforced)")
        # Sanity: drain the backlog until it empties (bounded), then a peer
        # that drains accepts a normal small write.
        b.settimeout(2.0)
        drained = 0
        try:
            while drained < 8 * 1024 * 1024 + 2:
                chunk = b.recv(1 << 20)
                if not chunk:
                    break
                drained += len(chunk)
        except socket.timeout:
            pass
        b.settimeout(None)
        deadline_send(a, b"hi", time.monotonic() + 2.0)
        got = b.recv(2)
        check("backpressure self-test: drained peer accepts writes", got == b"hi",
              f"recv={got!r} drained={drained}")
    finally:
        a.close()
        b.close()


class Host:
    """One owned host process (own process group) + one framed connection.

    transport='stdio': frames over stdin/stdout pipes.
    transport='tcp':   loopback socket, token auth on the first line;
                       documented lifecycle is ONE authenticated connection
                       per host process (clean EOF must terminate it).
    """

    def __init__(self, binary: str, transport: str, workdir: str):
        self.transport = transport
        self.workdir = workdir
        self.token: Optional[str] = None
        self.proc: Optional[subprocess.Popen] = None
        argv = [binary, "--mock-backend"]
        if transport == "tcp":
            probe = socket.socket()
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
            probe.close()
            self.token = secrets.token_hex(24)
            # Unique per Host: many Hosts share one test workdir, and the
            # exclusive-create semantics must not collide across trials.
            token_path = os.path.join(workdir, f"token-{secrets.token_hex(8)}")
            fd = os.open(token_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "w") as f:
                f.write(self.token + "\n")
            argv += ["--listen", f"127.0.0.1:{port}", "--token-file", token_path]
            self.port = port
        try:
            self.proc = subprocess.Popen(
                argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE, start_new_session=True)
            # stdio writes go through bounded nonblocking os.write so a wedged
            # host can never block the harness on a full pipe.
            if transport == "stdio":
                os.set_blocking(self.proc.stdin.fileno(), False)
            self.stderr_tail: List[bytes] = []
            threading.Thread(target=self._drain, daemon=True).start()
            self._lines: "queue.Queue[Optional[bytes]]" = queue.Queue()
            self._closed = False        # local disconnect performed by us
            self._peer_eof = False      # genuine EOF (empty readline) observed
            self._reader_error = False  # reader exception, NOT proof of EOF
            self._reader_started = False
            self.sock: Optional[socket.socket] = None
            self._sock_file = None      # makefile("rb") wrapper, owns a ref
            self._reader_thread: Optional[threading.Thread] = None
            if transport == "stdio":
                self._start_reader(self.proc.stdout)
            else:
                self._wait_listen()
                self.connect()
        except BaseException:
            # Construction/startup failure must still clean the owned group.
            if self.proc is not None:
                self.terminate()
            raise

    def _drain(self) -> None:
        for raw in self.proc.stderr:
            self.stderr_tail.append(raw)
            del self.stderr_tail[:-200]
        # Peer closed stderr; drop the reference so teardown is clean.
        try:
            self.proc.stderr.close()
        except Exception:
            pass

    def _wait_listen(self, timeout: float = 10.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError(f"tcp host exited at startup rc={self.proc.returncode}")
            try:
                s = socket.create_connection(("127.0.0.1", self.port), timeout=0.5)
                s.close()
                return
            except OSError:
                time.sleep(0.05)
        raise RuntimeError("tcp host did not start listening")

    def _start_reader(self, source) -> None:
        def run() -> None:
            try:
                while True:
                    data = source.readline()
                    if not data:
                        self._peer_eof = True   # GENUINE EOF: empty readline
                        break
                    self._lines.put(data)
            except (OSError, ValueError):
                # A reader EXCEPTION is not necessarily peer EOF (e.g. our own
                # local teardown closing the reader file); it is recorded
                # separately and is NEVER counted as received EOF in overflow
                # or termination evidence.
                self._reader_error = True
            self._lines.put(None)     # end-of-stream marker (EOF or error)
        self._reader_thread = threading.Thread(target=run, daemon=True)
        self._reader_thread.start()
        self._reader_started = True

    def connect(self) -> None:
        """(TCP) open the single authenticated connection for this process.

        The documented lifecycle is one connection per host process; this is
        called exactly once, at construction."""
        assert self.transport == "tcp"
        self.sock = socket.create_connection(("127.0.0.1", self.port), timeout=10.0)
        self.sock.settimeout(None)  # blocking mode; deadline_write drives all writes
        self.sock.sendall(self.token.encode() + b"\n")  # token is never printed
        self._lines = queue.Queue()
        self._closed = False
        self._peer_eof = False
        self._reader_error = False
        self._sock_file = self.sock.makefile("rb")
        self._start_reader(self._sock_file)

    # ---- framed I/O -----------------------------------------------------
    def write_raw(self, data: bytes, timeout: float = 15.0) -> None:
        """Bounded frame write: never blocks past `timeout` on a wedged host.

        stdio goes through select/os.write deadline_write on the (genuinely
        nonblocking) pipe fd. TCP goes through deadline_send: the socket
        stays BLOCKING for the shared makefile reader, and each send call is
        individually nonblocking (MSG_DONTWAIT) under a select deadline —
        select(writable)+blocking os.write on a socket with a full peer
        buffer could hang, and toggling the shared socket's blocking flag
        would race the reader."""
        if self.transport == "stdio":
            deadline_write(self.proc.stdin.fileno(), data,
                           time.monotonic() + timeout)
        else:
            deadline_send(self.sock, data, time.monotonic() + timeout)

    def read_msg(self, timeout: float) -> Optional[Dict[str, Any]]:
        """Next parsed JSON object, or None on EOF/timeout before one."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                raw = self._lines.get(timeout=max(0.05, deadline - time.monotonic()))
            except queue.Empty:
                return None
            if raw is None:
                return None
            stripped = raw.strip()
            if not stripped:
                continue
            try:
                msg = json.loads(stripped.decode())
            except (UnicodeDecodeError, json.JSONDecodeError):
                check("frames are valid JSON", False, f"garbage frame: {stripped[:120]!r}")
                continue
            return msg
        return None

    def wait_for(self, want_id: Any, timeout: float) -> Tuple[Optional[Dict[str, Any]], List[Dict[str, Any]]]:
        """Collect frames until the response for want_id (matched by exact
        id) arrives. Returns (matching_response, other_frames_in_order)."""
        others: List[Dict[str, Any]] = []
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            msg = self.read_msg(max(0.05, deadline - time.monotonic()))
            if msg is None:
                return None, others
            if msg.get("id") == want_id and ("result" in msg or "error" in msg):
                return msg, others
            others.append(msg)
        return None, others

    # ---- lifecycle ------------------------------------------------------
    def request(self, rid: Any, method: str, params: Optional[dict] = None,
                timeout: float = 15.0) -> Dict[str, Any]:
        self.write_raw(line({"jsonrpc": "2.0", "id": rid, "method": method,
                             "params": params or {}}))
        resp, _ = self.wait_for(rid, timeout)
        if resp is None:
            raise TimeoutError(f"no response for {method} id={rid!r}")
        return resp

    def call_tool(self, rid: Any, name: str, args: dict, timeout: float = 15.0) -> Dict[str, Any]:
        return self.request(rid, "tools/call", {"name": name, "arguments": args}, timeout)

    def handshake(self, tag: str) -> None:
        resp = self.request(f"{tag}-init", "initialize", {
            "protocolVersion": PROTO, "capabilities": {},
            "clientInfo": {"name": "qa-transport-ingress", "version": "0.1"}})
        if "error" in resp:
            raise RuntimeError(f"initialize failed: {resp['error']}")
        self.write_raw(line({"jsonrpc": "2.0", "method": "notifications/initialized"}))

    def open_session(self, tag: str) -> str:
        resp = self.call_tool(f"{tag}-open", "computer_open", {})
        meta = text_meta(resp.get("result") or {})
        sid = meta.get("session_id")
        if not isinstance(sid, str) or not sid:
            raise RuntimeError(f"open failed: {json.dumps(resp)[:300]}")
        return sid

    def disconnect(self) -> None:
        """EOF the peer (stdio: close stdin; tcp: shutdown+close socket).

        TCP: the makefile("rb") reader holds its own reference, so closing
        the socket object alone does NOT deliver EOF to the reader. We
        shutdown(SHUT_RDWR) FIRST (unblocks the reader with a real EOF and
        delivers EOF to the peer), then close the socket and the reader
        file, and join the reader thread with a bound. _closed records OUR
        local disconnect only; actual peer EOF is tracked separately in
        _peer_eof and is never assumed."""
        if self._closed:
            return
        self._closed = True
        try:
            if self.transport == "stdio":
                self.proc.stdin.close()
            else:
                try:
                    self.sock.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                try:
                    self.sock.close()
                except OSError:
                    pass
                if self._sock_file is not None:
                    try:
                        self._sock_file.close()
                    except (OSError, ValueError):
                        pass
                if (self._reader_thread is not None
                        and self._reader_thread is not threading.current_thread()):
                    self._reader_thread.join(timeout=2.0)
        except (OSError, ValueError):
            pass

    def terminate(self, grace: float = 5.0) -> None:
        """Bounded teardown of our OWN process group only (no broad kills)."""
        if self.proc is None:
            return
        try:
            self.disconnect()
        except Exception:
            pass
        if self.proc.poll() is None:
            try:
                os.killpg(self.proc.pid, signal.SIGKILL)  # owned group, verified by start_new_session
            except (ProcessLookupError, PermissionError):
                pass
            try:
                self.proc.wait(timeout=grace)
            except subprocess.TimeoutExpired:
                pass

    def stderr_text(self) -> str:
        return b"".join(self.stderr_tail).decode("utf-8", "replace")


# -------------------------------------------------------------------------
# T1 — same-write cancel race (3 trials, fresh host+session each)
# -------------------------------------------------------------------------

def t1_same_write_cancel(binary: str, transport: str, workdir: str) -> None:
    for trial in range(1, 4):
        h = Host(binary, transport, workdir)
        try:
            h.handshake(f"t1-{trial}")
            sid = h.open_session(f"t1-{trial}")
            rid = f"t1-{trial}-obs-cancelled-race"  # escaped string id
            h.write_raw(
                line({"jsonrpc": "2.0", "id": rid, "method": "tools/call", "params": {
                    "name": "computer_observe",
                    "arguments": {"session_id": sid, "wait_ms": OBSERVE_WAIT_MS}}})
                + cancelled_notification(rid))  # ONE write, no sleep between
            t0 = time.monotonic()
            resp, _ = h.wait_for(rid, OBSERVE_WAIT_MS / 1000 + RESP_SLACK)
            dt = time.monotonic() - t0
            if resp is None:
                check(f"T1.{trial} cancelled observe completes within bound", False,
                      "no response (hung past wait_ms+slack)")
            else:
                cls = classify_cancelled(resp)
                check(f"T1.{trial} cancelled observe completes within bound", True,
                      f"{dt:.2f}s")
                check(f"T1.{trial} cancelled observe is never a clean success",
                      cls == "cancelled",
                      f"classification={cls} in {dt:.2f}s (race lost: cancel behind "
                      f"request was not honored): {json.dumps(resp)[:300]}")
        finally:
            h.terminate()

    # Unknown id must not cancel a valid in-flight request.
    h = Host(binary, transport, workdir)
    try:
        h.handshake("t1u")
        sid = h.open_session("t1u")
        rid = "t1u-valid-observe"
        h.write_raw(
            line({"jsonrpc": "2.0", "id": rid, "method": "tools/call", "params": {
                "name": "computer_observe",
                "arguments": {"session_id": sid, "wait_ms": 600}}})
            + cancelled_notification("t1u-NO-SUCH-REQUEST"))  # unknown id, same write
        resp, _ = h.wait_for(rid, 0.6 + RESP_SLACK)
        if resp is None:
            check("T1.unknown-id cancel leaves valid request alive", False, "no response")
        else:
            cls = classify_cancelled(resp)
            check("T1.unknown-id cancel leaves valid request alive", cls == "clean_success",
                  f"classification={cls}: unknown requestId cancelled/tainted an unrelated "
                  f"in-flight request: {json.dumps(resp)[:300]}")
    finally:
        h.terminate()


# -------------------------------------------------------------------------
# T2 — pre-stop queued resume refused; fresh resume succeeds first try
# -------------------------------------------------------------------------

def t2_stale_resume(binary: str, transport: str, workdir: str) -> None:
    h = Host(binary, transport, workdir)
    try:
        h.handshake("t2")
        sid = h.open_session("t2")
        id_obs, id_resume_old, id_pause = "t2-obs", "t2-resume-queued", "t2-pause"
        h.write_raw(
            line({"jsonrpc": "2.0", "id": id_obs, "method": "tools/call", "params": {
                "name": "computer_observe",
                "arguments": {"session_id": sid, "wait_ms": OBSERVE_WAIT_MS}}})
            + line({"jsonrpc": "2.0", "id": id_resume_old, "method": "tools/call", "params": {
                "name": "computer_resume", "arguments": {"session_id": sid}}})
            + line({"jsonrpc": "2.0", "id": id_pause, "method": "tools/call", "params": {
                "name": "computer_pause", "arguments": {"session_id": sid}}}))
        # Drain by exact id; independent of response order, no retries.
        deadline = time.monotonic() + OBSERVE_WAIT_MS / 1000 + RESP_SLACK + 5
        got: Dict[str, Dict[str, Any]] = {}
        while len(got) < 3 and time.monotonic() < deadline:
            msg = h.read_msg(max(0.05, deadline - time.monotonic()))
            if msg is None:
                break
            mid = msg.get("id")
            if mid in (id_obs, id_resume_old, id_pause) and ("result" in msg or "error" in msg):
                got[mid] = msg
        if len(got) < 3:
            check("T2 all three queued responses arrive by id", False,
                  f"only got ids {sorted(got)}")
            return
        check("T2 all three queued responses arrive by id", True)

        old = classify_cancelled(got[id_resume_old])
        check("T2 pre-stop queued resume is refused (cancelled/error)",
              old == "cancelled" or old == "other_error",
              f"classification={old}: a resume accepted BEFORE the pause stop "
              f"restored the session: {json.dumps(got[id_resume_old])[:300]}")

        pause_meta = text_meta((got[id_pause].get("result") or {}))
        pause_blob = json.dumps(got[id_pause]).lower()
        check("T2 pause took effect (paused state or explicit stop)",
              "pause" in pause_blob or pause_meta.get("state") in ("paused", "closed")
              or classify_cancelled(got[id_pause]) == "cancelled",
              f"{json.dumps(got[id_pause])[:300]}")

        # Fresh resume AFTER the pause response was drained: FIRST attempt.
        resp = h.call_tool("t2-resume-fresh", "computer_resume", {"session_id": sid})
        cls = classify_cancelled(resp)
        check("T2 fresh resume after drained pause succeeds on FIRST request",
              cls == "clean_success",
              f"classification={cls}: {json.dumps(resp)[:300]}")
    finally:
        h.terminate()


# -------------------------------------------------------------------------
# T3 — EOF/disconnect with pending observe
# -------------------------------------------------------------------------

def t3_eof(binary: str, transport: str, workdir: str) -> None:
    h = Host(binary, transport, workdir)
    try:
        h.handshake("t3")
        sid = h.open_session("t3")
        h.write_raw(line({"jsonrpc": "2.0", "id": "t3-obs", "method": "tools/call", "params": {
            "name": "computer_observe",
            "arguments": {"session_id": sid, "wait_ms": OBSERVE_WAIT_MS}}}))
        h.disconnect()  # EOF while the observe is still pending
        t0 = time.monotonic()
        try:
            rc = h.proc.wait(timeout=EOF_BOUND)
        except subprocess.TimeoutExpired:
            rc = None
        dt = time.monotonic() - t0
        if transport == "stdio":
            check("T3 stdio host terminates promptly on EOF with pending observe",
                  rc is not None,
                  f"still running after {dt:.1f}s (bound {EOF_BOUND}s; session "
                  f"lifetime is 600s — this must be far below it)")
            if rc is not None:
                info("T3 stdio exit", f"rc={rc} after {dt:.1f}s; "
                     f"stderr tail: {h.stderr_text()[-200:].strip()!r}")
        else:
            # TCP documented lifecycle: ONE authenticated connection per host
            # process — a clean EOF MUST terminate the host boundedly. An
            # alive host after its only client disconnected is a failure,
            # regardless of what its stderr happens to contain; there is no
            # product reconnect requirement (a new trial starts a new host).
            check("T3 tcp clean EOF on the single connection terminates the host",
                  rc is not None,
                  f"host still running {dt:.1f}s after its only authenticated "
                  f"connection got a clean EOF (bound {EOF_BOUND}s); the "
                  f"documented one-connection-per-process lifecycle requires "
                  f"bounded termination; stderr tail: "
                  f"{h.stderr_text()[-200:].strip()!r}")
            if rc is not None:
                info("T3 tcp classification",
                     f"host exited rc={rc} after {dt:.1f}s on clean EOF of its "
                     f"single authenticated connection")
    finally:
        h.terminate()


# -------------------------------------------------------------------------
# T4 — bounded backlog: explicit overflow refusal; oversized frames
# -------------------------------------------------------------------------

def t4_flood(binary: str, transport: str, workdir: str) -> None:
    # 4a: small-frame flood past the bounded queue while observe is pending.
    h = Host(binary, transport, workdir)
    try:
        h.handshake("t4")
        sid = h.open_session("t4")
        frames = [line({"jsonrpc": "2.0", "id": "t4-obs", "method": "tools/call", "params": {
            "name": "computer_observe",
            "arguments": {"session_id": sid, "wait_ms": OBSERVE_WAIT_MS}}})]
        for i in range(FLOOD_FRAMES):
            frames.append(line({"jsonrpc": "2.0", "id": f"t4-flood-{i}", "method": "ping"}))
        write_error: Optional[str] = None
        try:
            h.write_raw(b"".join(frames), timeout=WRITE_BOUND)
        except (BrokenPipeError, OSError, WriteTimeout) as e:
            # A write failure is NOT proof of peer EOF/termination: a
            # WriteTimeout can also mean a wedged host that stopped draining.
            write_error = f"{type(e).__name__}: {e}"
            info("T4 flood write", f"write path raised mid-flood: {write_error}")
        refusal = None
        answered_after_refusal = False
        # Always drain within the remaining bound — even after a write
        # failure, since a refusal frame may still be on the wire.
        deadline = time.monotonic() + FLOOD_BOUND
        while time.monotonic() < deadline:
            msg = h.read_msg(max(0.05, deadline - time.monotonic()))
            if msg is None:
                break
            err = msg.get("error") or {}
            if refusal is None and (
                    err.get("code") == -32000
                    or "overflow" in json.dumps(err).lower()
                    or "refused" in json.dumps(err).lower()):
                refusal = msg
            elif refusal is not None:
                answered_after_refusal = True
            if h.proc.poll() is not None:
                break
        # Actual termination/EOF derived ONLY from the reader's genuine-EOF
        # marker and the process poll — never from a write exception, and a
        # reader exception (_reader_error) is NOT counted as received EOF.
        terminated = h._peer_eof or h.proc.poll() is not None
        check("T4 flood past bounded queue: explicit refusal or bounded termination",
              refusal is not None or terminated,
              f"{FLOOD_FRAMES} queued frames accepted with NO refusal and host still "
              f"serving after {FLOOD_BOUND}s — silently-accepted backlog "
              f"(write_error={write_error!r}, peer_eof={h._peer_eof}, "
              f"reader_error={h._reader_error}, poll={h.proc.poll()})")
        check("T4 no responses AFTER an explicit overflow refusal",
              not answered_after_refusal,
              "host kept answering frames after declaring the connection refused")
    finally:
        h.terminate()

    # 4b: oversized frames (>1 MiB cap) sent UNDER BACKLOG: a pending
    # observe(wait_ms=3000) pins the worker first, so the 68 oversized frames
    # genuinely queue. Bytes are generated in chunks. Overflow MUST be an
    # explicit refusal and bounded connection termination; if the host
    # instead services everything, EVERY one of the 68 responses plus the
    # trailing ping must be accounted for — one error plus 67 silently lost
    # frames is exactly the bug under test and is a FAILURE.
    h = Host(binary, transport, workdir)
    try:
        h.handshake("t4b")
        sid = h.open_session("t4b")
        overflow_refused = False
        oversize_errors = 0     # error replies for too-large frames (often id=null)
        write_error: Optional[str] = None
        unexpected: List[Dict[str, Any]] = []
        by_id: Dict[Any, Dict[str, Any]] = {}
        # ONE deadline covers writes AND reads; the read loop never resets it.
        deadline = time.monotonic() + FLOOD_BOUND + WRITE_BOUND
        t0 = time.monotonic()
        try:
            h.write_raw(line({"jsonrpc": "2.0", "id": "t4b-obs", "method": "tools/call",
                              "params": {"name": "computer_observe",
                                         "arguments": {"session_id": sid,
                                                       "wait_ms": OBSERVE_WAIT_MS}}}),
                        timeout=max(0.1, deadline - time.monotonic()))
            for i in range(OVERSIZE_BATCH):
                # One oversized frame: valid JSON-RPC prefix + huge filler.
                head = b'{"jsonrpc":"2.0","id":"t4b-big-%d","method":"ping","params":{"filler":"' % i
                h.write_raw(head, timeout=max(0.1, deadline - time.monotonic()))
                remaining = OVERSIZE_BYTES - len(head) - 2
                while remaining > 0:
                    chunk = b"A" * min(OVERSIZE_CHUNK, remaining)
                    h.write_raw(chunk, timeout=max(0.1, deadline - time.monotonic()))
                    remaining -= len(chunk)
                h.write_raw(b'"}\n', timeout=max(0.1, deadline - time.monotonic()))
            # A trailing valid ping: if the host still serves normally it
            # answers this; if it tore down, EOF/poll proves termination.
            h.write_raw(line({"jsonrpc": "2.0", "id": "t4b-tail", "method": "ping"}),
                        timeout=max(0.1, deadline - time.monotonic()))
        except (BrokenPipeError, OSError, WriteTimeout) as e:
            # A write failure is NOT proof of peer EOF/termination; record it
            # separately and STILL drain the wire below (a too-large refusal
            # frame may already be queued for us).
            write_error = f"{type(e).__name__}: {e}"
            info("T4 oversized write", f"write path raised mid-batch: {write_error}")
        while time.monotonic() < deadline:
            msg = h.read_msg(max(0.05, deadline - time.monotonic()))
            if msg is None:
                break  # timeout, genuine peer EOF, or reader error (not EOF)
            err = msg.get("error") or {}
            blob = json.dumps(err).lower()
            if ("overflow" in blob or "refused" in blob or "too large" in blob
                    or err.get("code") == -32000):
                overflow_refused = True
            if "error" in msg and msg.get("id") is None and err:
                # Too-large frames cannot reliably expose their original id;
                # the host rejects them with a null-id error. Count them.
                oversize_errors += 1
            if msg.get("id") in ("t4b-obs", "t4b-tail") or (
                    isinstance(msg.get("id"), str)
                    and msg["id"].startswith("t4b-big-")):
                by_id[msg["id"]] = msg
            else:
                unexpected.append(msg)
        # Actual EOF/termination ONLY from the reader's genuine-EOF marker
        # (empty readline) and the process poll — never inferred from a write
        # exception (a write timeout can also mean a wedged host, which must
        # NOT false-pass this check), and a reader exception/timeout is NOT
        # counted as received EOF.
        peer_eof = h._peer_eof
        terminated = h.proc.poll() is not None
        alive = not terminated
        eof = peer_eof
        expected_ids = (["t4b-obs"] + [f"t4b-big-{i}" for i in range(OVERSIZE_BATCH)]
                        + ["t4b-tail"])
        missing = [i for i in expected_ids if i not in by_id]
        # Null-id oversized error replies account for big frames whose
        # original id could not be recovered.
        accounting_ok = (len(missing) <= oversize_errors and not unexpected)

        if overflow_refused:
            # Explicit overflow observed: the host must ALSO terminate the
            # connection/process boundedly (actual EOF on the wire or process
            # exit); it must not silently continue serving after refusing.
            check("T4 oversized backlog: explicit refusal AND bounded termination",
                  eof or terminated,
                  f"explicit overflow refusal seen but host still alive "
                  f"(poll={h.proc.poll()}, peer_eof={peer_eof}, "
                  f"write_error={write_error!r}) serving at deadline — "
                  f"refusal without bounded termination is a silent backlog")
        elif not alive or eof:
            check("T4 oversized backlog: explicit refusal AND bounded termination",
                  False,
                  f"host tore down (peer_eof={peer_eof}, alive={alive}, "
                  f"write_error={write_error!r}) WITHOUT an explicit overflow "
                  f"refusal frame — the required explicit refusal is missing")
        else:
            check("T4 oversized backlog: explicit refusal AND bounded termination",
                  False,
                  f"{OVERSIZE_BATCH}x{OVERSIZE_BYTES}-byte frames under backlog: "
                  f"no overflow refusal, host alive and serving at deadline "
                  f"(peer_eof={peer_eof}, write_error={write_error!r}) — "
                  f"silently-accepted unbounded backlog")
        if not (overflow_refused or not alive or eof):
            if accounting_ok:
                # Every frame got its own response and nothing extra: the
                # mandatory overflow path was NOT exercised. This is NOT a
                # pass for the overflow check above.
                info("T4 oversized accounting",
                     "all frames serviced, overflow not exercised "
                     f"({len(by_id)}/{len(expected_ids)} responses, "
                     f"null-id errors={oversize_errors})")
            else:
                check("T4 oversized backlog: every frame accounted for when serviced",
                      False,
                      f"missing responses for {len(missing)} ids "
                      f"(e.g. {missing[:3]}); null-id errors={oversize_errors}; "
                      f"unexpected frames: "
                      f"{[m.get('id') for m in unexpected][:5]} — a live host "
                      f"with dropped frames is the exact bug under test")
        info("T4 oversized classification",
             f"overflow_refused={overflow_refused} write_error={write_error!r} "
             f"peer_eof={peer_eof} reader_error={h._reader_error} host_alive={alive} "
             f"answered={len(by_id)}/"
             f"{len(expected_ids)} null_id_errors={oversize_errors} "
             f"elapsed={time.monotonic() - t0:.1f}s — this asserts explicit "
             f"bounded refusal/termination only; total memory safety is NOT "
             f"provable by protocol test alone (scope documented)")
    finally:
        h.terminate()


# -------------------------------------------------------------------------
# T5 — malformed/cancel-text frames never act as control
# -------------------------------------------------------------------------

def image_block_present(result: Dict[str, Any]) -> bool:
    for block in (result or {}).get("content", []):
        if isinstance(block, dict) and block.get("type") == "image" and block.get("data"):
            return True
    return False


def step_executed_ok(resp: Dict[str, Any]) -> bool:
    """True iff the step reply proves a REAL executed schema-valid step:

    result.isError must be false, input_outcome must be "dispatched",
    observation_outcome must be "available", the NESTED observation object
    (step.data['observation'], NOT the top level — verified in
    src/runtime/runtime/tests.rs full_happy_path and
    src/runtime/session.rs StepRecord::to_json) must carry a string
    observation_id, and the reply must contain an image content block.
    A schema rejection, stale_observation error, or any other error proves
    NOTHING about text-payload handling and returns False here."""
    if resp is None:
        return False
    result = resp.get("result") or {}
    if result.get("isError"):
        return False
    meta = text_meta(result)
    if meta.get("input_outcome") != "dispatched":
        return False
    if meta.get("observation_outcome") != "available":
        return False
    obs = meta.get("observation")
    if not isinstance(obs, dict) or not isinstance(obs.get("observation_id"), str):
        return False
    return image_block_present(result)


def t5_no_spoofed_control(binary: str, transport: str, workdir: str) -> None:
    h = Host(binary, transport, workdir)
    try:
        h.handshake("t5")
        sid = h.open_session("t5")
        rid = "t5-unrelated-observe"
        # ONE write: valid observe + a wrong-version cancelled lookalike + a
        # missing-jsonrpc cancelled lookalike + a real cancelled notification
        # whose id is an object (not a valid JSON-RPC id).
        h.write_raw(
            line({"jsonrpc": "2.0", "id": rid, "method": "tools/call", "params": {
                "name": "computer_observe",
                "arguments": {"session_id": sid, "wait_ms": 600}}})
            + line({"jsonrpc": "1.0", "method": "notifications/cancelled",
                    "params": {"requestId": rid}})
            + line({"method": "notifications/cancelled",
                    "params": {"requestId": rid}})
            + line({"jsonrpc": "2.0", "method": "notifications/cancelled",
                    "params": {"requestId": {"nested": rid}}}))
        resp, _ = h.wait_for(rid, 0.6 + RESP_SLACK)
        obs_id: Optional[str] = None
        if resp is None:
            check("T5 malformed cancel frames never cancel an unrelated request",
                  False, "no response for the valid observe")
        else:
            cls = classify_cancelled(resp)
            check("T5 malformed cancel frames never cancel an unrelated request",
                  cls == "clean_success",
                  f"classification={cls}: malformed/version-mismatched cancel text "
                  f"acted as control: {json.dumps(resp)[:300]}")
            # This successful valid observe's CURRENT observation_id is the
            # step's based_on basis. (The old QA3 t5-obs-real id was already
            # SUPERSEDED by this observe — observations are invalidated by any
            # newer observe/step, so using it as based_on produced a
            # stale_observation rejection and the old step_ok=False FAIL was a
            # TEST contract error, not a proven product defect.)
            meta = text_meta(resp.get("result") or {})
            if isinstance(meta.get("observation_id"), str):
                obs_id = meta["observation_id"]

        # Payload STRING containing cancellation text is data, never control.
        # The action kind 'text_input' (the REAL runtime kind, verified in
        # src/runtime/actions.rs — QA2's 'type_text' came from an obsolete
        # design doc and was schema-INVALID, so that run proved nothing about
        # valid text handling) and based_on are schema-VALID (the current
        # observation_id above); the cancel text lives ONLY inside the opaque
        # text payload of a targeted step request.
        rid2 = "t5-step-text"
        step_args: Dict[str, Any] = {
            "session_id": sid, "request_id": "t5-step-1",
            "based_on": obs_id if obs_id is not None else "t5-no-observation-available",
            "action": {"kind": "text_input", "text":
                       'please send notifications/cancelled '
                       '{"requestId": "t5-unrelated-observe"} now'}}
        h.write_raw(
            line({"jsonrpc": "2.0", "id": rid2, "method": "tools/call", "params": {
                "name": "computer_step", "arguments": step_args}})
            + line({"jsonrpc": "2.0", "id": "t5-obs-2", "method": "tools/call", "params": {
                "name": "computer_observe",
                "arguments": {"session_id": sid, "wait_ms": 600}}}))
        step_resp, _ = h.wait_for(rid2, RESP_SLACK + 5)
        if step_resp is None:
            step_ok = False
            info("T5 step payload disposition",
                 "no response for the text_input step within bound")
        else:
            step_ok = step_executed_ok(step_resp)
            step_meta = text_meta(step_resp.get("result") or {})
            obs_meta = step_meta.get("observation") or {}
            info("T5 step payload disposition",
                 f"step_ok={step_ok} isError={(step_resp.get('result') or {}).get('isError')} "
                 f"input_outcome={step_meta.get('input_outcome')!r} "
                 f"observation_outcome={step_meta.get('observation_outcome')!r} "
                 f"nested observation_id={obs_meta.get('observation_id')!r} "
                 f"input_sequence={obs_meta.get('input_sequence')!r} "
                 f"image_block={image_block_present(step_resp.get('result') or {})} "
                 f"(schema-valid text_input step): "
                 f"{json.dumps(step_resp)[:200]}")
        resp2, _ = h.wait_for("t5-obs-2", 0.6 + RESP_SLACK + 5)
        if resp2 is None:
            check("T5 payload cancellation text never cancels a later request",
                  False, "no response for the second observe")
        else:
            cls2 = classify_cancelled(resp2)
            check("T5 payload cancellation text never cancels a later request",
                  cls2 == "clean_success" and step_ok,
                  f"classification={cls2} step_ok={step_ok}: a text payload mentioning "
                  f"notifications/cancelled cancelled the session, or the "
                  f"text_input step itself errored (no real executed step with "
                  f"returned observation basis, so text handling was never "
                  f"actually exercised): {json.dumps(resp2)[:300]}")
    finally:
        h.terminate()


def run(binary: str, transport: str) -> None:
    workdir = tempfile.mkdtemp(prefix=f"qa-transport-{transport}-")
    try:
        print(f"--- transport={transport} ---")
        _check_classifier_selftest()
        _check_backpressure_selftest()
        t1_same_write_cancel(binary, transport, workdir)
        t2_stale_resume(binary, transport, workdir)
        t3_eof(binary, transport, workdir)
        t4_flood(binary, transport, workdir)
        t5_no_spoofed_control(binary, transport, workdir)
    finally:
        shutil.rmtree(workdir, ignore_errors=True)


def main(argv: List[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--host", required=True, help="path to the computer-host binary")
    ap.add_argument("--transport", choices=["stdio", "tcp", "both"], default="both")
    args = ap.parse_args(argv)

    if not os.path.isfile(args.host) or not os.access(args.host, os.X_OK):
        print(f"host binary not found/executable: {args.host}", file=sys.stderr)
        return 2

    # --mock-backend is MANDATORY and hardwired; there is no native option.
    transports = ["stdio", "tcp"] if args.transport == "both" else [args.transport]
    for t in transports:
        run(args.host, t)

    passed = sum(1 for ok, _, _ in RESULTS if ok)
    failed = [(n, d) for ok, n, d in RESULTS if not ok]
    print(f"\n=== transport-ingress regressions: {passed} passed, {len(failed)} failed ===")
    for n, d in failed:
        print(f"FAIL {n} :: {d[:400]}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
