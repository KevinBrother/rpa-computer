"""Independent cases: real host events, actual owned application state."""
import time

from common import NotImplementedCase, require

ASCII = 'AbC XYZ 019 !@#$%^&*()_+-=[]{};:\'",.<>/?\\|`~'
UNICODE = "中文🙂🚀"


def smoke(c):
    c.computer.open()
    old = c.state()["clicks"]
    c.click("button")
    c.wait(lambda s: s["clicks"] == old + 1, "button_exactly_once")
    c.click("entry")
    c.text("abc123")
    c.wait(lambda s: s["entry"] == "abc123", "native_smoke_exact_text")
    c.held_clear("smoke_release")
    c.computer.observe()


def native(c):
    c.computer.open()
    c.click("entry")
    expected = ""
    for label, payload in (("ascii_upper_punctuation", ASCII), ("chinese", "中文"), ("emoji", "🙂🚀")):
        seq = c.state()["seq"]
        c.text(payload)
        expected += payload
        s = c.wait(lambda s: s["entry"] == expected, label + "_exact_text")
        events = [e for e in s["events"] if e["seq"] > seq]
        c.check(any(e["kind"] == "key_press" for e in events) and
                any(e["kind"] == "key_release" for e in events), label + "_real_key_edges")
        c.check(not any(e.get("synthetic") for e in events), label + "_no_send_event")
        c.held_clear(label + "_release")
    c.computer.observe()


def editing(c):
    c.computer.open()
    c.click("entry")
    c.text("copy AbC 中文🙂")
    c.wait(lambda s: s["entry"] == "copy AbC 中文🙂", "copy_source_exact")
    seq = c.state()["seq"]
    c.chord("a")
    # Tk 8.6 indices may count supplementary characters as two units: compare
    # selected text via copy, not a guessed Unicode scalar index length.
    c.wait(lambda s: s["selection"] is not None and s["selection"][0] == 0, "ctrl_a_selection")
    c.chord("c")
    s = c.wait(lambda s: s["copies"] == 1 and s["copied_text"] == "copy AbC 中文🙂", "ctrl_c_exact_copy")
    presses = [e for e in s["events"] if e["seq"] > seq and e["kind"] == "key_press"]
    for key in ("a", "c"):
        c.check(any(e["keysym"].lower() == key and e["state"] & 4 for e in presses),
                "real_ctrl_" + key)
    c.click("text")
    c.chord("v")
    c.wait(lambda s: s["text"] == "copy AbC 中文🙂", "ctrl_v_fixture_owned_clipboard")
    c.held_clear("copy_chords_release")


def multiline(c):
    c.computer.open()
    c.click("entry")
    c.text("prefix\t")
    c.wait(lambda s: s["entry"] == "prefix" and s["focus"] == s["controls"]["text"]["widget"],
           "native_entry_tab_moves_focus")
    c.text("line1\nline2\rline3\r\nline4\tend")
    s = c.wait(lambda s: s["text"] == "line1\nline2\nline3\nline4\tend",
               "native_lf_cr_crlf_tab_exact")
    c.check(s["enter_effects"] == 3, "native_crlf_single_enter_effect")
    symbols = [e["keysym"] for e in s["events"] if e["kind"] == "key_press"]
    c.check(symbols.count("Return") == 3 and symbols.count("Tab") == 2, "native_newline_tab_key_semantics")
    c.held_clear("multiline_release")


def canvas_point(c, item):
    s = c.state()
    a = s["drag"][item]
    r = s["controls"]["canvas"]
    return c.computer.point(r["x"] + (a[0] + a[2]) / 2, r["y"] + (a[1] + a[3]) / 2)


def drag(c):
    c.computer.open()
    c.computer.observe()
    before = c.state()
    c.computer.step({"kind": "drag", "path": [canvas_point(c, "a"), canvas_point(c, "b")],
                     "button": "left", "duration_ms": 450})
    def at_b(s):
        a, b = s["drag"]["a"], s["drag"]["b"]
        return s["drag"]["completed"] == 1 and all(abs(x-y) <= 2 for x, y in zip(a, b))
    after = c.wait(at_b, "drag_actual_canvas_coordinates_at_b")
    c.check(after["drag"]["a"] != before["drag"]["a"], "drag_moved_item_not_just_events")
    c.held_clear("drag_release")
    c.computer.observe()


