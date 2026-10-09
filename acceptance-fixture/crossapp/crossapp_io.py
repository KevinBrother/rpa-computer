"""Small rechecking IO boundary; no recursive removal, process or network support.

Requires a private, non-concurrently-mutated root and parent. No claim that
Python pathname opens are atomic against hostile Windows ancestor swaps.
"""
import os
from pathlib import Path
import stat

from crossapp_paths import _absolute, _inspect, _safe_stat, resolve_owned_path


class TooLarge(ValueError):
    pass


def _identity(s):
    return s.st_dev, s.st_ino


class OwnedIO:
    def __init__(self, root):
        self.root = _absolute(str(root))
        _inspect(self.root)
        s = _safe_stat(self.root)
        if not stat.S_ISDIR(s.st_mode): raise ValueError("owned root is not a directory")
        self.identity = _identity(s)

    def path(self, relative):
        p = resolve_owned_path(str(self.root), relative)
        if _identity(_safe_stat(self.root)) != self.identity:
            raise ValueError("root identity changed")
        return p

    def mkdir(self, relative):
        p = self.path(relative)
        _inspect(p.parent)
        os.mkdir(p)  # Exclusive, never exist_ok, never create outside the root.
        self.path(relative)
        if not stat.S_ISDIR(_safe_stat(p).st_mode): raise ValueError("created directory changed")

    def _open(self, relative, flags):
        p = self.path(relative)
        _inspect(p.parent)
        try:
            before = _safe_stat(p)
        except FileNotFoundError:
            before = None
        if before is not None and (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1):
            raise ValueError("artifact must be a regular, single-link file")
        fd = os.open(p, flags | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0), 0o600)
        try:
            actual = os.fstat(fd)
            self.path(relative)
            after = _safe_stat(p)
            if not stat.S_ISREG(actual.st_mode) or actual.st_nlink != 1 or _identity(actual) != _identity(after):
                raise ValueError("artifact identity changed during open")
            if before is not None and _identity(before) != _identity(actual):
                raise ValueError("artifact was replaced")
            return fd, p, _identity(actual)
        except BaseException:
            os.close(fd)
            raise

    def write_new(self, relative, data):
        fd, p, identity = self._open(relative, os.O_WRONLY | os.O_CREAT | os.O_EXCL)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
        self.path(relative)
        if _identity(_safe_stat(p)) != identity: raise ValueError("file changed after write")

    def read(self, relative, limit):
        fd, p, identity = self._open(relative, os.O_RDONLY)
        with os.fdopen(fd, "rb") as stream:
            before = os.fstat(stream.fileno())
            if before.st_size > limit: raise TooLarge("artifact exceeds byte limit")
            data = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
        self.path(relative)
        current = _safe_stat(p)
        if _identity(current) != identity or (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns) or (current.st_size, current.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError("artifact changed during read")
        if len(data) > limit: raise TooLarge("artifact exceeds byte limit")
        return data

    def names(self, relative, limit=128):
        p = self.path(relative)
        before = _safe_stat(p)
        if not stat.S_ISDIR(before.st_mode): raise ValueError("inventory is not a directory")
        names = []
        with os.scandir(p) as entries:
            for entry in entries:
                if len(names) >= limit: raise TooLarge("too many directory entries")
                # Reject reparse entries without reading their contents.
                child = relative + "/" + entry.name
                self.path(child)
                names.append(entry.name)
        self.path(relative)
        if _identity(_safe_stat(p)) != _identity(before): raise ValueError("directory replaced")
        return sorted(names)
