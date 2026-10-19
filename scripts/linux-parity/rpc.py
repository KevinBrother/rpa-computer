"""Real newline-framed stdio JSON-RPC, nonblocking writes and reads, no retry."""
import hashlib
import json
import os
import selectors
import subprocess
import time

from common import Failure, require, spawn, stop_owned

MAX_FRAME = 32 * 1024 * 1024
MAX_PENDING = 8


class Rpc:
    def __init__(self, host, deadline, evidence, request_timeout=20, env=None):
        self.deadline = deadline
        self.evidence = evidence
        self.timeout = request_timeout
        self.process = spawn([host], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, env=env, bufsize=0)
        self.selector = selectors.DefaultSelector()
        for name in ("stdin", "stdout", "stderr"):
            os.set_blocking(getattr(self.process, name).fileno(), False)
        self.selector.register(self.process.stdout, selectors.EVENT_READ, "stdout")
        self.selector.register(self.process.stderr, selectors.EVENT_READ, "stderr")
        self.buffer = bytearray()
        self.pending = {}
        self.replies = {}
        self.serial = 0
        self.notifications = 0
        self.stderr_bytes = 0
        self.stderr_hash = hashlib.sha256()
        self.eof = False
        self.poisoned = False

    def _limit(self, end):
        # Every outstanding request retains its original deadline, even while
        # waiting for a different response (e.g. cancellation or pause).
        ends = [end] + [v[0] for k, v in self.pending.items() if k not in self.replies]
        limit = min(self.deadline.left(), min(ends) - time.monotonic())
        if limit <= 0:
            self.poisoned = True
            raise TimeoutError("JSON-RPC request deadline exceeded; no retry")
        return limit

    def _pump(self, end, write_buffer=None):
        for key, _ in self.selector.select(self._limit(end)):
            if key.data == "stdin":
                if write_buffer:
                    try:
                        size = os.write(key.fd, write_buffer)
                        del write_buffer[:size]
                    except BlockingIOError:
                        pass
                continue
            try:
                chunk = os.read(key.fd, 65536)
            except BlockingIOError:
                continue
            if not chunk:
                self.selector.unregister(key.fileobj)
                if key.data == "stdout":
                    self.eof = True
                    require(not self.buffer, "EOF in partial JSON-RPC frame")
                continue
            if key.data == "stderr":
                # No raw stderr: third-party errors can contain env values.
                self.stderr_bytes += len(chunk)
                self.stderr_hash.update(chunk)
                require(self.stderr_bytes <= MAX_FRAME, "stderr flood")
                continue
            self.buffer.extend(chunk)
            require(len(self.buffer) <= MAX_FRAME, "JSON-RPC frame exceeds bound")
            while b"\n" in self.buffer:
                line, _, tail = self.buffer.partition(b"\n")
                self.buffer = bytearray(tail)
                require(bool(line.strip()), "empty protocol frame")
                frame = json.loads(line)
                require(isinstance(frame, dict) and frame.get("jsonrpc") == "2.0",
                        "invalid JSON-RPC envelope")
                if "id" not in frame:
                    require(isinstance(frame.get("method"), str), "invalid notification")
                    self.notifications += 1
                    require(self.notifications <= 128, "notification flood")
                    self.evidence.event("notification", method=frame["method"])
                    continue
                rid = frame["id"]
                require(type(rid) is int and rid in self.pending and rid not in self.replies,
                        "unknown/duplicate response id or unsupported server request")
                require(("result" in frame) != ("error" in frame), "invalid response fields")
                self.replies[rid] = frame

    def send(self, method, params=None, notify=False):
        require(not self.poisoned and not self.eof, "transport unavailable")
        require(len(self.pending) < MAX_PENDING, "too many outstanding RPCs")
        self.serial += 1
        rid = self.serial
        frame = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            frame["params"] = params
        end = time.monotonic() + self.deadline.left(self.timeout)
        if not notify:
            frame["id"] = rid
            self.pending[rid] = (end, method)
        data = bytearray((json.dumps(frame, ensure_ascii=True) + "\n").encode())
        require(len(data) <= MAX_FRAME, "outbound frame exceeds bound")
        self.evidence.event("rpc_request", frame=frame)
        self.selector.register(self.process.stdin, selectors.EVENT_WRITE, "stdin")
        try:
            while data:
                self._pump(end, data)
                require(not self.eof, "host EOF while sending request")
        except Exception:
            self.poisoned = True
            raise
        finally:
            self.selector.unregister(self.process.stdin)
        return None if notify else rid

    def receive(self, rid):
        require(rid in self.pending, "request not pending")
        try:
            while rid not in self.replies:
                self._pump(self.pending[rid][0])
                require(not self.eof or rid in self.replies, "host EOF before response")
            frame = self.replies.pop(rid)
            del self.pending[rid]
            if "error" in frame:
                self.evidence.event("rpc_error", id=rid, error=frame["error"])
                raise Failure("JSON-RPC error (see evidence)")
            return frame["result"]
        except Exception:
            self.poisoned = True
            raise

    def call(self, method, params=None):
        return self.receive(self.send(method, params))

    def eof_and_wait(self, seconds=8):
        if not self.process.stdin.closed:
            self.process.stdin.close()
        end = time.monotonic() + self.deadline.left(seconds)
        # Drain while exiting: a final screenshot must not fill stdout and deadlock.
        while not self.eof or self.process.poll() is None:
            self._pump(end)
        self.evidence.event("host_exit", returncode=self.process.returncode,
                            unanswered_ids=sorted(self.pending))
        return self.process.returncode

    def dispose(self):
        self.evidence.event("stderr_digest", bytes=self.stderr_bytes,
                            sha256=self.stderr_hash.hexdigest())
        stop_owned(self.process)
        self.selector.close()
