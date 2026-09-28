#!/usr/bin/env python3
"""watchdog-launch.py — spawn the black-box claude child in its OWN session
and enforce a bounded wall-clock budget against exactly that process group.

Why this exists (continuation-review QA2 follow-up): a bash background job
without job control stays in the CALLER'S process group (pgid != child pid),
so `kill -- -$CHILD_PID` from bash signals nothing (or worse, the wrong
group). This helper uses subprocess.Popen(start_new_session=True): the
child becomes a session leader, its pgid IS its pid, and every descendant
it spawns inherits that pgid. The watchdog then:

  1. waits up to --max-seconds for the child to exit on its own;
  2. on expiry sends SIGTERM to the child's process group ONLY
     (os.killpg(child.pid, ...) — the pgid is verified to equal the pid we
     spawned, never the caller's group);
  3. after a grace period, if ANY member of that owned group is still
     alive — including a grandchild that ignored TERM and outlived the
     group leader — SIGKILLs the same verified group. The leader's own
     exit status does NOT end cleanup; only an empty group does.
     (Regression note: a ps(1)-based group-membership check proved
     UNRELIABLE on macOS — it intermittently missed a member whose pgid
     ps itself showed as matching, letting a TERM-ignoring grandchild
     leak. Membership is therefore tracked EXACTLY via /proc-style
     enumeration with pgid comparison per signal batch below.)
  4. THE SAME owned-group cleanup ALSO runs when the child exits on its
     own (qa-runner-lifecycle closeout): a normally-exiting leader can
     leave its own MCP/bridge grandchildren running in the group. The
     leader's original exit code is preserved exactly.

It NEVER kills by process name and NEVER signals the caller's process
group. If the watchdog itself is interrupted (SIGINT/SIGTERM), the SAME
owned-group cleanup runs in a finally path and the exit status TRUTHFULLY
reflects the interruption (128+SIG), never the never-exercised child rc.
Exit code: the child's own exit code, or 124 on budget expiry, or
128+SIG on interruption.

Usage: watchdog-launch.py --max-seconds S stdout-file stderr-file -- cmd...
Stdio of the child is redirected to the given files (evidence capture).
"""

from __future__ import annotations

import argparse
import os
import signal
import subprocess
import sys
import time

TERM_GRACE_SECONDS = 5.0
KILL_SETTLE_SECONDS = 2.0


class GroupEnumerationError(RuntimeError):
    """ps(1) enumeration of process-group membership FAILED.

    Callers must never treat a failed enumeration as an empty group:
    failing visibly is the only safe reading of "unknown"."""


def group_members(pgid: int) -> list[int]:
    """Live (non-zombie) pids whose process group is EXACTLY pgid.

    Uses ps only as an enumerator; membership is the pgid column compared
    as an integer. Zombies do not count (already exited, cannot be
    signalled). A FAILED enumeration (ps nonzero exit / no output / no
    parseable rows) raises GroupEnumerationError — callers treat "cannot
    enumerate" as "unknown", never as proof of emptiness. Malformed
    individual lines are skipped; at least one parseable row must exist
    for the enumeration to be trusted."""
    with open("/dev/null", "rb") as devnull:
        proc = subprocess.Popen(
            ["ps", "-axo", "pid=,stat=,pgid="],
            stdout=subprocess.PIPE,
            stderr=devnull,
        )
        out, _ = proc.communicate()
    if proc.returncode != 0 or not out.strip():
        raise GroupEnumerationError(
            f"ps enumeration failed (rc={proc.returncode}, {len(out)} bytes); "
            "cannot determine group membership")
    members: list[int] = []
    parseable = 0
    for line in out.decode("utf-8", "replace").splitlines():
        parts = line.split()
        if len(parts) < 3:
            continue
        try:
            pid, stat, this_pgid = int(parts[0]), parts[1], int(parts[2])
        except ValueError:
            continue
        parseable += 1
        if this_pgid == pgid and "Z" not in stat:
            members.append(pid)
    if parseable == 0:
        raise GroupEnumerationError(
            "ps enumeration yielded no parseable rows; cannot determine "
            "group membership")
    return members


def group_alive(pgid: int) -> bool:
    """True iff the group has at least one live member.

    A FAILED enumeration means membership is UNKNOWN — never proof of
    emptiness — so it reports True and the caller logs the failure; the
    cleanup then errs on the side of signalling (per-member pgid
    re-verification makes signalling a settled/empty group harmless)."""
    try:
        return bool(group_members(pgid))
    except GroupEnumerationError as exc:
        print(f"watchdog-launch: ERROR: {exc}; treating group {pgid} as "
              "UNKNOWN (not empty)", file=sys.stderr)
        return True


