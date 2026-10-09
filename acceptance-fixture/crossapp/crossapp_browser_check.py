"""Untrusted browser application records, never trusted GUI/OS evidence."""
import math


def _number(value):
    if type(value) not in (int, float):
        return False
    try:
        return math.isfinite(value)
    except OverflowError:
        return False


def check_browser_export(data, case, oracle):
    unknown = ("unknown", "missing/malformed bounded browser application evidence; no GUI claim")
    if not isinstance(data, dict) or data.get("schema") != "windows-crossapp-browser-events-v1" or any(data.get(k) != case[k] for k in ("case_id", "run_id", "trial_id", "nonce")):
        return unknown
    if data.get("events_truncated") is not False or data.get("untrusted_application_data") is not True or data.get("download_confirmation") is not True:
        return unknown
    events = data.get("events")
    if not isinstance(events, list) or not 1 <= len(events) <= 256:
        return unknown
    allowed = {"scroll_y", "scroll_x", "form_submit", "drag_start", "drag_motion", "drop", "drag_end", "download_gate", "export_requested"}
    time = -1
    for i, e in enumerate(events, 1):
        if not isinstance(e, dict) or type(e.get("index")) is not int or e["index"] != i or not isinstance(e.get("kind"), str) or e.get("kind") not in allowed or e.get("trusted") is not True:
            return unknown
        t = e.get("elapsed_ms")
        if not _number(t) or t < time or t < 0 or t > 3600000:
            return unknown
        time = t
        if e["kind"] in ("scroll_y", "scroll_x") and (not _number(e.get("position")) or e["position"] < 0):
            return unknown
        if e["kind"] in ("drag_start", "drag_motion", "drop") and not all(_number(e.get(k)) for k in ("x", "y")):
            return unknown
    kinds = [e["kind"] for e in events]
    for kind in ("scroll_y", "scroll_x"):
        positions = [e["position"] for e in events if e["kind"] == kind]
        if not positions or max(positions) <= 0:
            return unknown
    # Preserve first-attempt failures, not just the final form state.
    submits = [e for e in events if e["kind"] == "form_submit"]
    if len(submits) != 1:
        return unknown
    if submits[0].get("text") != oracle["expected_form_text"] or submits[0].get("choice") != oracle["expected_choice"]:
        return "mismatch", "application first submitted values differ (not a GUI judgement)"
    if any(kinds.count(k) != 1 for k in ("drag_start", "drop", "drag_end", "export_requested")):
        return unknown
    start, drop, end = (kinds.index(k) for k in ("drag_start", "drop", "drag_end"))
    if not start < drop < end or not any(e["kind"] == "drag_motion" for e in events[start+1:drop]):
        return unknown
    if events[drop].get("target") != "drop-target" or events[end].get("dropped") is not True or kinds[-1] != "export_requested":
        return unknown
    final = data.get("final")
    if not isinstance(final, dict): return unknown
    if final.get("text") != oracle["expected_form_text"] or final.get("choice") != oracle["expected_choice"] or final.get("dropped") is not True:
        return "mismatch", "application final values differ; raw data not rewritten"
    return "artifact_match", "bounded application record matches; browser isTrusted/download flags are self-reports, NOT trusted complete GUI/ownership/drag proof"
