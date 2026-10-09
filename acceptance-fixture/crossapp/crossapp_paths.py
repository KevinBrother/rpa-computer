"""Local path boundaries. Reparse facts are checked, not just resolved lexically.

No file contents or system settings are changed here. These checks plus the IO
module's before/after checks are defence in depth, NOT an atomic protection from
hostile concurrent ancestor replacement. The supervisor must keep the pack and
its parent private and quiescent while preparing/checking it.
"""
import ntpath
import os
from pathlib import Path
import re
import stat


def _component(name):
    if not name or name in (".", "..") or name.endswith((".", " ")):
        raise ValueError("ambiguous/traversing path component")
    if any(ord(c) < 32 or c in '<>:"|?*' for c in name):
        raise ValueError("invalid Windows name or alternate data stream")
    stem = name.split(".", 1)[0].upper()
    if stem in ("CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$") or re.fullmatch(r"(?:COM|LPT)[1-9¹²³]", stem):
        raise ValueError("reserved Windows device name")


def _absolute(root):
    if not isinstance(root, str) or not root or root.startswith(("\\\\", "//")):
        raise ValueError("root must be an absolute local path, not UNC/device namespace")
    drive, tail = ntpath.splitdrive(root)
    if drive and (not re.fullmatch(r"[A-Za-z]:", drive) or not tail.startswith(("/", "\\"))):
        raise ValueError("drive-relative/device path rejected")
    for part in re.split(r"[\\/]", tail if drive else root):
        if part: _component(part)
    p = Path(root)
    if not p.is_absolute():
        raise ValueError("root must be absolute on the current platform")
    if os.name == "nt":
        # A drive letter can still be a mapped network share. Read-only query;
        # no volume setup or activation. Unknown/remote roots fail closed.
        import ctypes
        get_type = ctypes.WinDLL("kernel32", use_last_error=True).GetDriveTypeW
        get_type.argtypes = [ctypes.c_wchar_p]
        get_type.restype = ctypes.c_uint
        if get_type(p.anchor) not in (2, 3, 6):  # removable, fixed, RAM disk
            raise ValueError("root volume is not a confirmed writable local drive type")
    return p


def _safe_stat(p):
    s = os.lstat(p)
    if stat.S_ISLNK(s.st_mode) or getattr(s, "st_file_attributes", 0) & 0x400:
        raise ValueError("symlink/junction/reparse path rejected: " + str(p))
    return s


def _inspect(p, missing_leaf=False, allow_missing_tail=False):
    """lstat each existing component including root/leaf. Never follow links."""
    chain = list(reversed(p.parents)) + [p]
    missing = False
    for i, entry in enumerate(chain):
        try:
            s = _safe_stat(entry)
        except FileNotFoundError:
            if allow_missing_tail or missing_leaf and i == len(chain) - 1:
                missing = True
                continue
            raise ValueError("parent/root does not exist: " + str(entry))
        if missing:
            raise ValueError("path appeared during inspection")
        if i < len(chain) - 1 and not stat.S_ISDIR(s.st_mode):
            raise ValueError("non-directory ancestor")
    return not missing


def validate_new_root(root: str) -> Path:
    """Inspect NEW absolute local root with existing non-reparse ancestors."""
    p = _absolute(root)
    if _inspect(p, missing_leaf=True):
        raise ValueError("root already exists; never overwrite or reuse it")
    return p


def resolve_owned_path(root: str, relative: str) -> Path:
    """Bound a relative path. This never declares a window or process owned."""
    base = _absolute(root)
    _inspect(base)
    if not stat.S_ISDIR(_safe_stat(base).st_mode):
        raise ValueError("root is not a directory")
    if not isinstance(relative, str) or not relative or relative.startswith(("/", "\\")) or ntpath.splitdrive(relative)[0]:
        raise ValueError("artifact path must be relative")
    parts = re.split(r"[\\/]", relative)
    for part in parts: _component(part)
    candidate = base.joinpath(*parts)
    # No resolve(): it could follow the very reparse point being rejected.
    if not candidate.is_relative_to(base):
        raise ValueError("artifact escapes root")
    _inspect(candidate, allow_missing_tail=True)
    return candidate
