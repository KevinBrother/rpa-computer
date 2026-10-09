#!/usr/bin/env python3
"""Offline native-gesture oracle diagnostic, not a tool-policy/screenshot audit.

Associate by run/session/suite/case/nonce and bounded times; never N-to-N. An
app MATCHED is accepted only with owned native-event evidence or an explicitly
non-input case. First and latest checks are separate. Denominator is always
10 planned UNIQUE cases; zero/rejection cases are not input successes. Missing
model, timestamps, causal tool result, or ambiguous calls stay unknown.
Tool-result content (potentially base64 images) is never traversed or emitted.
"""
from __future__ import annotations
import argparse
from collections import Counter
from datetime import datetime, timezone
import json
import math
from pathlib import Path

MODEL = "glm-5.3-flash"
SPECS = {
    "multiclick": ["flow=single;expect_count=1", "flow=multi;expect_count=2", "flow=multi;expect_count=3",
                  "flow=select_word", "flow=select_line", "flow=two_targets", "flow=two_positions",
                  "flow=right_between", "flow=slow_two", "flow=invalid_count"],
    "drag": ["axis=h;dir=lr", "axis=h;dir=rl", "axis=v;dir=tb", "axis=v;dir=bt", "path=polyline",
             "span=short", "span=long;min_ms=750", "release=inner_edge", "flow=text_select", "path=curve"],
    "scroll": ["panel=A;axis=v;dir=down;ticks=3", "panel=A;axis=v;dir=up;ticks=3",
               "panel=A;axis=h;dir=right;ticks=3", "panel=A;axis=h;dir=left;ticks=3",
               "panel=A;axis=both;dir=down;dir2=right;ticks=3", "panel=B;axis=v;dir=down;ticks=3",
               "panel=A;axis=v;dir=down;ticks=1", "panel=A;axis=v;dir=down;ticks=20;flow=saturate",
               "flow=zero_delta", "flow=invalid_args"],
}
DISCLAIMER = "Oracle diagnostic only; policy audit and screenshot inspection are separate CC+GLM gates; no GUI pass is proved by this report."


def ts(value):
    if isinstance(value, bool): return None
    if isinstance(value, (float, int)): return float(value) if math.isfinite(value) else None
    if not isinstance(value, str): return None
    try:
        d = datetime.fromisoformat(value.replace("Z", "+00:00"))
        # Unqualified local times cannot safely correlate two processes.
        return d.timestamp() if d.tzinfo is not None else None
    except ValueError: return None


def key(r):
    return tuple(r.get(k) if isinstance(r.get(k), (str, int)) and not isinstance(r.get(k), bool) else None
                 for k in ("run_id", "session_id", "suite", "case_id", "nonce", "trial"))


def runkey(r): return r.get("run_id"), r.get("session_id"), r.get("suite")


def load(path):
    records, warnings = [], []
    with Path(path).open(encoding="utf-8-sig") as f:
        for n, line in enumerate(f, 1):
            if not line.strip(): continue
            try:
                r = json.loads(line)
                if isinstance(r, dict): records.append(r)
                else: warnings.append(f"line {n}: non-object record")
            except ValueError: warnings.append(f"line {n}: invalid JSON (incomplete run)")
    return records, warnings


