"""Public API, also loadable by exec_module without registering this facade.

Register only the private package before its relative imports; never alter sys.path.
A location-derived name prevents different staged source trees sharing modules.
"""
from hashlib import sha256 as _sha256
import importlib.util as _import_util
from pathlib import Path as _Path
import sys as _sys

_core_path = _Path(__file__).resolve().parent / "_core" / "__init__.py"
_core_name = "_windows_first_trials_" + _sha256(str(_core_path).encode("utf-8")).hexdigest()
_core = _sys.modules.get(_core_name)
if _core is None:
    _spec = _import_util.spec_from_file_location(_core_name, _core_path)
    _core = _import_util.module_from_spec(_spec)
    _sys.modules[_core_name] = _core
    try:
        _spec.loader.exec_module(_core)
    except BaseException:
        for _name in tuple(_sys.modules):
            if _name == _core_name or _name.startswith(_core_name + "."):
                del _sys.modules[_name]
        raise

build_manifest = _core.build_manifest
summarize = _core.summarize
__all__ = ["build_manifest", "summarize"]
