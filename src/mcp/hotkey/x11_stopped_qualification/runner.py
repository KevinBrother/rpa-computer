#!/usr/bin/env python3
"""CC-only Stage 2 runner. Authoring this file does not authorize execution.

Launches ONLY its own disposable Xvfb and one compiled exact Rust test. All
signals use retained pidfds. Only X setup/auth probes, never input/map requests.
"""
import argparse
import json
import os
from pathlib import Path
import select
import secrets
import signal
import socket
import struct
import subprocess
import tempfile
import time

TEST = ("mcp::hotkey::x11_registration_tests::stopped_qualification::"
        "post_arm_stopped_server_disarm_releases_grabs_after_resume")


def parse_proc_stat(text):
    # comm can contain spaces, newlines and ')'. The suffix begins at field 3;
    # starttime is field 22 => suffix index 19, NOT awk $19 after stripping comm.
    if "(" not in text or ")" not in text:
        raise ValueError("invalid proc stat framing")
    fields = text.rsplit(")", 1)[1].split()
    if len(fields) < 20 or fields[0] not in ("R", "S", "D", "T", "t", "Z", "X", "x", "K", "W", "P", "I"):
        raise ValueError("invalid proc stat suffix")
    parent, ticks = int(fields[1]), int(fields[19])
    if parent <= 0 or ticks <= 0:
        raise ValueError("invalid owned child parent/start ticks")
    return fields[0], parent, ticks


def identity(pid):
    return parse_proc_stat(Path(f"/proc/{pid}/stat").read_text())


class Child:
    def __init__(self, process):
        self.process = process
        self.fd = None
        self.ticks = None

    def retain(self):
        # This single-threaded runner uses SIGCHLD=SIG_DFL, no SA_NOCLDWAIT,
        # and does not poll/wait/reap this child before acquiring its pidfd.
        # Even an immediately exited child retains its PID as our zombie.
        self.fd = os.pidfd_open(self.process.pid, 0)
        _, parent, self.ticks = identity(self.process.pid)
        if parent != os.getpid():
            raise RuntimeError("child ownership mismatch")

    def send(self, sig):
        if self.fd is None:
            raise RuntimeError("no retained pidfd: refusing bare-PID signal")
        try:
            signal.pidfd_send_signal(self.fd, sig, None, 0)
        except ProcessLookupError:
            pass  # Only this retained process can be targeted, never a reused PID.

    def state(self):
        if self.fd is None or select.select([self.fd], [], [], 0)[0]:
            raise RuntimeError("owned child exited")
        state, parent, ticks = identity(self.process.pid)
        if parent != os.getpid() or ticks != self.ticks:
            raise RuntimeError("retained child identity mismatch")
        return state

    def wait_state(self, stopped):
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            state = self.state()
            if (state == "T") == stopped and state not in ("Z", "X", "x", "t"):
                return
            time.sleep(.01)
        raise TimeoutError("owned server state transition deadline")

    def cleanup(self, name, record):
        # Never replace an unavailable pidfd with kill(pid), nor wait forever.
        if self.process.returncode is None:
            if self.fd is None:
                record("cleanup_unknown", child=name, pid=self.process.pid,
                       reason="pidfd acquisition failed; private container disposal required")
                return False
            self.send(signal.SIGCONT)  # Also unblocks teardown on failure paths.
            self.send(signal.SIGTERM)
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.send(signal.SIGKILL)
                try:
                    self.process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    record("cleanup_unknown", child=name, pid=self.process.pid,
                           reason="owned child not reaped after bounded SIGKILL wait")
                    return False
        record("reaped", child=name, pid=self.process.pid, exit=self.process.returncode)
        return True

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None



def authority_record(hostname, number, cookie):
    if not number.isascii() or not number.isdecimal() or str(int(number)) != number:
        raise ValueError("authority requires canonical decimal display")
    if not hostname or len(cookie) != 16:
        raise ValueError("invalid local authority parameters")
    fields = [hostname, number.encode("ascii"), b"MIT-MAGIC-COOKIE-1", cookie]
    return struct.pack(">H", 256) + b"".join(struct.pack(">H", len(v)) + v for v in fields)


