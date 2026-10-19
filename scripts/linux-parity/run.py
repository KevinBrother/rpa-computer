#!/usr/bin/env python3
"""CC-only runner inside a dedicated Linux Xvfb environment. No mock backend."""
import argparse
import json
import os
from pathlib import Path
import signal
import sys
import time

from authority import authority
from cancellation import cases
from common import Deadline, Evidence, Failure, NotImplementedCase, write_json
from context import Context
from scenarios import SCENARIOS
from process_ledger import VARIABLE, reap

SCENARIOS.update(cases())
SCENARIOS["authority"] = authority
CORE = ["smoke", "native", "editing", "multiline", "drag", "scroll", "hold", "displays", "lifecycle"]
DEFERRED = {
    "full-blackbox": "Tk ground truth is explicitly whitebox; OS-isolated image-only acceptance remains separate",
    "full-xkb": "core keysyms/modifiers are compared, not full XKB actions/types/groups/controls",
    "multimonitor": "negative origins, gaps, mixed scales, rotation/hotplug require controlled RandR fixture",
    "authority-revoke": "cross-UID ownership theft/revocation, late init quarantine and adversarial races not covered",
    "clipboard-faults": "selection loss, external focus change, INCR/large data, close lifetime need separate cases",
    "feedback": "Linux capture-excluded feedback and global emergency-stop UX need implementation and independent evidence",
    "remote-tls": "remote TLS ownership/cancellation not exercised by stdio harness",
    "windows-macos": "regressions and builds outside this Linux test-code scope",
}


def positive(value):
    v = float(value)
    if not 0 < v <= 3600:
        raise argparse.ArgumentTypeError("must be >0 and <=3600")
    return v


def arguments():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--host", required=True, help="absolute path to real Linux computer-host binary")
    p.add_argument("--out", required=True, type=Path, help="new private artifact directory (must not exist)")
    p.add_argument("--scenarios", default="smoke", help="comma-separated names, core, or all (includes clipboard)")
    p.add_argument("--dedicated-xvfb", action="store_true", help="CC attests this is a dedicated disposable Xvfb")
    p.add_argument("--overall", type=positive, default=600, help="wall budget including reserved cleanup")
    p.add_argument("--scenario-timeout", type=positive, default=90)
    p.add_argument("--request-timeout", type=positive, default=20)
    p.add_argument("--fixture-tracebacks", action="store_true",
                   help="diagnostic-only periodic Python stack file for the owned fixture; no Tk updates")
    p.add_argument("--authority-display", help="same-server alternate screen, only in controlled authority wrapper")
    p.add_argument("--list", action="store_true", help="list names only; still requires host/out args")
    return p.parse_args()


def main():
    args = arguments()
    if args.list:
        print("\n".join(SCENARIOS))
        return 0
    if sys.platform != "linux" or not args.dedicated_xvfb or not os.environ.get("DISPLAY"):
        raise SystemExit("requires Linux DISPLAY and explicit --dedicated-xvfb; no desktop auto-discovery")
    if not Path(args.host).is_absolute() or not os.access(args.host, os.X_OK):
        raise SystemExit("--host must be an executable absolute path")
    if args.overall <= 25:
        raise SystemExit("--overall must exceed 25 seconds (20 reserved for cleanup)")
    chosen = []
    for name in args.scenarios.split(","):
        names = CORE if name == "core" else list(SCENARIOS) if name == "all" else [name]
        for candidate in names:
            if candidate not in SCENARIOS:
                raise SystemExit("unknown scenario: " + candidate)
            if candidate not in chosen:
                chosen.append(candidate)
    os.umask(0o077)
    root = Evidence(args.out.resolve())
    ledger = root.directory / "owned-processes.jsonl"
    old_ledger = os.environ.get(VARIABLE)
    os.environ[VARIABLE] = str(ledger)
    overall = Deadline(args.overall)
    results = []
    summary = {"schema": "linux-parity-evidence-v1", "oracle": "owned Tk whitebox",
               "acceptance_claim": False, "fixture_tracebacks": args.fixture_tracebacks, "host": args.host, "selected": chosen,
               "results": results, "unimplemented_scenarios": DEFERRED,
               "not_selected": [n for n in SCENARIOS if n not in chosen]}

    aborted = [False]

    def interrupted(signum, frame):
        aborted[0] = True
        raise Failure("runner interrupted/deadline signal " + str(signum))

    previous = {sig: signal.signal(sig, interrupted) for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGALRM)}
    signal.setitimer(signal.ITIMER_REAL, args.overall)
    try:
        for name in chosen:
            if aborted[0] or overall.end - time.monotonic() <= 21:
                results.append({"scenario": name, "status": "NOT_RUN", "reason": "interrupted or overall cleanup reserve"})
                continue
            ev = Evidence(root.directory / name)
            deadline = Deadline(min(args.scenario_timeout, overall.left() - 20))
            ctx = Context(args, ev, deadline)
            outcome = {"scenario": name, "status": "FAIL", "assertions": 0}
            started = time.monotonic()
            try:
                ctx.start()
                SCENARIOS[name](ctx)
                # Screenshot after the fixture's receipt assertion, not just dispatch.
                if ctx.computer.sid and not ctx.rpc.eof:
                    ctx.computer.observe()
                outcome["status"] = "PASS"
            except NotImplementedCase as exc:
                outcome.update(status="NOT_IMPLEMENTED", reason=str(exc))
            except Exception as exc:
                # No tracebacks/host output on stdout. Detailed observations are
                # already in files, and unknown/partial responses are preserved.
                outcome.update(status="FAIL", reason=str(exc), error_type=type(exc).__name__)
            finally:
                cleanup = Deadline(max(0.1, min(18, overall.end - time.monotonic())))
                try:
                    ctx.close_resources(cleanup)
                except Exception as exc:
                    ctx.cleanups.append({"phase": "cleanup_interrupted", "error": type(exc).__name__, "detail": str(exc)})
                    # Immediate process-only fallback, no GUI operations.
                    from common import stop_owned
                    if ctx.rpc:
                        stop_owned(ctx.rpc.process, grace=0.1)
                    stop_owned(ctx.fixture, grace=0.1)
                if ctx.cleanups:
                    outcome["pre_cleanup_status"] = outcome["status"]
                    outcome["status"] = "FAIL"
                    outcome["cleanup_errors"] = ctx.cleanups
                outcome["assertions"] = ctx.assertions
                outcome["elapsed_s"] = round(time.monotonic() - started, 3)
                ev.json("result.json", outcome)
                results.append(outcome)
                root.json("summary.json", summary)
                print(f"{name}: {outcome['status']}", flush=True)
            if ctx.cleanups:
                # Never start another input case after uncertain cleanup.
                for later in chosen[chosen.index(name) + 1:]:
                    results.append({"scenario": later, "status": "NOT_RUN", "reason": "prior cleanup uncertain"})
                break
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        reaped = reap(ledger)
        summary["emergency_reaped_pids"] = reaped
        if reaped:
            results.append({"scenario": "runner_teardown", "status": "FAIL",
                            "reason": "registered processes required emergency termination"})
        if old_ledger is None:
            os.environ.pop(VARIABLE, None)
        else:
            os.environ[VARIABLE] = old_ledger
        for sig, handler in previous.items():
            signal.signal(sig, handler)
        root.json("summary.json", summary)
    print("Evidence: " + str(root.directory / "summary.json"))
    if any(r["status"] == "FAIL" for r in results):
        return 1
    if len(results) != len(chosen) or any(r["status"] != "PASS" for r in results):
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