def scroll(c):
    c.computer.open()
    for axis, delta in (("y", 3), ("y", -2), ("x", 3), ("x", -2)):
        old = dict(c.state()["wheel"])
        c.computer.observe()
        c.computer.step({"kind": "scroll", "position": c.target("wheel"), "delta_x": delta if axis == "x" else 0,
                         "delta_y": delta if axis == "y" else 0, "unit": "wheel_ticks"})
        expected = {**old, axis: old[axis] + delta, "events": old["events"] + abs(delta)}
        c.wait(lambda s: s["wheel"] == expected, f"wheel_{axis}_{delta}_exact")
    c.held_clear("wheel_release")


def hold(c):
    c.computer.open()
    c.click("entry")
    seq = c.state()["seq"]
    c.computer.step({"kind": "key_hold", "key": "shift", "duration_ms": 250})
    s = c.wait(lambda s: not s["pressed_keys"] and s["seq"] > seq + 1, "hold_released")
    events = [e for e in s["events"] if e["seq"] > seq and e.get("keysym", "").startswith("Shift")]
    downs = [e for e in events if e["kind"] == "key_press"]
    ups = [e for e in events if e["kind"] == "key_release"]
    c.check(bool(downs and ups) and ups[-1]["monotonic"] - downs[0]["monotonic"] >= 0.15,
            "hold_observed_down_up_interval", events=events)
    c.text("lower")
    c.wait(lambda s: s["entry"] == "lower", "hold_no_stuck_shift")
    c.held_clear("hold_idle")


