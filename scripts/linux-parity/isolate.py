#!/usr/bin/env python3
"""CC-only controlled environment wrapper. Own a NEW Xvfb, never change :99/user X.

Child-only DISPLAY/XAUTHORITY/TMPDIR/XDG_RUNTIME_DIR and Xft.dpi overrides make
restoration removal of owned resources, not a best-effort edit of user settings.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from common import Deadline, Evidence, Failure, require, spawn, stop_owned
from process_ledger import reap

HERE = Path(__file__).resolve().parent


def bounded(argv, env, deadline, input_bytes=None):
    proc = spawn(argv, stdin=subprocess.PIPE if input_bytes is not None else subprocess.DEVNULL,
                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env)
    try:
        proc.communicate(input=input_bytes, timeout=deadline.left(6))
        require(proc.returncode == 0, "controlled environment helper failed: " + Path(argv[0]).name)
    finally:
        stop_owned(proc, grace=0.1)


def one(args, parent, dpi, name):
    ev = Evidence(parent / name)
    budget = Deadline(args.overall + 25)
    server = child = None
    temp = None
    read_fd = write_fd = None
    report = {"mode": name, "dpi": dpi, "status": "FAIL", "private_server": True,
              "restore_strategy": "child-only env and disposable X server; no existing desktop mutations"}
    try:
        temp = tempfile.mkdtemp(prefix="linux-parity-xvfb-")
        work = Path(temp)
        runtime = work / "runtime"
        runtime.mkdir(mode=0o700)
        auth = work / "Xauthority"
        auth.touch(mode=0o600)
        env = os.environ.copy()
        env.update(XAUTHORITY=str(auth), XDG_RUNTIME_DIR=str(runtime), TMPDIR=str(work),
                   XDG_SESSION_TYPE="x11", LANG="C.UTF-8", LC_ALL="C.UTF-8")
        env.pop("WAYLAND_DISPLAY", None)
        cookie = secrets.token_hex(16)
        # X server reads cookies, not client display-name matching. Seed a
        # private auth file before it starts; add chosen display for clients.
        bounded(["xauth", "-f", str(auth)], env, budget,
                f"add :0 MIT-MAGIC-COOKIE-1 {cookie}\n".encode())
        read_fd, write_fd = os.pipe()
        server = spawn(["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1280x720x24",
                        *(["-screen", "1", "1280x720x24"] if args.mode == "authority" else []),
                        "-nolisten", "tcp", "-auth", str(auth), "-dpi", str(dpi), "-noreset"],
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       env=env, pass_fds=(write_fd,))
        os.close(write_fd)
        write_fd = None
        os.set_blocking(read_fd, False)
        with selectors.DefaultSelector() as selector:
            selector.register(read_fd, selectors.EVENT_READ)
            end, data = time.monotonic() + budget.left(10), bytearray()
            while b"\n" not in data:
                remaining = min(budget.left(), end - time.monotonic())
                require(remaining > 0 and server.poll() is None, "Xvfb readiness deadline/exited")
                for _, _ in selector.select(remaining):
                    block = os.read(read_fd, 128)
                    require(block, "Xvfb displayfd EOF")
                    data.extend(block)
                    require(len(data) <= 32, "invalid displayfd output")
            display = data.decode("ascii").strip()
            require(display.isdigit(), "invalid display number")
        os.close(read_fd)
        read_fd = None
        env["DISPLAY"] = ":" + display + ".0"
        bounded(["xauth", "-f", str(auth)], env, budget,
                f"add :{display} MIT-MAGIC-COOKIE-1 {cookie}\n".encode())
        # Only our NEW server is mutated; no user X resources are read/copied.
        bounded(["xrdb", "-nocpp", "-merge"], env, budget, f"Xft.dpi: {dpi}\n".encode())
        report.update(display=env["DISPLAY"], root_pixels=[1280, 720], xft_dpi=dpi)
        scenario = args.scenarios or ("authority" if args.mode == "authority" else
                                      "displays,smoke" if args.mode == "hidpi" else "smoke")
        command = [sys.executable, str(HERE / "run.py"), "--host", args.host,
                   "--out", str(ev.directory / "cases"), "--dedicated-xvfb",
                   "--scenarios", scenario, "--overall", str(args.overall)]
        if args.mode == "authority":
            command += ["--authority-display", ":" + display + ".1"]
        if args.fixture_tracebacks:
            command.append("--fixture-tracebacks")
        child = spawn(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                      stderr=subprocess.DEVNULL, env=env)
        try:
            child.wait(timeout=budget.left())
        except subprocess.TimeoutExpired:
            raise Failure("child runner exceeded wrapper deadline")
        report["runner_exit"] = child.returncode
        require(server.poll() is None, "owned Xvfb exited during scenario")
        result_path = ev.directory / "cases" / "summary.json"
        if result_path.exists():
            report["cases"] = json.loads(result_path.read_text())["results"]
        require(child.returncode in (0, 1, 2), "runner did not return a structured outcome")
        report["status"] = {0: "PASS", 1: "FAIL", 2: "NOT_IMPLEMENTED"}[child.returncode]
        # Stop while private auth/runtime files still exist, then remove temp.
        stop_owned(child, grace=0.1)
        child = None
        stop_owned(server)
        server = None
    except Exception as exc:
        report.update(status="FAIL", reason=str(exc), error_type=type(exc).__name__)
    finally:
        stop_owned(child, grace=20)
        reaped = reap(ev.directory / "cases" / "owned-processes.jsonl")
        report["emergency_reaped_pids"] = reaped
        if reaped:
            report["status"] = "FAIL"
        stop_owned(server)
        if temp is not None:
            shutil.rmtree(temp)
        for fd in (read_fd, write_fd):
            if fd is not None:
                os.close(fd)
        ev.json("environment.json", report)
    return report


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--host", required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--mode", choices=("native", "authority", "hidpi"), default="native")
    p.add_argument("--fixture-tracebacks", action="store_true",
                   help="passive Python stack diagnostics; forwarded to the owned fixture")
    p.add_argument("--scenarios", help="forwarded to runner; independent scenario selection")
    p.add_argument("--overall", type=int, default=600, help="budget per owned environment")
    args = p.parse_args()
    require(sys.platform == "linux", "wrapper only runs on Linux")
    require(25 < args.overall <= 3600, "overall budget outside bounds")
    require(Path(args.host).is_absolute() and os.access(args.host, os.X_OK), "invalid host path")
    os.umask(0o077)
    root = Evidence(args.out.resolve())
    missing = [name for name in ("Xvfb", "xauth", "xrdb") if not shutil.which(name)]
    if missing:
        root.json("summary.json", {"status": "NOT_IMPLEMENTED", "missing_environment_tools": missing})
        print("NOT_IMPLEMENTED: wrapper environment dependencies; see summary.json")
        return 2

    def interrupted(signum, frame):
        raise Failure("wrapper interrupted")

    handlers = {sig: signal.signal(sig, interrupted) for sig in (signal.SIGINT, signal.SIGTERM)}
    reports = []
    try:
        for dpi in ((96, 192) if args.mode == "hidpi" else (96,)):
            reports.append(one(args, root.directory, dpi, args.mode + "-" + str(dpi)))
            if reports[-1]["status"] == "FAIL":
                break
        root.json("summary.json", {"mode": args.mode, "environments": reports,
                                   "acceptance_claim": False, "same_physical_root_across_dpi": args.mode == "hidpi"})
    finally:
        for sig, handler in handlers.items():
            signal.signal(sig, handler)
    print("Evidence: " + str(root.directory / "summary.json"))
    return 1 if any(r["status"] == "FAIL" for r in reports) else 2 if any(r["status"] != "PASS" for r in reports) else 0


if __name__ == "__main__":
    sys.exit(main())