def write_private_authority(path, number):
    # Exclusive file in our 0700 directory; cookie never travels in argv/env/logs.
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, "wb") as file:
        os.fchmod(file.fileno(), 0o600)
        cookie = secrets.token_bytes(16)
        file.write(authority_record(os.fsencode(socket.gethostname()), number, cookie))
    return cookie


def require_owned_listener(child, endpoint):
    child.state()
    owned = set()
    for entry in Path(f"/proc/{child.process.pid}/fd").iterdir():
        try:
            target = os.readlink(entry)
        except FileNotFoundError:
            continue
        if target.startswith("socket:[") and target.endswith("]"):
            owned.add(target[8:-1])
    matches = []
    for line in Path("/proc/net/unix").read_text().splitlines()[1:]:
        fields = line.split()
        if len(fields) >= 8 and fields[7] == endpoint:
            # Only a listening Unix STREAM socket, not a similarly named client.
            matches.append(fields[3] == "00010000" and fields[4] == "0001" and fields[6] in owned)
    if not matches or not all(matches):
        raise RuntimeError("X endpoint is not exclusively an owned server listener")


def probe_auth(child, endpoint, protocol, cookie):
    # Each probe uses one absolute transport deadline, not a per-byte timeout.
    # This setup-only connection creates no resources or keyboard/map requests.
    deadline = time.monotonic() + 2
    child.state()
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as peer:
        def budget():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("X authentication probe deadline")
            peer.settimeout(remaining)

        def read_exact(size):
            data = bytearray()
            while len(data) < size:
                budget()
                part = peer.recv(size - len(data))
                if not part:
                    raise RuntimeError("truncated X setup reply")
                data.extend(part)
            return bytes(data)

        budget()
        peer.connect(endpoint)
        pid, uid, _gid = struct.unpack("3i", peer.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if (pid, uid) != (child.process.pid, os.getuid()):
            raise RuntimeError("X authentication peer is not retained owned server")
        child.state()  # Retained pidfd still live before sending credentials.
        request = (b"l\0" + struct.pack("<HHHHH", 11, 0, len(protocol), len(cookie), 0)
                   + protocol + b"\0" * (-len(protocol) % 4)
                   + cookie + b"\0" * (-len(cookie) % 4))
        budget()
        peer.sendall(request)
        header = read_exact(8)
        status, _reason, major, minor, units = struct.unpack("<BBHHH", header)
        size = units * 4
        if status not in (0, 1) or size > 65536:
            raise RuntimeError("unexpected/oversized X setup reply")
        read_exact(size)  # Discard opaque payload; never put auth failure text in logs.
        if status == 1 and (major, minor) != (11, 0):
            raise RuntimeError("unexpected X protocol version")
        return status


def verify_private_auth(child, endpoint, cookie):
    require_owned_listener(child, endpoint)
    if probe_auth(child, endpoint, b"", b"") != 0:
        raise RuntimeError("unauthenticated X setup accepted; refusing non-private server")
    if probe_auth(child, endpoint, b"MIT-MAGIC-COOKIE-1", cookie) != 1:
        raise RuntimeError("owned private cookie X setup rejected")
    child.state()


def read_line(peer, overall, seconds=5):
    deadline = min(overall, time.monotonic() + seconds)
    data = bytearray()
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("private control phase deadline")
        peer.settimeout(remaining)
        part = peer.recv(1)
        if not part:
            raise RuntimeError("test control connection closed before verification")
        if part == b"\n":
            value = data.decode("ascii")
            if value == "ABORT":
                raise RuntimeError("test requested abort/resume")
            return value
        data += part
        if len(data) > 128 or not part.isascii():
            raise RuntimeError("invalid or oversized control frame")


def write_line(peer, value):
    peer.settimeout(2)
    peer.sendall((value + "\n").encode("ascii"))


def require(actual, expected):
    if actual != expected:
        raise RuntimeError(f"unexpected control frame: expected {expected!r}, got {actual!r}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--owned-disposable", action="store_true", required=True)
    parser.add_argument("--display", type=int, required=True)
    parser.add_argument("--test-binary", type=Path, required=True)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.display <= 65535:
        parser.error("use a fresh local display 1..65535; current product limit remains open")
    binary = args.test_binary.resolve(strict=True)
    with binary.open("rb") as file:
        if file.read(4) != b"\x7fELF" or not os.access(binary, os.X_OK):
            parser.error("a compiled executable Linux Rust test binary is required, not cargo/shell")
    if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
        parser.error("native Python/Linux pidfd support required; no fallback/profile changes")
    # Remove inherited SIGCHLD ignore/auto-reap behavior before creating children.
    signal.signal(signal.SIGCHLD, signal.SIG_DFL)
    test_fd = os.pidfd_open(os.getpid(), 0)
    os.close(test_fd)  # Preflight availability without launching any child.
    display = f":{args.display}"
    endpoint = f"/tmp/.X11-unix/X{args.display}"
    if os.path.lexists(endpoint) or os.path.lexists(f"/tmp/.X{args.display}-lock"):
        parser.error("display socket/lock already exists; never remove another owner's files")
    unix = Path("/proc/net/unix").read_text().splitlines()
    if any(line.split()[-1:] in ([endpoint], ["@" + endpoint]) for line in unix):
        parser.error("DISPLAY already has a Unix socket; select a new private fixture")
    args.evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    os.umask(0o077)
    server = test = None
    peer = None
    okay = cleanup_okay = False
    with (args.evidence_dir / "runner.jsonl").open("x") as log, \
            (args.evidence_dir / "xvfb.log").open("xb") as server_log, \
            (args.evidence_dir / "test.log").open("xb") as test_log, \
            tempfile.TemporaryDirectory(prefix="hotkey-stop-") as directory:
        def record(event, **fields):
            log.write(json.dumps(dict(event=event, monotonic=time.monotonic(), **fields)) + "\n")
            log.flush()

        interruptions = []

        def interrupted(signum, _frame):
            # Do not raise between Popen returning and retaining ownership, and
            # never signal from this handler. Bounded phase waits lead back to
            # normal cleanup; a hard outer kill remains explicitly unknown.
            interruptions.append(signum)

        def check_interrupt():
            if interruptions:
                raise RuntimeError(f"runner interrupted by signal {interruptions[0]}")

        old_term = signal.signal(signal.SIGTERM, interrupted)
        old_int = signal.signal(signal.SIGINT, interrupted)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            path = str(Path(directory) / "control.sock")
            listener.bind(path)
            os.chmod(path, 0o600)
            listener.listen(1)
            authority = Path(directory) / "authority"
            cookie = write_private_authority(authority, str(args.display))
            # Private file and setup authentication, never -ac or inherited auth.
            process = subprocess.Popen(["Xvfb", display, "-screen", "0", "800x600x24",
                                        "-noreset", "-nolisten", "tcp", "-auth", str(authority)],
                                       stdin=subprocess.DEVNULL, stdout=server_log,
                                       stderr=subprocess.STDOUT, close_fds=True)
            server = Child(process)
            server.retain()
            check_interrupt()
            record("server_owned", pid=process.pid, start_ticks=server.ticks, display=display)
            start_deadline = time.monotonic() + 5
            while not os.path.exists(endpoint):
                server.state()
                if time.monotonic() >= start_deadline:
                    raise TimeoutError("owned Xvfb socket startup deadline")
                time.sleep(.01)
            server.wait_state(False)
            verify_private_auth(server, endpoint, cookie)
            del cookie
            record("private_auth_verified", unauthenticated_rejected=True, authenticated_accepted=True)
            check_interrupt()
            env = os.environ.copy()
            env.pop("XAUTHLOCALHOSTNAME", None)  # Match our exact kernel hostname record.
            env.update(DISPLAY=display, XAUTHORITY=str(authority), XDG_SESSION_TYPE="x11", WAYLAND_DISPLAY="",
                       RPA_X11_HOTKEY_STOP_QUALIFY="owned-disposable-postarm-v1",
                       RPA_X11_STOP_RUNNER_PID=str(os.getpid()),
                       RPA_X11_STOP_SERVER_PID=str(server.process.pid),
                       RPA_X11_STOP_SERVER_START_TICKS=str(server.ticks),
                       RPA_X11_STOP_CONTROL_SOCKET=path)
            # Only the newly created authority file is used; no credentials logged.
            process = subprocess.Popen([str(binary), TEST, "--ignored", "--exact", "--nocapture", "--test-threads=1"],
                                       env=env, stdin=subprocess.DEVNULL, stdout=test_log,
                                       stderr=subprocess.STDOUT, close_fds=True)
            test = Child(process)
            test.retain()
            check_interrupt()
            overall = time.monotonic() + 30
            listener.settimeout(8)
            peer, _ = listener.accept()
            pid, uid, _gid = struct.unpack("3i", peer.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
            if (pid, uid) != (test.process.pid, os.getuid()):
                raise RuntimeError("control peer is not this runner's exact test child")
            write_line(peer, f"HELLO {server.process.pid} {server.ticks}")
            require(read_line(peer, overall, 8), "ARMED_PROVEN")
            check_interrupt()
            server.wait_state(False)
            record("healthy_arm_proven_received")
            server.send(signal.SIGSTOP)
            server.wait_state(True)
            record("server_stopped", pid=server.process.pid, start_ticks=server.ticks)
            write_line(peer, f"STOPPED {server.process.pid} {server.ticks}")
            result = read_line(peer, overall, 4)
            parts = result.split()
            if len(parts) != 2 or parts[0] != "DISARMED" or not parts[1].isdigit():
                raise RuntimeError("expected bounded DISARMED measurement")
            check_interrupt()
            micros = int(parts[1])
            if micros >= 2_000_000:
                raise RuntimeError("product disarm exceeded 2 seconds")
            server.wait_state(True)
            record("disarm_completed_while_stopped", microseconds=micros)
            server.send(signal.SIGCONT)
            server.wait_state(False)
            record("server_resumed", pid=server.process.pid, start_ticks=server.ticks)
            write_line(peer, f"RESUMED {server.process.pid} {server.ticks}")
            require(read_line(peer, overall), "VERIFIED")
            record("old_grabs_absent_and_baseline_verified")
            write_line(peer, "FINISH")
            code = test.process.wait(timeout=5)
            if code != 0:
                raise RuntimeError(f"exact Rust test exited {code}")
            check_interrupt()
            okay = True
        except Exception as error:
            record("qualification_failed", reason=str(error))
        finally:
            # Repeated outer interrupts must not skip owned cleanup attempts.
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            signal.signal(signal.SIGINT, signal.SIG_IGN)
            if peer is not None:
                peer.close()
            listener.close()
            cleanup_okay = True
            if server is not None and server.fd is not None:
                try:
                    server.send(signal.SIGCONT)
                    record("cleanup_resume_requested")
                except Exception as error:
                    cleanup_okay = False
                    record("cleanup_unknown", child="server", reason=str(error))
            for name, child in (("test", test), ("server", server)):
                if child is not None:
                    try:
                        cleanup_okay = child.cleanup(name, record) and cleanup_okay
                    except Exception as error:
                        cleanup_okay = False
                        record("cleanup_unknown", child=name, reason=str(error))
                    finally:
                        child.close()
            record("result", qualified=okay, cleanup_confirmed=cleanup_okay)
            signal.signal(signal.SIGTERM, old_term)
            signal.signal(signal.SIGINT, old_int)
    return 0 if okay and cleanup_okay else 1


if __name__ == "__main__":
    raise SystemExit(main())
