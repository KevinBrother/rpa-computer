#!/usr/bin/env python3
"""Owned deterministic GUI fixture, not a browser or a model black-box test."""
import json
import os
from pathlib import Path
import signal
import tempfile
import time
import tkinter as tk
from tkinter import ttk

STATE_PATH = Path("/tmp/rpa-fixture-state.json")


class Fixture:
    def __init__(self):
        self.root = tk.Tk(className="RpaLinuxFixture")
        self.root.title("RPA Linux X11 | Owned smoke fixture")
        self.root.geometry("960x620+80+40")
        self.root.minsize(760, 500)
        self.root.configure(background="#eef2f6")
        self.started_at = time.time()
        self.clicks = 0
        self.wheel_total = 0
        self.wheel_events = 0
        self.last_event = "startup"
        self.ready = False
        self.pending = None
        self.text = tk.StringVar()
        self.click_label = tk.StringVar(value="Clicks: 0")
        self.status = tk.StringVar(value="Ready for owned-container input only")
        style = ttk.Style()
        style.theme_use("clam")
        style.configure("TFrame", background="#eef2f6")
        style.configure("TLabel", background="#eef2f6", font=("DejaVu Sans", 12))
        style.configure("Title.TLabel", font=("DejaVu Sans", 21, "bold"))
        style.configure("TButton", font=("DejaVu Sans", 12), padding=(16, 10))
        body = ttk.Frame(self.root, padding=28)
        body.pack(fill="both", expand=True)
        ttk.Label(body, text="Linux X11 · Input smoke", style="Title.TLabel").pack(anchor="w")
        ttk.Label(body, text="Owned test content only · noVNC is view-only").pack(anchor="w", pady=(8, 22))
        ttk.Label(body, text="1. Type into the entry").pack(anchor="w")
        self.entry = ttk.Entry(body, textvariable=self.text, font=("DejaVu Sans", 15))
        self.entry.pack(fill="x", pady=(8, 18), ipady=7)
        row = ttk.Frame(body)
        row.pack(fill="x", pady=(0, 18))
        self.button = ttk.Button(row, text="2. Count a click", command=self.click)
        self.button.pack(side="left")
        ttk.Label(row, textvariable=self.click_label).pack(side="left", padx=20)
        ttk.Label(body, text="3. Scroll inside the panel (positive = up, negative = down)").pack(anchor="w")
        self.scroll = tk.Text(body, height=9, font=("DejaVu Sans Mono", 11),
                              background="white", relief="flat", padx=12, pady=12)
        self.scroll.pack(fill="both", expand=True, pady=(8, 12))
        self.scroll.insert("1.0", "\n".join(f"Owned scroll target — row {n:03d}" for n in range(1, 101)))
        self.scroll.configure(state="disabled")
        self.scroll.bind("<Button-4>", lambda event: self.wheel(1), add="+")
        self.scroll.bind("<Button-5>", lambda event: self.wheel(-1), add="+")
        self.scroll.bind("<MouseWheel>", lambda event: self.wheel(event.delta / 120), add="+")
        ttk.Label(body, textvariable=self.status).pack(anchor="w")
        self.widgets = {"entry": self.entry, "button": self.button, "scroll_area": self.scroll}
        self.text.trace_add("write", lambda *_: self.changed("text"))
        self.root.bind("<Configure>", lambda _: self.queue_write(), add="+")
        self.root.bind("<FocusIn>", lambda _: self.queue_write(), add="+")
        self.root.protocol("WM_DELETE_WINDOW", self.close)
        signal.signal(signal.SIGTERM, lambda *_: self.close())
        signal.signal(signal.SIGINT, lambda *_: self.close())
        self.root.after(200, self.activate)

    def activate(self):
        self.ready = True
        self.entry.focus_set()
        self.changed("ready")
        self.tick()

    def tick(self):
        # Give Python signal handlers a bounded chance to run while Tk is idle.
        if self.ready:
            self.root.after(200, self.tick)

    def click(self):
        self.clicks += 1
        self.click_label.set(f"Clicks: {self.clicks}")
        self.changed("button_click")

    def wheel(self, delta):
        self.wheel_total += delta
        self.wheel_events += 1
        self.changed("wheel")

    def changed(self, event):
        self.last_event = event
        self.status.set(f"Last: {event}  |  clicks: {self.clicks}  |  wheel: {self.wheel_total:+g}")
        self.queue_write()

    def queue_write(self):
        if self.pending is None:
            self.pending = self.root.after_idle(self.write_state)

    @staticmethod
    def rectangle(widget):
        x, y = widget.winfo_rootx(), widget.winfo_rooty()
        width, height = widget.winfo_width(), widget.winfo_height()
        return {"x": x, "y": y, "width": width, "height": height,
                "center": {"x": x + width // 2, "y": y + height // 2}}

    def write_state(self):
        self.pending = None
        focus = self.root.focus_get()
        state = {
            "schema_version": 1, "kind": "deterministic_gui_smoke_fixture",
            "pid": os.getpid(), "uid": os.getuid(), "display": os.environ.get("DISPLAY"),
            "started_at_unix": self.started_at, "updated_at_unix": time.time(),
            "ready": self.ready, "last_event": self.last_event,
            "button_clicks": self.clicks, "entry_text": self.text.get(),
            "wheel_total": self.wheel_total, "wheel_events": self.wheel_events,
            "wheel_convention": "positive_up_negative_down; X11 button4=+1 button5=-1",
            "focus_widget": str(focus) if focus else None,
            "window": self.rectangle(self.root),
            "controls": {name: self.rectangle(widget) for name, widget in self.widgets.items()},
        }
        # Same filesystem + replace keeps readers from observing partial JSON.
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=STATE_PATH.parent,
                                         prefix=".rpa-fixture-", delete=False) as handle:
            temporary = Path(handle.name)
            try:
                json.dump(state, handle, ensure_ascii=False, indent=2)
                handle.write("\n")
                handle.flush()
                os.replace(temporary, STATE_PATH)
            finally:
                temporary.unlink(missing_ok=True)

    def close(self):
        self.ready = False
        self.last_event = "shutdown"
        self.write_state()
        self.root.destroy()

    def run(self):
        print(f"[fixture] ground truth: {STATE_PATH}", flush=True)
        self.root.mainloop()


if __name__ == "__main__":
    Fixture().run()