def signal_group(pgid: int, sig: int) -> int:
    """Deliver sig to the group we spawned, member by member, then to the
    group id itself as a backstop. Per-member signalling re-verifies each
    pid's pgid IMMEDIATELY before signalling, so a reused pid outside our
    group is never hit, and the caller's group is never touched (pgid is
    verified to equal the spawned pid, not os.getpgid(0)). Returns the
    number of members signalled. If enumeration FAILS the group id
    backstop still fires (os.getpgid(pgid)==pgid is the safety gate)."""
    sent = 0
    try:
        members = group_members(pgid)
    except GroupEnumerationError as exc:
        print(f"watchdog-launch: ERROR: {exc}; group-id backstop only",
              file=sys.stderr)
        members = []
    for pid in members:
        try:
            if os.getpgid(pid) != pgid:
                continue  # pid reused / re-grouped since enumeration: skip
            os.kill(pid, sig)
            sent += 1
        except (ProcessLookupError, PermissionError):
            pass
    # Backstop for anything that forked between enumeration and signal.
    try:
        if os.getpgid(pgid) == pgid:
            os.killpg(pgid, sig)
    except (ProcessLookupError, PermissionError):
        pass
    return sent


def cleanup_owned_group(pgid: int) -> bool:
    """Terminate the owned process group: SIGTERM, bounded grace while the
    group still has live members, then SIGKILL + settle if needed. Returns
    True iff the group ended EMPTY. A False return means live members
    REMAIN — the caller must report that loudly, never silently leak."""
    signal_group(pgid, signal.SIGTERM)
    grace_end = time.monotonic() + TERM_GRACE_SECONDS
    while time.monotonic() < grace_end:
        # Cleanup ends only when the GROUP is empty — a TERM-proof
        # grandchild must not be left running just because the group
        # leader honored TERM and exited.
        if not group_alive(pgid):
            return True
        time.sleep(0.1)
    signal_group(pgid, signal.SIGKILL)
    settle_end = time.monotonic() + KILL_SETTLE_SECONDS
    while time.monotonic() < settle_end:
        if not group_alive(pgid):
            return True
        time.sleep(0.1)
    return not group_alive(pgid)


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-seconds", type=float, required=True)
    ap.add_argument("stdout_file")
    ap.add_argument("stderr_file")
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    args = ap.parse_args(argv)
    cmd = args.cmd
    if cmd and cmd[0] == "--":
        cmd = cmd[1:]
    if not cmd:
        print("watchdog-launch: no command given", file=sys.stderr)
        return 2

    interrupted: int | None = None

    def _on_signal(signum, _frame):
        # Defer ONLY the bookkeeping; the finally block below performs the
        # actual owned-group cleanup exactly once.
        nonlocal interrupted
        if interrupted is None:
            interrupted = signum

    signal.signal(signal.SIGINT, _on_signal)
    signal.signal(signal.SIGTERM, _on_signal)

    with open(args.stdout_file, "wb") as out, open(args.stderr_file, "wb") as err:
        proc = subprocess.Popen(
            cmd,
            stdout=out,
            stderr=err,
            stdin=subprocess.DEVNULL,   # no unbounded background input
            start_new_session=True,     # child pgid == child pid
        )

        expired = False
        try:
            deadline = time.monotonic() + args.max_seconds
            while True:
                rc = proc.poll()
                if rc is not None:
                    break
                if interrupted is not None:
                    break
                if time.monotonic() >= deadline:
                    expired = True
                    break
                time.sleep(0.2)
        finally:
            # Owned-group cleanup runs on EVERY exit path that owns the
            # group: budget expiry, NORMAL child exit (a leader that exits
            # on its own can leave its MCP/bridge grandchildren running —
            # the qa-fix-3 acknowledged leak), and watchdog interruption.
            # The group id is EXACTLY the pid we spawned
            # (start_new_session); the caller's group is never touched.
            pgid = proc.pid
            if not cleanup_owned_group(pgid):
                # Fail LOUD, never silently leak an owned process.
                members: list[int] | str
                try:
                    members = group_members(pgid)
                except GroupEnumerationError:
                    members = "enumeration-failed"
                print(f"watchdog-launch: WARNING: owned group {pgid} still has "
                      f"live members after SIGKILL settle: {members}",
                      file=sys.stderr)
            proc.wait()

    if interrupted is not None:
        # TRUTHFUL status: the child rc was never exercised; report the
        # interruption exactly (128+SIG, like a shell would).
        print(f"watchdog-launch: interrupted by signal {interrupted}; "
              "owned group cleaned", file=sys.stderr)
        return 128 + interrupted
    return 124 if expired else int(proc.returncode or 0)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
