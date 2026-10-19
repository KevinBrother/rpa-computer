"""Private ownership ledger for wrapper teardown after a runner is interrupted.

Only recorded process groups are addressed. Linux /proc start ticks prevent
signalling a recycled leader PID. This is not process-name discovery.
"""
import json
import os
from pathlib import Path
import signal
import time


VARIABLE = "LINUX_PARITY_PROCESS_LEDGER"


def identity(pid):
    try:
        # comm may contain spaces/parentheses. Fields following its final ')' begin at #3.
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return {"pid": pid, "pgrp": int(fields[2]), "start_ticks": int(fields[19])}
    except FileNotFoundError:
        return None


def register(process, environment=None):
    path = (environment if environment is not None else os.environ).get(VARIABLE)
    if not path:
        return
    value = identity(process.pid)
    if value is not None:
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(value) + "\n")


def reap(path):
    """Last resort for live registered leaders after the runner has been stopped."""
    path = Path(path)
    if not path.exists():
        return []
    records = [json.loads(line) for line in path.read_text().splitlines()]
    addressed = []
    for sig in (signal.SIGTERM, signal.SIGKILL):
        for record in records:
            current = identity(record["pid"])
            if current == record and record["pgrp"] == record["pid"]:
                try:
                    os.killpg(record["pid"], sig)
                    if record["pid"] not in addressed:
                        addressed.append(record["pid"])
                except ProcessLookupError:
                    pass
        if addressed and sig == signal.SIGTERM:
            time.sleep(0.2)
    return addressed
