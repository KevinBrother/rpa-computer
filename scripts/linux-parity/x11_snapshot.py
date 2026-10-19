#!/usr/bin/env python3
"""Read-only Xlib core map/modifiers/held state; NOT a full XKB struct binding.

Run as a bounded child: XOpenDisplay/XSync are blocking C calls. No XTest,
XChangeKeyboardMapping, XSetModifierMapping, XGrabServer or input injection.
"""
import ctypes as C
import ctypes.util
import json


class ModifierMap(C.Structure):
    _fields_ = [("max_keypermod", C.c_int), ("modifiermap", C.POINTER(C.c_ubyte))]


def snapshot():
    lib = ctypes.util.find_library("X11")
    if not lib:
        raise RuntimeError("libX11 unavailable")
    x = C.CDLL(lib)
    signatures = {
        "XOpenDisplay": (C.c_void_p, [C.c_char_p]),
        "XCloseDisplay": (C.c_int, [C.c_void_p]),
        "XDisplayKeycodes": (C.c_int, [C.c_void_p, C.POINTER(C.c_int), C.POINTER(C.c_int)]),
        "XGetKeyboardMapping": (C.POINTER(C.c_ulong), [C.c_void_p, C.c_ubyte, C.c_int, C.POINTER(C.c_int)]),
        "XGetModifierMapping": (C.POINTER(ModifierMap), [C.c_void_p]),
        "XFreeModifiermap": (C.c_int, [C.POINTER(ModifierMap)]),
        "XFree": (C.c_int, [C.c_void_p]),
        "XQueryKeymap": (C.c_int, [C.c_void_p, C.POINTER(C.c_char)]),
        "XDefaultRootWindow": (C.c_ulong, [C.c_void_p]),
        "XQueryPointer": (C.c_int, [C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong),
                                     C.POINTER(C.c_ulong), C.POINTER(C.c_int), C.POINTER(C.c_int),
                                     C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_uint)]),
        "XDefaultScreen": (C.c_int, [C.c_void_p]),
        "XDisplayWidth": (C.c_int, [C.c_void_p, C.c_int]),
        "XDisplayHeight": (C.c_int, [C.c_void_p, C.c_int]),
    }
    for name, (result, args) in signatures.items():
        fn = getattr(x, name)
        fn.restype, fn.argtypes = result, args
    display = x.XOpenDisplay(None)
    if not display:
        raise RuntimeError("XOpenDisplay failed")
    mapping = modifiers = None
    try:
        low, high, width = C.c_int(), C.c_int(), C.c_int()
        x.XDisplayKeycodes(display, C.byref(low), C.byref(high))
        if not 8 <= low.value <= high.value <= 255:
            raise RuntimeError("invalid core keycode range")
        mapping = x.XGetKeyboardMapping(display, low.value, high.value - low.value + 1, C.byref(width))
        if not mapping or not 1 <= width.value <= 128:
            raise RuntimeError("invalid core keymap")
        keys = [[int(mapping[i * width.value + j]) for j in range(width.value)]
                for i in range(high.value - low.value + 1)]
        modifiers = x.XGetModifierMapping(display)
        if not modifiers or not 0 <= modifiers.contents.max_keypermod <= 256:
            raise RuntimeError("invalid modifier map")
        slots = modifiers.contents.max_keypermod
        mods = [[int(modifiers.contents.modifiermap[i * slots + j]) for j in range(slots)]
                for i in range(8)]
        held = C.create_string_buffer(32)
        x.XQueryKeymap(display, held)
        pressed = [i for i in range(256) if held.raw[i // 8] & (1 << (i % 8))]
        root, child = C.c_ulong(), C.c_ulong()
        rx, ry, wx, wy, mask = C.c_int(), C.c_int(), C.c_int(), C.c_int(), C.c_uint()
        same_screen = x.XQueryPointer(display, x.XDefaultRootWindow(display), C.byref(root),
                                     C.byref(child), C.byref(rx), C.byref(ry), C.byref(wx),
                                     C.byref(wy), C.byref(mask))
        screen = x.XDefaultScreen(display)
        return {"scope": "XKB-core-projection-only; excludes XKB actions/types/groups/controls",
                "mapping": {"min": low.value, "max": high.value, "width": width.value,
                            "keysyms": keys, "modifiers": mods},
                "pressed_keycodes": pressed,
                "pointer": {"same_screen": bool(same_screen), "x": rx.value, "y": ry.value,
                            "mask": mask.value, "buttons_mask": mask.value & 0x1f00},
                "root_size": [x.XDisplayWidth(display, screen), x.XDisplayHeight(display, screen)]}
    finally:
        if mapping:
            x.XFree(C.cast(mapping, C.c_void_p))
        if modifiers:
            x.XFreeModifiermap(modifiers)
        x.XCloseDisplay(display)


if __name__ == "__main__":
    print(json.dumps(snapshot()))
