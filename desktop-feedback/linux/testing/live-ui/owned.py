"""Owned processes/display only; no inherited desktop or global configuration."""
import hashlib
import json
import os
from pathlib import Path
import resource
import secrets
import shutil
import signal
import stat
import subprocess
import time
from rules import display_number
from wire import Failure, require

class Deadline:
    def __init__(self, seconds): self.end = time.monotonic() + seconds
    def check(self): require(time.monotonic() < self.end, 'deadline exceeded')
    def remaining(self):
        remaining = self.end - time.monotonic()
        require(remaining > 0, 'deadline exceeded')
        return remaining

class Evidence:
    def __init__(self, directory):
        self.directory = directory
        self.used = 0
        self.file = (directory / 'trace.jsonl').open('xb')
    def record(self, kind, value):
        data = json.dumps(dict(t=time.monotonic(), kind=kind, value=value), separators=(',', ':'), allow_nan=False).encode() + b'\n'
        self.used += len(data)
        require(self.used <= 512*1024, 'evidence budget exceeded')
        self.file.write(data)
        self.file.flush()
    def close(self): self.file.close()

def limits():
    # Child-local only; no global settings. Xvfb stderr and screenshots are bounded.
    resource.setrlimit(resource.RLIMIT_FSIZE, (8*1024*1024, 8*1024*1024))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

class Owned:
    def __init__(self): self.children = []
    def spawn(self, argv, **kwargs):
        child = subprocess.Popen(argv, start_new_session=True, close_fds=True, preexec_fn=limits, **kwargs)
        self.children.append(child)
        return child
    def stop(self, child):
        if child.poll() is None:
            try: os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError: pass
            try: child.wait(timeout=0.5)
            except subprocess.TimeoutExpired:
                try: os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError: pass
                child.wait(timeout=1)
        else: child.wait(timeout=0.1)
    def close(self):
        errors = []
        for child in reversed(self.children):
            try: self.stop(child)
            except Exception as exc: errors.append(type(exc).__name__)
        require(not errors, 'owned child cleanup/reap failed: '+','.join(errors))

def sha256_file(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as source:
        for chunk in iter(lambda: source.read(65536), b''):
            digest.update(chunk)
    return digest.hexdigest()

def binary(path, expected):
    path = Path(path).resolve(strict=True)
    require(path.is_file() and os.access(path, os.X_OK), 'executable missing')
    with path.open('rb') as source:
        require(source.read(4) == b'\x7fELF', 'Linux ELF required')
    digest = sha256_file(path)
    require(digest == expected.lower(), 'executable SHA256 mismatch')
    return str(path)

class PrivateX:
    def __init__(self, directory, number, owner):
        display = display_number(number)
        lock = Path(f'/tmp/.X{number}-lock')
        socket = Path(f'/tmp/.X11-unix/X{number}')
        require(not os.path.lexists(lock) and not os.path.lexists(socket), 'display occupied; never remove foreign locks')
        xvfb, xauth = shutil.which('Xvfb'), shutil.which('xauth')
        require(xvfb and xauth, 'dependency blocker: Xvfb/xauth missing; no install attempted')
        auth = directory / 'Xauthority'
        auth.touch(mode=0o600, exist_ok=False)
        self.env = dict(PATH=os.environ.get('PATH','/usr/bin:/bin'), LANG='C.UTF-8', LC_ALL='C.UTF-8',
                        HOME=str(directory), DISPLAY=display, XAUTHORITY=str(auth),
                        XDG_SESSION_TYPE='x11', RPA_FIXTURE_OWNED_DISPLAY=display)
        with (directory/'xauth.stderr').open('xb') as err:
            child=owner.spawn([xauth,'-f',str(auth),'add',display,'MIT-MAGIC-COOKIE-1',secrets.token_hex(16)],
                              env=self.env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=err)
            require(child.wait(timeout=2)==0, 'Xauthority creation failed')
        require(stat.S_IMODE(auth.stat().st_mode)==0o600, 'private auth permissions')
        self.stderr=(directory/'xvfb.stderr').open('xb')
        try:
            self.child=owner.spawn([xvfb,display,'-screen','0','1920x1080x24','-nolisten','tcp','-auth',str(auth)],
                                   env=self.env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=self.stderr)
            deadline=Deadline(3)
            while not (lock.exists() and socket.exists()):
                deadline.check()
                require(self.child.poll() is None, 'Xvfb failed; inspect owned stderr')
                time.sleep(0.02)
            require(int(lock.read_text().strip())==self.child.pid, 'X server lock is not owned PID')
            self.lock, self.socket = lock, socket
        except BaseException:
            self.stderr.close()
            raise
    def assert_alive(self): require(self.child.poll() is None, 'owned Xvfb died')
