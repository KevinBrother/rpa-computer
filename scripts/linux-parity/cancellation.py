"""Cancel only after fixture evidence proves a real key/button is currently down."""
import time

from common import require
from scenarios import canvas_point


def cancel(c, operation, device):
    c.computer.open()
    c.click("entry")
    before = c.state()
    if device == "key":
        action = {"kind": "key_hold", "key": "shift", "duration_ms": 5000}
        predicate = lambda s: any(k.startswith("Shift") for k in s["pressed_keys"].values())
    else:
        c.computer.observe()
        action = {"kind": "drag", "button": "left", "duration_ms": 5000,
                  "path": [canvas_point(c, "a"), canvas_point(c, "b")]}
        predicate = lambda s: 1 in s["pressed_buttons"] and s["drag"]["active"]
    token = c.computer.step_begin(action)
    held = c.wait(predicate, "cancellation_precondition_actual_held_" + device, timeout=2, stable=0)
    started = time.monotonic()
    c.ev.event("cancel_requested", operation=operation, device=device, request_id=token[1], held_state=held)
    if operation == "eof":
        # The transport may discard the step reply when stdin closes. Preserve
        # any returned reply; never manufacture an outcome for a missing one.
        rc = c.rpc.eof_and_wait(seconds=3)
        c.check(rc == 0, "eof_cancellation_host_exit")
        if token[0] in c.rpc.replies:
            c.computer.step_finish(token, cancellation=True)
        else:
            c.ev.event("step_outcome_unavailable_on_eof", request_id=token[1], input_outcome="unknown",
                       note="No step reply; only fixture/X server release and process exit are asserted")
        c.computer.sid = None
    else:
        request = c.computer.begin("computer_" + operation, {"session_id": c.computer.sid})
        # Either response order is valid; Rpc routes by id and keeps both deadlines.
        c.computer.step_finish(token, cancellation=True)
        _, meta, _ = c.computer.finish(request)
        c.check(meta.get("state") == ("paused" if operation == "pause" else "closed"), "cancel_state")
        c.check(meta.get("cleanup_outcome") in ("released", "not_needed"), "cancel_control_cleanup")
        if operation == "close":
            c.computer.sid = None
    c.check(time.monotonic() - started < 3, "cancel_before_natural_five_second_completion")
    c.held_clear("cancel_" + operation + "_" + device + "_released")
    after = c.state()
    event_kind = "key_release" if device == "key" else "button_release"
    released = [e for e in after["events"] if e["seq"] > held["seq"] and e["kind"] == event_kind]
    c.check(bool(released), "fixture_received_release_after_cancel", events=released)
    c.wait(lambda s: s["seq"] == after["seq"] and s["entry"] == before["entry"] and
           s["drag"]["a"] == after["drag"]["a"], "cancel_no_late_input", stable=0.4)
    if operation == "pause":
        c.computer.tool("computer_resume", {"session_id": c.computer.sid})
        c.computer.observe()
        c.click("entry")
        c.text("lower")
        c.wait(lambda s: s["entry"] == "lower", "post_cancel_resume_no_held_modifier")
        c.held_clear("post_cancel_resume_idle")


def cases():
    return {f"cancel-{operation}-{device}":
            (lambda c, op=operation, dev=device: cancel(c, op, dev))
            for operation in ("pause", "close", "eof") for device in ("key", "mouse")}
