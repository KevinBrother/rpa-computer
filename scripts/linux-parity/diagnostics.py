"""Passive fixture-failure evidence. Never updates/retries/accepts fixture state.

Reads only the rejected file and explicitly owned PID identities. No GUI APIs,
process-name search, signals, ptrace, environments, cmdlines or other desktops.
"""
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import time

from common import Failure, require, write_json

HEARTBEAT_LIMIT_S = 1.0  # Original gate; diagnostics MUST NOT relax it.
MAX_STATE_BYTES = 4 * 1024 * 1024
MAX_PROC_BYTES = 16 * 1024
MAX_THREADS = 16
STATUS_FIELDS = {"State", "Pid", "PPid", "Threads", "VmRSS", "VmSize", "SigPnd", "ShdPnd",
                 "SigBlk", "SigIgn", "SigCgt", "voluntary_ctxt_switches", "nonvoluntary_ctxt_switches"}
IO_FIELDS = {"rchar", "wchar", "syscr", "syscw", "read_bytes", "write_bytes", "cancelled_write_bytes"}


@dataclass
class FixtureSample:
    raw: bytes
    state: dict
    read: dict


def read_fixture_sample(path, expected_pid):
    started = time.monotonic()
    with Path(path).open("rb") as handle:
        stat = os.fstat(handle.fileno())
        raw = handle.read(MAX_STATE_BYTES + 1)
    require(len(raw) <= MAX_STATE_BYTES, "fixture state exceeds diagnostic bound")
    state = json.loads(raw)
    require(isinstance(state, dict), "fixture state is not an object")
    require(state.get("pid") == expected_pid, "fixture state PID mismatch")
    require(state.get("schema") == "linux-parity-fixture-v1", "fixture schema mismatch")
    finished = time.monotonic()
    read = {"read_started_monotonic": started, "read_finished_monotonic": finished,
            "read_duration_s": finished - started, "state_monotonic": state["monotonic"],
            "age_s": finished - state["monotonic"], "threshold_s": HEARTBEAT_LIMIT_S,
            "revision": state.get("revision"), "ready": state.get("ready"), "seq": state.get("seq"),
            "pid": expected_pid, "file": {"size": stat.st_size, "inode": stat.st_ino,
                                         "mtime_ns": stat.st_mtime_ns}}
    return FixtureSample(raw=raw, state=state, read=read)


def read_bounded(path):
    with Path(path).open("rb") as handle:
        raw = handle.read(MAX_PROC_BYTES + 1)
    require(len(raw) <= MAX_PROC_BYTES, "proc diagnostic file exceeds bound")
    return raw.decode("utf-8", errors="replace")


def parse_stat(text):
    # Ignore comm, which can include spaces and ')'. Never preserve command args.
    pid = int(text.split(" ", 1)[0])
    fields = text.rsplit(")", 1)[1].split()
    return {"pid": pid, "state": fields[0], "ppid": int(fields[1]), "pgrp": int(fields[2]),
            "utime_ticks": int(fields[11]), "stime_ticks": int(fields[12]),
            "threads": int(fields[17]), "start_ticks": int(fields[19]),
            "vsize_bytes": int(fields[20]), "rss_pages": int(fields[21])}


def matches(stat, expected):
    return all(stat[key] == expected[key] for key in ("pid", "pgrp", "start_ticks"))


def process_snapshot(expected, proc_root=Path("/proc")):
    """Best effort, strictly identity-authorized; unavailable is NOT proof of idle."""
    if expected is None:
        return {"status": "not_authorized", "note": "no recorded owned process identity; no proc reads"}
    directory = Path(proc_root) / str(expected["pid"])
    try:
        before = parse_stat(read_bounded(directory / "stat"))
    except (OSError, ValueError, IndexError, KeyError, Failure) as exc:
        return {"status": "unavailable", "expected": expected, "error_type": type(exc).__name__}
    if not matches(before, expected):
        return {"status": "identity_mismatch", "expected": expected}
    result = {"status": "collected", "stat": before, "files": {}, "threads": [],
              "note": "read-only process diagnostics; not proof of app responsiveness or input cleanup"}
    for name in ("status", "wchan", "schedstat", "io"):
        try:
            text = read_bounded(directory / name)
            if name in ("status", "io"):
                allowed = STATUS_FIELDS if name == "status" else IO_FIELDS
                result["files"][name] = {k: v.strip() for line in text.splitlines() if ":" in line
                                         for k, v in [line.split(":", 1)] if k in allowed}
            else:
                result["files"][name] = {"text": text}
        except (OSError, ValueError, Failure) as exc:
            result["files"][name] = {"error_type": type(exc).__name__}
    try:
        # Stop enumeration at the cap; only this already-authorized process's tasks.
        count = 0
        with os.scandir(directory / "task") as entries:
            for entry in entries:
                if not entry.name.isdigit():
                    continue
                if count >= MAX_THREADS:
                    result["threads_truncated"] = True
                    break
                count += 1
                task = {"tid": int(entry.name)}
                try:
                    task["stat"] = parse_stat(read_bounded(Path(entry.path) / "stat"))
                    task["wchan"] = read_bounded(Path(entry.path) / "wchan").strip()
                except (OSError, ValueError, IndexError, KeyError, Failure) as exc:
                    task["error_type"] = type(exc).__name__
                result["threads"].append(task)
    except OSError as exc:
        result["threads_error_type"] = type(exc).__name__
    try:
        after = parse_stat(read_bounded(directory / "stat"))
    except (OSError, ValueError, IndexError, KeyError, Failure) as exc:
        result["identity_after"] = {"status": "unavailable", "error_type": type(exc).__name__}
    else:
        if not matches(after, expected):
            return {"status": "identity_changed_during_read", "expected": expected}
        result["identity_after"] = "matched"
    return result


def save_raw(directory, name, raw):
    # Do not overwrite a prior rejection, even if cleanup reports the same state.
    with (directory / name).open("xb") as handle:
        handle.write(raw)
    return {"file": name, "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def capture_rejection(evidence, sample, serial, reason, last_fresh, phase, wait_label, owned, transport):
    stem = f"fixture-rejected-{serial:04d}"
    report = {"schema": "linux-parity-fixture-diagnostic-v1", "reason": reason,
              "acceptance": "REJECTED; diagnostics never satisfy fixture assertions",
              "phase": phase, "wait_label": wait_label, "read": sample.read,
              "raw": save_raw(evidence.directory, stem + ".raw", sample.raw),
              "last_fresh_read": last_fresh.read if last_fresh else None,
              "transport": transport, "owned_processes": {}}
    if last_fresh:
        report["last_fresh_raw"] = save_raw(evidence.directory, stem + "-last-fresh.raw", last_fresh.raw)
    for role, expected in owned.items():
        report["owned_processes"][role] = process_snapshot(expected)
    evidence.json(stem + ".json", report)
    evidence.event("fixture_rejected", report=stem + ".json", reason=reason,
                   age_s=sample.read["age_s"], revision=sample.read["revision"],
                   phase=phase, wait_label=wait_label)
    return report


def save_shutdown(state_path, state):
    """Keep the final live heartbeat intact. Shutdown is a separate lifecycle fact."""
    write_json(Path(state_path).with_name("fixture-shutdown.json"), state)