def tools(records):
    calls, results, models = [], {}, []
    missing_model = 0
    for r in records:
        msg = r.get("message")
        if r.get("type") == "assistant":
            model = msg.get("model") if isinstance(msg, dict) else None
            if isinstance(model, str): models.append(model)
            else: missing_model += 1
        if not isinstance(msg, dict): continue
        content = msg.get("content")
        if not isinstance(content, list): continue
        time = ts(r.get("timestamp", r.get("ts")))
        if r.get("type") == "assistant":
            for block in content:
                if not isinstance(block, dict) or block.get("type") != "tool_use": continue
                inp = block.get("input")
                action = inp.get("action") if isinstance(inp, dict) else None
                if not isinstance(action, dict) or action.get("kind") not in ("click", "drag", "scroll"): continue
                name = block.get("name", "")
                if not isinstance(name, str) or not (name == "computer_step" or name.endswith("__computer_step")): continue
                allowed = ("kind", "position", "button", "count", "path", "duration_ms", "delta_x", "delta_y", "unit")
                calls.append(dict(id=block.get("id"), start=time, action={k: action[k] for k in allowed if k in action}))
        elif r.get("type") == "user":
            for block in content:
                if not isinstance(block, dict) or block.get("type") != "tool_result": continue
                ident = block.get("tool_use_id")
                if not isinstance(ident, str): continue
                # Do not inspect content: only existence/nonemptiness/error metadata.
                results.setdefault(ident, []).append(dict(end=time, present=bool(block.get("content")), error=block.get("is_error") is True))
    counts = Counter(c["id"] for c in calls if isinstance(c["id"], str))
    for c in calls:
        rows = results.get(c["id"], []) if isinstance(c["id"], str) else []
        c["result"] = rows[0] if len(rows) == 1 and counts[c["id"]] == 1 and isinstance(c["id"], str) else None
    verified = bool(models) and missing_model == 0 and all(m == MODEL for m in models)
    return calls, verified, sorted(set(models)), missing_model


def native_proof(suite, case_id, events, check):
    """Structural evidence, not an independent replacement for platform judges."""
    mode = case_id in ("multiclick-10", "scroll-09", "scroll-10")
    count = check.get("event_count")
    if not isinstance(count, int) or isinstance(count, bool) or count != len(events):
        return False, "event_count does not match owned stream"
    if check.get("released") is False: return False, "held input at check"
    if mode:
        if suite == "scroll":
            for panel in ("a", "b"):
                for axis in ("v", "h"):
                    start, end = check.get(f"{panel}_{axis}_start"), check.get(f"{panel}_{axis}_end")
                    if type(start) is not int or type(end) is not int or start != end:
                        return False, "no-input case has changed/missing visible offsets"
        return not events, "no native input observed (not proof of API rejection)"
    if not events: return False, "no owned native events"
    if not isinstance(check.get("observed"), str) or not check["observed"]: return False, "visible result absent"
    indices = [e.get("event_index") for e in events]
    if indices != list(range(1, len(events) + 1)): return False, "missing or duplicate raw events"
    for e in events:
        if type(e.get("held")) is not bool: return False, "raw held state missing"
        source = e.get("source", "")
        if not isinstance(source, str) or not source.startswith(("NSEvent", "WndProc")):
            return False, "native source not owned/identified"
    if suite == "scroll":
        wheel = [e for e in events if e.get("kind") == "wheel"]
        target = "panelB" if case_id == "scroll-06" else "panelA"
        if not wheel or any(e.get("area") != target for e in wheel): return False, "missing/wrong panel wheel"
        dx, dy = 0.0, 0.0
        for e in wheel:
            rawx, rawy = e.get("raw_dx"), e.get("raw_dy")
            if any(type(v) not in (int, float) or not math.isfinite(v) for v in (rawx, rawy)):
                return False, "missing/nonfinite raw wheel fields"
            mac = e["source"].startswith("NSEvent")
            if not mac and rawx != 0 and e.get("native_message") != 0x020E:
                return False, "horizontal delta without real WM_MOUSEHWHEEL evidence"
            dx += -rawx if mac else rawx; dy -= rawy
        if dx == 0 and dy == 0: return False, "zero raw wheel deltas"
        offsets = {}
        for panel in ("a", "b"):
            for axis in ("v", "h"):
                start, end = check.get(f"{panel}_{axis}_start"), check.get(f"{panel}_{axis}_end")
                if type(start) is not int or type(end) is not int or start < 0 or end < 0:
                    return False, "visible offsets missing/invalid"
                offsets[panel + axis] = end - start
        panel = "b" if target == "panelB" else "a"
        other = "a" if panel == "b" else "b"
        if offsets[other + "v"] != 0 or offsets[other + "h"] != 0: return False, "adjacent panel changed"
        axes = ("h",) if case_id in ("scroll-03", "scroll-04") else ("v", "h") if case_id == "scroll-05" else ("v",)
        if "h" not in axes and offsets[panel + "h"] != 0 or "v" not in axes and offsets[panel + "v"] != 0:
            return False, "unexpected visible axis movement"
        sign = -1 if case_id in ("scroll-02", "scroll-04") else 1
        minimum = 15 if case_id == "scroll-07" else 45
        for axis in axes:
            raw = dy if axis == "v" else dx
            if raw * sign <= 0 or offsets[panel + axis] * sign <= 0:
                return False, "raw direction and visible displacement contradict expected semantics"
            if case_id != "scroll-08" and abs(offsets[panel + axis]) < minimum:
                return False, "visible displacement below declared threshold"
        if case_id == "scroll-08":
            maximum = check.get("a_v_max")
            if type(maximum) is not int or maximum <= 0 or check["a_v_end"] < maximum:
                return False, "boundary saturation has no maximum-offset proof"
        return True, "real wheel, correct direction/panel and visible offsets"
    held = None
    moves = 0
    for e in events:
        kind, button = e.get("kind"), e.get("button")
        if kind == "down":
            if held is not None: return False, "down before release"
            held = button
        elif kind == "up":
            if held != button or e.get("held"): return False, "up without matching press"
            held = None
        elif kind == "move":
            if e.get("held"):
                if suite != "drag" or held != button: return False, "held motion outside active press"
                moves += 1
        else: return False, "unexpected native event"
    if held is not None: return False, "missing release"
    downs = [e for e in events if e.get("kind") == "down"]
    if suite == "drag":
        return len(downs) == 1 and moves > 0, "ordered press / held motion / released up"
    expected = {"multiclick-01": 1, "multiclick-02": 2, "multiclick-03": 3, "multiclick-04": 2,
                "multiclick-05": 3, "multiclick-06": 2, "multiclick-07": 2, "multiclick-08": 3, "multiclick-09": 2}[case_id]
    if len(downs) != expected: return False, "wrong real press count (Clicks=2 is not two downs)"
    if case_id in ("multiclick-02", "multiclick-03", "multiclick-04", "multiclick-05"):
        native = [e.get("native_count") for e in downs]
        if downs[0]["source"].startswith("NSEvent") and native != list(range(1, expected+1)):
            return False, "native Mac clickCount sequence absent"
        if downs[0]["source"].startswith("WndProc") and not any(e.get("double_click_msg") is True for e in downs):
            return False, "native Windows DBLCLK absent"
    if case_id == "multiclick-04" and check.get("selection") != "alpha": return False, "word selection absent"
    if case_id == "multiclick-05":
        wanted = "alpha beta gamma delta epsilon"
        if not isinstance(check.get("selection"), str) or not check["selection"]: return False, "visible selection absent"
        if downs[0]["source"].startswith("NSEvent") and check["selection"] != wanted: return False, "native line selection absent"
    return True, "real native press/release pairs"


