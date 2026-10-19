"""Bounded time, private evidence, and exclusively owned process cleanup."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time


class Failure(Exception):
    pass


class NotImplementedCase(Failure):
    pass


class Deadline:
    def __init__(self, seconds):
        self.end = time.monotonic() + seconds

    def left(self, cap=None):
        remaining = self.end - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("overall/scenario deadline exceeded")
        return min(remaining, cap) if cap is not None else remaining

    def sleep(self, seconds):
        time.sleep(self.left(seconds))


def require(condition, message):
    if not condition:
        raise Failure(message)


def write_json(path, value):
    path = Path(path)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
    os.replace(tmp, path)


class Evidence:
    def __init__(self, directory):
        self.directory = Path(directory)
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=False)
        self.counter = 0
        self.started = time.monotonic()

    def event(self, kind, **fields):
        with (self.directory / "events.jsonl").open("a", encoding="utf-8") as out:
            out.write(json.dumps({"t": round(time.monotonic() - self.started, 6),
                                  "kind": kind, **fields}, ensure_ascii=True) + "\n")

    def json(self, name, value):
        write_json(self.directory / name, value)

    def blob(self, blob, extension="png"):
        self.counter += 1
        name = f"image-{self.counter:04d}.{extension}"
        (self.directory / name).write_bytes(blob)
        return {"file": name, "bytes": len(blob), "sha256": hashlib.sha256(blob).hexdigest()}


def spawn(argv, **kwargs):
    # No shell, no process-name lookup, no inherited stdin. The session/group is ours.
    process = subprocess.Popen(argv, start_new_session=True, **kwargs)
    try:
        from process_ledger import register
        register(process, kwargs.get("env"))
    except Exception:
        stop_owned(process, grace=0.1)
        raise
    return process


def stop_owned(process, grace=2.0):
    """Terminate the group created by spawn, including children of an exited leader."""
    if process is None:
        return
    pgid = process.pid
    try:
        os.killpg(pgid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=grace)
    except subprocess.TimeoutExpired:
        pass
    # Group belongs exclusively to this harness; do not rely on leader poll alone.
    try:
        os.killpg(pgid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=2)
    for stream in (process.stdin, process.stdout, process.stderr):
        if stream is not None and not stream.closed:
            stream.close()


def read_state(path, expected_pid=None):
    state = json.loads(Path(path).read_text(encoding="utf-8"))
    if expected_pid is not None:
        require(state.get("pid") == expected_pid, "fixture state PID mismatch")
    require(state.get("schema") == "linux-parity-fixture-v1", "fixture schema mismatch")
    return state
