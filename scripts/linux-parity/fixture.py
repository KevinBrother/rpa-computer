#!/usr/bin/env python3
"""Owned whitebox Tk app. No RPC/control socket and no synthetic input events."""
import argparse
import faulthandler
from collections import deque
import os
from pathlib import Path
import signal
import time
import tkinter as tk

from common import write_json
from diagnostics import save_shutdown


class Fixture:
    def __init__(self, path):
        self.path = path
        self.root = tk.Tk(className="LinuxParityFixture")
        self.root.title("Owned Linux parity • whitebox evidence")
        self.root.geometry("720x500+32+32")
        self.root.resizable(False, False)
        self.root.configure(bg="#edf2f7")
        self.ready = False
        self.callback_errors = deque(maxlen=32)
        self.root.report_callback_exception = self.callback_error
        self.seq = 0
        self.revision = 0
        self.events = deque(maxlen=4096)
        self.keys = {}
        self.buttons = set()
        self.clicks = 0
        self.wheel = {"x": 0, "y": 0, "events": 0}
        self.drag_offset = None
        self.drags = 0
        self.copies = 0
        self.enter_effects = 0
        self.copied_text = None
        self.widgets = {}
        self.label("Linux input parity · owned test surface", 16, 10, 688, 28, 16)
        self.label("Native keys / Unicode / copy-paste / drag / horizontal + vertical wheel",
                   16, 40, 688, 22, 10)
        self.entry = tk.Entry(self.root, name="entry", font=("DejaVu Sans", 12))
        self.place("entry", self.entry, 16, 72, 688, 34)
        self.text = tk.Text(self.root, name="editor", wrap="none", font=("DejaVu Sans Mono", 11),
                            undo=False, takefocus=True)
        self.place("text", self.text, 16, 116, 688, 112)
        self.button = tk.Button(self.root, name="counter", text="Count click: 0",
                                command=self.click, takefocus=True)
        self.place("button", self.button, 16, 240, 200, 36)
        self.label("Tab: Entry → Text. Text Tab inserts a literal tab.", 228, 242, 476, 32, 10)
        self.canvas = tk.Canvas(self.root, name="drag", bg="white", highlightthickness=0)
        self.place("canvas", self.canvas, 16, 288, 430, 160)
        self.target = self.canvas.create_rectangle(32, 50, 80, 98, fill="#176b90", outline="")
        self.destination = self.canvas.create_rectangle(330, 50, 378, 98,
                                                       outline="#176b90", width=3)
        self.canvas.create_text(56, 125, text="A · drag")
        self.canvas.create_text(354, 125, text="B · drop")
        self.canvas.bind("<ButtonPress-1>", self.drag_start)
        self.canvas.bind("<B1-Motion>", self.drag_move)
        self.canvas.bind("<ButtonRelease-1>", self.drag_end)
        self.wheel_canvas = tk.Canvas(self.root, name="wheel", bg="#dce8f0", highlightthickness=0)
        self.place("wheel", self.wheel_canvas, 458, 288, 246, 160)
        self.wheel_label = self.wheel_canvas.create_text(123, 80, text="Wheel target\nx=0 y=0")
        self.label("Ground truth is widget state, not a claim from an input dispatch result.",
                   16, 462, 688, 24, 10)
        for widget in (self.entry, self.text):
            # Deliberate application editing contract, not injected/synthetic events.
            widget.bind("<Control-a>", self.select_all)
            widget.bind("<Control-c>", self.copy)
            widget.bind("<Return>", self.enter_effect, add="+")
        self.root.bind_all("<ButtonPress>", lambda e: self.mouse(e, True), add="+")
        self.root.bind_all("<ButtonRelease>", lambda e: self.mouse(e, False), add="+")
        # Recorder first, then application handlers, then Tk class handlers. A
        # handler returning 'break' (Ctrl+A/C) must not hide events from evidence.
        for widget in self.widgets.values():
            tags = widget.bindtags()
            widget.bindtags(("ParityRecorder",) + tags)
        self.root.bind_class("ParityRecorder", "<KeyPress>", lambda e: self.key(e, True))
        self.root.bind_class("ParityRecorder", "<KeyRelease>", lambda e: self.key(e, False))
        self.root.protocol("WM_DELETE_WINDOW", self.close)
        signal.signal(signal.SIGTERM, lambda *_: self.close())
        signal.signal(signal.SIGINT, lambda *_: self.close())
        self.root.after(100, self.activate)

    def callback_error(self, kind, value, traceback):
        self.callback_errors.append({"type": kind.__name__, "message": str(value)[:500]})

    def label(self, text, x, y, w, h, size):
        tk.Label(self.root, text=text, bg="#edf2f7", fg="#203348",
                 anchor="w", font=("DejaVu Sans", size)).place(x=x, y=y, width=w, height=h)

    def place(self, name, widget, x, y, w, h):
        self.widgets[name] = widget
        widget.place(x=x, y=y, width=w, height=h)

    def record(self, kind, **fields):
        self.seq += 1
        self.events.append({"seq": self.seq, "kind": kind, "monotonic": time.monotonic(), **fields})

    def key(self, event, down):
        if down:
            self.keys[event.keycode] = event.keysym
        else:
            self.keys.pop(event.keycode, None)
        self.record("key_press" if down else "key_release", keycode=event.keycode,
                    keysym=event.keysym, char=event.char, state=event.state,
                    widget=str(event.widget), synthetic=bool(event.send_event))

    def mouse(self, event, down):
        if event.num in (1, 2, 3):
            if down:
                self.buttons.add(event.num)
            else:
                self.buttons.discard(event.num)
        if down and event.widget == self.wheel_canvas and event.num in (4, 5, 6, 7):
            self.wheel["events"] += 1
            axis, delta = {4: ("y", -1), 5: ("y", 1), 6: ("x", -1), 7: ("x", 1)}[event.num]
            self.wheel[axis] += delta
            self.wheel_canvas.itemconfigure(self.wheel_label,
                                           text=f"Wheel target\nx={self.wheel['x']} y={self.wheel['y']}")
        self.record("button_press" if down else "button_release", button=event.num,
                    x_root=event.x_root, y_root=event.y_root, widget=str(event.widget),
                    synthetic=bool(event.send_event))

    def select_all(self, event):
        if event.widget == self.entry:
            self.entry.selection_range(0, "end")
            self.entry.icursor("end")
        else:
            self.text.tag_add("sel", "1.0", "end-1c")
            self.text.mark_set("insert", "end-1c")
        return "break"

    def enter_effect(self, event):
        # Observable application activation; leave normal Tk newline handling intact.
        self.enter_effects += 1

    def copy(self, event):
        try:
            value = (self.entry.selection_get() if event.widget == self.entry
                     else self.text.get("sel.first", "sel.last"))
        except tk.TclError:
            return "break"
        self.root.clipboard_clear()
        self.root.clipboard_append(value)
        self.copies += 1
        self.copied_text = value
        return "break"

    def click(self):
        self.clicks += 1
        self.button.configure(text=f"Count click: {self.clicks}")

    def drag_start(self, event):
        x1, y1, x2, y2 = self.canvas.coords(self.target)
        if x1 <= event.x <= x2 and y1 <= event.y <= y2:
            self.drag_offset = (event.x - x1, event.y - y1)

    def drag_move(self, event):
        if self.drag_offset is not None:
            x, y = event.x - self.drag_offset[0], event.y - self.drag_offset[1]
            self.canvas.coords(self.target, x, y, x + 48, y + 48)

    def drag_end(self, event):
        if self.drag_offset is not None:
            self.drag_move(event)
            self.drags += 1
            self.drag_offset = None

    @staticmethod
    def rect(widget):
        return {"x": widget.winfo_rootx(), "y": widget.winfo_rooty(),
                "width": widget.winfo_width(), "height": widget.winfo_height()}

    def selection(self):
        try:
            return [self.entry.index("sel.first"), self.entry.index("sel.last")]
        except tk.TclError:
            return None

    def state(self):
        self.revision += 1
        focus = self.root.focus_get()
        return {"schema": "linux-parity-fixture-v1", "pid": os.getpid(), "ready": self.ready,
                "revision": self.revision, "monotonic": time.monotonic(), "seq": self.seq,
                "tk_patchlevel": self.root.tk.call("info", "patchlevel"),
                "callback_errors": list(self.callback_errors),
                "root_size": [self.root.winfo_screenwidth(), self.root.winfo_screenheight()],
                "window": self.rect(self.root),
                "controls": {n: {**self.rect(w), "widget": str(w)} for n, w in self.widgets.items()},
                "focus": str(focus) if focus else None,
                "entry": self.entry.get(), "text": self.text.get("1.0", "end-1c"),
                "selection": self.selection(), "clicks": self.clicks,
                "copies": self.copies, "copied_text": self.copied_text, "enter_effects": self.enter_effects,
                "wheel": dict(self.wheel), "pressed_keys": dict(self.keys),
                "pressed_buttons": sorted(self.buttons), "events": list(self.events),
                "drag": {"a": self.canvas.coords(self.target), "b": self.canvas.coords(self.destination),
                         "active": self.drag_offset is not None, "completed": self.drags}}

    def tick(self):
        write_json(self.path, self.state())
        if self.ready:
            self.root.after(25, self.tick)

    def activate(self):
        # Initial focus only on our own newly created window; subsequent focus
        # must come from real host clicks. No event_generate or helper input.
        self.root.lift()
        self.entry.focus_force()
        self.ready = True
        self.tick()

    def close(self):
        if self.ready:
            self.ready = False
            # Do not overwrite the stale/live sample that caused a case to fail.
            save_shutdown(self.path, self.state())
        self.root.destroy()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", required=True, type=Path)
    parser.add_argument("--tracebacks", action="store_true",
                        help="periodic watchdog stack output only; does not pump Tk or update ground truth")
    args = parser.parse_args()
    trace = None
    try:
        if args.tracebacks:
            trace = args.state.with_name("fixture-tracebacks.log").open("xb", buffering=0)
            # C watchdog, independent of Tk's event loop. No signal is sent by the
            # harness and no after/event_generate/update heartbeat is introduced.
            # The owned fixture's lifetime is bounded by the runner/wrapper.
            faulthandler.dump_traceback_later(0.5, repeat=True, file=trace)
        Fixture(args.state).root.mainloop()
    finally:
        if trace is not None:
            faulthandler.cancel_dump_traceback_later()
            trace.close()