def parameters(calls, trial, check, events):
    start, end = ts(trial.get("ts")), ts(check.get("ts"))
    kind = {"multiclick": "click", "drag": "drag", "scroll": "scroll"}[trial["suite"]]
    candidates = []
    for c in calls:
        result = c["result"]
        if c["action"]["kind"] != kind or c["start"] is None or result is None or result["end"] is None or not result["present"]: continue
        if start < c["start"] <= result["end"] <= end:
            candidates.append(c)
    unknown = dict(status="unknown", reason="no unique timestamp/result-bounded association; never N-to-N")
    if not candidates: return unknown
    # Same-second/coarse timestamps cannot disambiguate simultaneous calls.
    if len({c["start"] for c in candidates}) != len(candidates): return unknown
    if not events:
        if len(candidates) != 1: return unknown
        c = candidates[0]; action = c["action"]
        ident = trial["case_id"]
        semantic = (ident == "multiclick-10" and "count" in action and action["count"] not in (1, 2, 3)) or (
            ident == "scroll-09" and action.get("delta_x") == 0 and action.get("delta_y") == 0) or (
            ident == "scroll-10" and (action.get("unit") != "wheel_ticks" or
                any(isinstance(action.get(k), (int, float)) and abs(action[k]) > 100 for k in ("delta_x", "delta_y"))))
        if not semantic: return unknown
        return dict(status="associated", tool_ids=[c["id"]], parameters=[action], rejected=c["result"]["error"],
                    reason="no-input case, uniquely bounded tool call; API error metadata separate")
    by_call = {}
    for e in events:
        t = ts(e.get("ts"))
        matches = [c for c in candidates if c["start"] <= t <= c["result"]["end"]]
        if len(matches) != 1: return unknown
        c = matches[0]
        by_call.setdefault(c["id"], {"call": c, "events": []})["events"].append(e)
    # Unused click calls might be Check / Next: don't assign them by list index.
    selected = sorted(by_call.values(), key=lambda g: g["call"]["start"])
    for g in selected:
        c = g["call"]
        if c["result"]["error"]: return unknown
        if kind == "click":
            count = c["action"].get("count", 1)
            if not isinstance(count, int) or isinstance(count, bool) or count != sum(e.get("kind") == "down" for e in g["events"]): return unknown
        elif kind == "drag":
            if sum(e.get("kind") == "down" for e in g["events"]) != 1 or sum(e.get("kind") == "up" for e in g["events"]) != 1: return unknown
    return dict(status="associated", tool_ids=[g["call"]["id"] for g in selected],
                parameters=[g["call"]["action"] for g in selected],
                reason="case/nonce-owned native events uniquely enclosed by tool call/result times; screenshot coordinate mapping not proved")