def displays(c):
    desc = c.computer.description
    selections = desc.get("display_selections", [])
    if not {"primary", "id", "desktop"} <= set(selections):
        raise NotImplementedCase("primary/id/desktop selection not advertised")
    screens = desc.get("displays", [])
    require(screens, "no display descriptors")
    # Only select a monitor containing our fixture. Other monitors are NOT
    # clickable just because describe lists them.
    window = c.state()["window"]
    containing = [d for d in screens if d["native_bounds"]["x"] <= window["x"] and
                  d["native_bounds"]["y"] <= window["y"] and
                  window["x"] + window["width"] <= d["native_bounds"]["x"] + d["native_bounds"]["width"] and
                  window["y"] + window["height"] <= d["native_bounds"]["y"] + d["native_bounds"]["height"]]
    require(len(containing) == 1, "fixture not wholly inside one display")
    target_id = containing[0]["id"]
    require(containing[0]["is_primary"], "fixture must be on primary for this single-root case")
    root = c.state()["root_size"]
    require(len(screens) == 1, "single-root display case requires one RandR display; multimonitor is separate")
    c.check(containing[0]["native_bounds"] == {"x": 0, "y": 0, "width": root[0], "height": root[1]},
            "enumeration_matches_actual_x_root")
    for selection in ({"kind": "primary"}, {"kind": "id", "id": target_id}, {"kind": "desktop"}):
        for width, height in ((root[0], root[1]), (640, 400)):
            c.computer.open(selection, width, height)
            obs = c.computer.obs
            c.check(obs["selected_display_ids"] == [target_id], "selected_id_exact", selection=selection)
            if (width, height) == tuple(root):
                c.check([obs["width_px"], obs["height_px"]] == root, "unscaled_capture_matches_root")
            else:
                c.check(obs["width_px"] < root[0] and obs["height_px"] < root[1], "resizing_actually_exercised")
                c.check(abs(obs["width_px"] / obs["height_px"] - root[0] / root[1]) < 0.01,
                        "resize_aspect_ratio")
            old = c.state()["clicks"]
            seq = c.state()["seq"]
            c.click("button")
            s = c.wait(lambda s: s["clicks"] == old + 1, "mapped_root_click_hits_button")
            events = [e for e in s["events"] if e["seq"] > seq and e["kind"] == "button_press" and e["button"] == 1]
            r = s["controls"]["button"]
            c.check(bool(events) and abs(events[-1]["x_root"] - (r["x"] + r["width"] // 2)) <= 4 and
                    abs(events[-1]["y_root"] - (r["y"] + r["height"] // 2)) <= 4,
                    "actual_pointer_root_coordinates_match_fixture", events=events)
            c.computer.close()


def lifecycle(c):
    c.computer.open()
    c.click("entry")
    old = c.state()
    _, paused, _ = c.computer.tool("computer_pause", {"session_id": c.computer.sid})
    c.check(paused.get("state") == "paused" and paused.get("cleanup_outcome") in ("released", "not_needed"),
            "pause_acknowledged")
    token = c.computer.step_begin({"kind": "text_input", "text": "MUST_NOT_APPEAR", "method": "native"})
    error, meta, _ = c.computer.finish(token[0], allow_error=True)
    c.check(error and (meta.get("error") or {}).get("code") == "cancelled" and
            meta.get("input_outcome") in (None, "not_started"), "paused_step_rejected")
    c.ev.event("paused_outcome_scope", input_outcome=meta.get("input_outcome"),
               note="Current guard may return only error; no inferred input outcome. Fixture proves no events.")
    c.wait(lambda s: s["entry"] == old["entry"] and s["seq"] == old["seq"], "paused_no_app_events", stable=0.3)
    _, resumed, _ = c.computer.tool("computer_resume", {"session_id": c.computer.sid})
    c.check(resumed.get("state") == "ready", "resume_ready")
    c.computer.observe()
    c.text("resumed")
    c.wait(lambda s: s["entry"] == "resumed", "resume_actual_input")
    c.computer.observe()
    c.computer.close()
    c.held_clear("closed_idle")
    c.check(c.rpc.eof_and_wait() == 0, "clean_eof_exit")


def clipboard(c):
    c.computer.open()
    c.click("text")
    branches = c.computer.tools["computer_step"]["properties"]["action"].get("oneOf", [])
    text_schema = next((b for b in branches if b.get("properties", {}).get("kind", {}).get("const") == "text_input"), {})
    methods = text_schema.get("properties", {}).get("method", {}).get("enum", [])
    if "paste" not in methods:
        raise NotImplementedCase("paste method absent from advertised schema")
    c.ev.event("capability_probe", note="global schema is not proof of Linux clipboard implementation")
    payload = "Paste AbC 中文🙂\nline2\tend"
    before = c.state()
    token = c.computer.step_begin({"kind": "text_input", "method": "paste", "text": payload})
    error, meta, images = c.computer.finish(token[0], allow_error=True)
    c.check(meta.get("request_id") == token[1], "paste_request_id")
    if error:
        code = (meta.get("error") or {}).get("code", "")
        if code in ("unsupported", "unsupported_platform", "not_implemented"):
            c.wait(lambda s: s["text"] == before["text"] and s["seq"] == before["seq"],
                   "unimplemented_paste_has_no_input", stable=0.25)
            require(meta.get("input_outcome") == "not_started", "unsupported paste had possible side effects")
            raise NotImplementedCase("Linux explicit paste reports " + code)
        raise FailureWithOutcome(meta)
    c.check(meta.get("input_outcome") == "dispatched" and
            meta.get("cleanup_outcome") in ("released", "not_needed"), "paste_dispatch_and_cleanup")
    c.computer.accept_observation(meta["observation"], images)
    s = c.wait(lambda s: s["text"] == payload and s["focus"] == s["controls"]["text"]["widget"] and
               s["enter_effects"] == before["enter_effects"], "explicit_paste_exact_multiline_unicode_no_activation")
    events = [e for e in s["events"] if e["seq"] > before["seq"] and e["kind"] == "key_press"]
    c.check(not any(e["keysym"] in ("Return", "Tab") for e in events), "paste_no_native_tab_return_fallback")
    # A second ordinary Ctrl+V uses the same clipboard payload, demonstrating
    # retained content while this host owns the selection (no user clipboard read).
    c.chord("v")
    c.wait(lambda s: s["text"] == payload * 2 and s["enter_effects"] == before["enter_effects"],
           "paste_leaves_clipboard_payload_without_enter_effects")
    c.held_clear("paste_idle")


class FailureWithOutcome(Exception):
    def __init__(self, meta):
        super().__init__("paste failed: " + str((meta.get("error") or {}).get("code")) +
                         "; input_outcome=" + str(meta.get("input_outcome")))


SCENARIOS = {"smoke": smoke, "native": native, "editing": editing, "multiline": multiline,
             "drag": drag, "scroll": scroll, "hold": hold, "displays": displays,
             "lifecycle": lifecycle, "clipboard": clipboard}
