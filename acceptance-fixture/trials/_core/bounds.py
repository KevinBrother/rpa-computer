"""Bounded, strict JSON and regular-file input; no normalization or evidence I/O."""
import json
import os
from pathlib import Path
import stat

MANIFEST_BYTES = 1024 * 1024
RECORDS_BYTES = 8 * 1024 * 1024
MAX_RECORDS = 1200
MAX_STRING = 4096
MAX_DEPTH = 8


def canonical_json(value, limit):
    """Validate plain JSON types and budget before serializing, including aliases/cycles."""
    remaining = limit
    ancestors = set()

    def spend(size):
        nonlocal remaining
        remaining -= size
        if remaining < 0:
            raise ValueError("canonical JSON byte limit exceeded")

    def text(s):
        if len(s) > MAX_STRING or any(0xD800 <= ord(c) <= 0xDFFF for c in s):
            raise ValueError("overlong string or unpaired Unicode surrogate")
        # ASCII escaping makes character count exactly UTF-8 byte count.
        spend(len(json.dumps(s, ensure_ascii=True)))

    def visit(item, depth):
        kind = type(item)
        if kind in (dict, list):
            if depth >= MAX_DEPTH or id(item) in ancestors:
                raise ValueError("JSON nesting limit exceeded or cycle")
            ancestors.add(id(item))
            spend(2 + max(0, len(item) - 1))  # delimiters + commas
            if kind is dict:
                spend(len(item))  # colons
                for key, val in item.items():
                    if type(key) is not str:
                        raise ValueError("JSON object keys must be strings")
                    text(key)
                    visit(val, depth + 1)
            else:
                for val in item:
                    visit(val, depth + 1)
            ancestors.remove(id(item))
        elif kind is str:
            text(item)
        elif kind is bool:
            spend(4 if item else 5)
        elif item is None:
            spend(4)
        elif kind is int:
            # No contract integer field permits values outside signed 64 bits.
            if item.bit_length() > 63:
                raise ValueError("integer outside contract bounds")
            spend(len(str(item)))
        else:
            raise ValueError("only plain JSON values accepted; floats are forbidden")

    visit(value, 0)
    return json.dumps(value, ensure_ascii=True, sort_keys=True,
                      separators=(",", ":"), allow_nan=False)


def read_regular(path, limit):
    """No streams/devices/FIFOs/symlinks; cap bytes even if a regular file grows."""
    path = Path(path)
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
        raise ValueError("input must be a bounded regular file")
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NONBLOCK", 0)
    flags |= getattr(os, "O_NOFOLLOW", 0)
    fd = os.open(path, flags)
    try:
        opened = os.fstat(fd)
        if (not stat.S_ISREG(opened.st_mode) or opened.st_size > limit
                or (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino)):
            raise ValueError("input changed or is not a bounded regular file")
        with os.fdopen(fd, "rb", closefd=False) as stream:
            data = stream.read(limit + 1)
        if len(data) > limit:
            raise ValueError("input byte limit exceeded")
        return data
    finally:
        os.close(fd)


def load_json_file(path, limit):
    def pairs(items):
        obj = {}
        for key, value in items:
            if key in obj:
                raise ValueError("duplicate JSON key")
            obj[key] = value
        return obj

    def no_float(token):
        raise ValueError("non-integer JSON number forbidden")

    raw = read_regular(path, limit)
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=pairs,
                           parse_float=no_float, parse_constant=no_float)
        canonical_json(value, limit)
        return value
    except (UnicodeError, RecursionError) as exc:
        raise ValueError("invalid UTF-8 JSON or excessive nesting") from exc