def analyze(oracle, transcript, suite):
    if suite not in SPECS: raise ValueError("unknown suite")
    calls, model_ok, models, missing = tools(transcript)
    catalog = {f"{suite}-{i:02}": spec for i, spec in enumerate(SPECS[suite], 1)}
    rows = [r for r in oracle if r.get("suite") == suite]
    sessions = [r for r in rows if r.get("type") == "session"]
    trials = [r for r in rows if r.get("type") in ("trial", "gesture_trial")]
    freq = Counter(r.get("case_id") for r in trials if isinstance(r.get("case_id"), str))
    reports, warnings = [], []
    for trial in trials:
        cid = trial.get("case_id")
        if not isinstance(cid, str) or cid not in catalog:
            warnings.append("unrecognized case_id ignored"); continue
        report = dict(case_id=cid, nonce=trial.get("nonce"), first="unknown", final="unknown", parameter_evidence=dict(status="unknown"))
        reports.append(report)
        identity = key(trial)
        st = ts(trial.get("ts"))
        own_session = [r for r in sessions if runkey(r) == runkey(trial)]
        if freq[cid] != 1 or any(v is None or v == "" for v in identity) or trial.get("spec") != catalog[cid] or st is None or len(own_session) != 1:
            report["reason"] = "duplicate/forged case, identity/spec/time/session evidence missing"; continue
        if ts(own_session[0].get("ts")) is None or ts(own_session[0]["ts"]) > st:
            report["reason"] = "session timestamp missing or after trial"; continue
        bounds = [ts(r.get("ts")) for r in rows if r.get("type") == "trial_end" and key(r) == identity]
        next_times = [ts(r.get("ts")) for r in trials if runkey(r) == runkey(trial) and ts(r.get("ts")) is not None and ts(r["ts"]) > st]
        if any(t is None for t in bounds) or len(bounds) > 1:
            report["reason"] = "ambiguous trial ending"; continue
        upper = min(bounds + next_times) if bounds or next_times else math.inf
        checks = [r for r in rows if r.get("type") == "gesture_check" and key(r) == identity]
        checks.sort(key=lambda r: ts(r.get("ts")) if ts(r.get("ts")) is not None else math.inf)
        firsts = [r for r in checks if r.get("check_kind") == "first"]
        indexes = [r.get("check_index") for r in checks]
        if any(c.get("check_kind") not in ("first", "final") for c in checks) or not checks or len(firsts) != 1 or indexes != list(range(1, len(checks) + 1)) or checks[0].get("check_kind") != "first":
            report["reason"] = "first/check order missing or duplicated"; continue
        # An unowned event/check during this trial can hide a missing/forged nonce.
        foreign = [r for r in rows if r.get("type") in ("input_event", "gesture_check") and key(r) != identity
                   and (runkey(r) == runkey(trial) or r.get("trial") == trial.get("trial"))
                   and (ts(r.get("ts")) is None or st <= ts(r["ts"]) <= upper)]
        if foreign:
            report["reason"] = "unattributed/wrong nonce/run native event or check in trial interval"; continue
        for label, check in (("first", checks[0]), ("final", checks[-1])):
            ct = ts(check.get("ts"))
            native = [r for r in rows if r.get("type") == "input_event" and key(r) == identity
                      and (ts(r.get("ts")) is None or ts(r["ts"]) <= (ct if ct is not None else math.inf))]
            times = [ts(r.get("ts")) for r in native]
            if ct is None or not st <= ct <= upper or any(t is None or not st <= t <= ct for t in times) or times != sorted(times):
                report[label + "_reason"] = "native/check timestamps missing, unordered or out of interval"; continue
            proof, reason = native_proof(suite, cid, native, check)
            report[label + "_reason"] = reason
            if not proof or type(check.get("matched")) is not bool: continue
            report[label] = "matched" if check["matched"] else "failed"
            if label == "first": report["parameter_evidence"] = parameters(calls, trial, check, native)
        report["non_input_case"] = cid in ("multiclick-10", "scroll-09", "scroll-10")
    first = sum(r["first"] == "matched" for r in reports)
    final = sum(r["final"] == "matched" for r in reports)
    terminal = [r for r in rows if r.get("type") == "suite_complete"]
    complete = False
    if len(terminal) == 1 and len(trials) == 10 and len(reports) == 10 and all(freq[cid] == 1 for cid in catalog) and all(r["first"] != "unknown" for r in reports):
        t = terminal[0]; end = ts(t.get("ts"))
        last_trial = max(trials, key=lambda r: ts(r["ts"]))
        last_ends = [r for r in rows if r.get("type") == "trial_end" and key(r) == key(last_trial)]
        complete = (len(last_ends) == 1 and ts(last_ends[0].get("ts")) is not None and end is not None
                    and ts(last_ends[0]["ts"]) <= end and key(t) == key(last_trial) and t.get("case_total") == 10 and t.get("visited") == 10 and
                    len({runkey(r) for r in trials}) == 1 and runkey(t) == runkey(trials[0]) and
                    all(ts(r.get("ts")) is not None and ts(r["ts"]) <= end for r in rows if r.get("type") == "gesture_check"))
    return dict(suite=suite, planned_cases=10, observed_unique_cases=len(set(r["case_id"] for r in reports)),
        gui_verified=False, screenshot_evidence="unknown (independent inspection required)",
        policy_audit="not performed by this analyzer",
        status="complete" if complete else "partial" if reports else "unknown", suite_complete=complete,
        first_matched=first, final_matched=final, first_pass_rate=None if first == 10 and not complete else first * 10,
        final_pass_rate=None if final == 10 and not complete else final * 10,
        first_input_successes=sum(r["first"] == "matched" and not r.get("non_input_case", False) for r in reports),
        glm_associated_first_matches=sum(model_ok and r["first"] == "matched" and r["parameter_evidence"]["status"] == "associated" for r in reports),
        model_verified=model_ok, observed_models=models, missing_model_messages=missing,
        cases=reports, warnings=warnings, disclaimer=DISCLAIMER)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--oracle", required=True); p.add_argument("--transcript", required=True)
    p.add_argument("--suite", choices=SPECS, required=True); p.add_argument("--output")
    args = p.parse_args()
    oracle, ow = load(args.oracle); trace, tw = load(args.transcript)
    result = analyze(oracle, trace, args.suite); result["warnings"].extend(ow + tw)
    if ow or tw:
        result["suite_complete"] = False; result["status"] = "partial" if result["cases"] else "unknown"
        if result["first_pass_rate"] == 100: result["first_pass_rate"] = None
        if result["final_pass_rate"] == 100: result["final_pass_rate"] = None
    text = json.dumps(result, ensure_ascii=False, indent=2)
    if args.output: Path(args.output).write_text(text + "\n", encoding="utf-8")
    print(text)
    return 0  # Diagnostic only; never an acceptance gate by this exit code.


if __name__ == "__main__": raise SystemExit(main())
