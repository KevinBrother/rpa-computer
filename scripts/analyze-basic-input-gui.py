#!/usr/bin/env python3
"""Windows B2 offline oracle diagnostic; NEVER a GUI/policy certification.

Strict explicit JSON tool-result metadata is needed here (rejection/scaling), so
this small parser is independent of the gesture analyzer's opaque-result parser.
Only known envelope/text metadata levels are read; image blocks are never walked
or included in output. No N-to-N association, no inference from model prose.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime
import json
import math
from pathlib import Path

MODEL = "glm-5.3-flash"
SPECS = {
    "pointer": ("center_move", "edge_move", "scaled_move", "left_click", "right_menu", "middle_click",
                "inactive_click", "small_target", "padding_reject", "bounds_reject"),
    "keyboard": ("plain_key", "enter_tab", "select_all", "shift_select", "alt_command", "multi_modifier",
                 "short_hold", "long_hold", "modifier_release", "invalid_key"),
}
DISCLAIMER = "Diagnostic only: actual GLM tool-policy audit and screenshot review remain CC gates. No GUI verification is claimed."
FIELDS = ("run_id", "session_id", "suite", "case_id", "nonce", "trial")


def timestamp(value):
    if type(value) in (int, float):
        return float(value) if math.isfinite(value) else None
    if isinstance(value, str):
        try:
            d = datetime.fromisoformat(value.replace("Z", "+00:00"))
            return d.timestamp() if d.tzinfo else None
        except (ValueError, OverflowError):
            pass
    return None


def identity(row):
    values = tuple(row.get(k) for k in FIELDS)
    return values if all(isinstance(x, str) and x for x in values[:-1]) and type(values[-1]) is int else None


def load(path):
    rows, warnings = [], []
    with Path(path).open(encoding="utf-8-sig") as f:
        for n, line in enumerate(f, 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
                if not isinstance(row, dict):
                    raise ValueError("non-object")
                rows.append(row)
            except ValueError:
                warnings.append(f"line {n}: invalid/incomplete JSON record")
    return rows, warnings


def json_object(text):
    if not isinstance(text, str) or len(text) > 131072:
        return None
    try:
        value = json.loads(text)
        return value if isinstance(value, dict) else None
    except ValueError:
        return None


def result_metadata(block):
    """Accept direct metadata or a single MCP content envelope, not arbitrary trees."""
    content = block.get("content")
    candidates = []
    outer_error = block.get("is_error") is True
    if isinstance(content, str):
        obj = json_object(content)
        if obj is None:
            return None
        if isinstance(obj.get("content"), list):
            outer_error |= obj.get("isError") is True
            content = obj["content"]
        else:
            candidates.append(obj)
    if isinstance(content, list):
        for item in content:
            if not isinstance(item, dict) or item.get("type") != "text":
                continue  # including image; no access to its payload
            obj = json_object(item.get("text"))
            if obj is not None:
                candidates.append(obj)
    if len(candidates) != 1:
        return None
    obj = candidates[0]
    # Some CC adapters put a serialized MCP envelope in one text block.
    # Unwrap that single declared level only; never recurse into unknown objects.
    if isinstance(obj.get("content"), list):
        outer_error |= obj.get("isError") is True
        texts = [json_object(x.get("text")) for x in obj["content"]
                 if isinstance(x, dict) and x.get("type") == "text"]
        texts = [x for x in texts if x is not None]
        if len(texts) != 1 or "content" in texts[0]:
            return None
        obj = texts[0]
    error = obj.get("error")
    obs = obj.get("observation")
    if not isinstance(obs, dict) and "observation_id" in obj:
        obs = obj
    observation = None
    if isinstance(obs, dict):
        names = ("session_id", "observation_id", "surface_id", "geometry_version", "width_px", "height_px", "input_sequence")
        observation = {k: obs[k] for k in names if k in obs and isinstance(obs[k], (str, int)) and not isinstance(obs[k], bool)}
    # Do not return error prose, images, unknown objects or entire content.
    return dict(input_outcome=obj.get("input_outcome") if isinstance(obj.get("input_outcome"), str) else None,
                cleanup_outcome=obj.get("cleanup_outcome") if isinstance(obj.get("cleanup_outcome"), str) else None,
                error_code=error.get("code") if isinstance(error, dict) and isinstance(error.get("code"), str) else None,
                error=outer_error or obj.get("is_error") is True or obj.get("step_is_error") is True or isinstance(error, dict),
                request_id=obj.get("request_id") if isinstance(obj.get("request_id"), str) else None,
                observation=observation)


def tool_envelopes(rows):
    calls, results, models = [], {}, []
    missing_model = 0
    for row in rows:
        if not isinstance(row,dict):
            continue
        message = row.get("message")
        if not isinstance(message, dict):
            continue
        content = message.get("content")
        if row.get("type") == "assistant":
            model = message.get("model")
            if not isinstance(model, str):
                missing_model += 1
            else:
                models.append(model)
        if not isinstance(content, list):
            continue
        time = timestamp(row.get("timestamp", row.get("ts")))
        for block in content:
            if not isinstance(block, dict):
                continue
            if row.get("type") == "assistant" and block.get("type") == "tool_use":
                name = block.get("name")
                if not isinstance(name, str):
                    continue
                short = name.split("__")[-1]
                if short not in ("computer_step", "computer_open", "computer_observe", "computer_get_step"):
                    continue
                inp = block.get("input")
                if not isinstance(inp, dict):
                    inp = {}
                action = inp.get("action")
                allowed = ("kind", "position", "button", "count", "key", "duration_ms", "modifiers")
                action = {k: action[k] for k in allowed if k in action} if isinstance(action, dict) else {}
                calls.append(dict(id=block.get("id") if isinstance(block.get("id"),str) else None,
                                  name=short, start=time, action=action,
                                  session_id=inp.get("session_id") if isinstance(inp.get("session_id"),str) else None,
                                  request_id=inp.get("request_id") if isinstance(inp.get("request_id"),str) else None,
                                  based_on=inp.get("based_on") if isinstance(inp.get("based_on"),str) else None))
            elif row.get("type") == "user" and block.get("type") == "tool_result":
                ident = block.get("tool_use_id")
                if isinstance(ident, str):
                    results.setdefault(ident, []).append(dict(end=time, metadata=result_metadata(block)))
    ids = Counter(c["id"] for c in calls if isinstance(c["id"], str))
    requests = Counter((c["session_id"], c["request_id"]) for c in calls if c["name"] == "computer_step"
                       and isinstance(c["session_id"], str) and isinstance(c["request_id"], str))
    observations = []
    for c in calls:
        rs = results.get(c["id"], []) if isinstance(c["id"], str) else []
        c["result"] = rs[0] if len(rs) == 1 and ids[c["id"]] == 1 else None
        c["duplicate_request"] = c["name"] == "computer_step" and (not isinstance(c["session_id"], str)
            or not isinstance(c["request_id"], str) or requests[(c["session_id"], c["request_id"])] != 1)
        r = c["result"]
        if r and r["metadata"] and r["metadata"]["observation"] and c["start"] is not None and r["end"] is not None:
            obs = dict(r["metadata"]["observation"])
            obs["time"] = r["end"]
            if c["start"] <= r["end"] and not r["metadata"]["error"]:
                observations.append(obs)
    return calls, observations, bool(models) and not missing_model and all(m == MODEL for m in models), sorted(set(models))


def raw_valid(e):
    for k in ("native_message", "raw_wparam", "raw_lparam", "native_timestamp_ms", "event_index", "own_handle"):
        if type(e.get(k)) is not int:
            return False
    if e["own_handle"] == 0 or not 0 <= e["native_timestamp_ms"] <= 0xffffffff:
        return False
    source, msg, kind = e.get("source"), e["native_message"], e.get("kind")
    if not all(isinstance(e.get(k),str) for k in ("kind","area","source")):
        return False
    if kind == "activate":
        return source == "WndProc/own-Form" and msg == 0x21 and type(e.get("active_before")) is bool
    if source == "WndProc/own-pointer":
        table = {0x200:("move","none"),0x201:("down","left"),0x202:("up","left"),0x203:("down","left"),
                 0x204:("down","right"),0x205:("up","right"),0x206:("down","right"),
                 0x207:("down","middle"),0x208:("up","middle"),0x209:("down","middle")}
        if not isinstance(e.get("button"),str):
            return False
        mask={"left":1,"right":2,"middle":16}.get(e.get("button"),0)
        if kind=="down" and not e["raw_wparam"]&mask or kind=="up" and e["raw_wparam"]&mask:
            return False
        signed = lambda n: (n & 0xffff) - (65536 if n & 0x8000 else 0)
        return table.get(msg) == (kind, e.get("button")) and type(e.get("x")) is int and type(e.get("y")) is int \
            and e["x"] == signed(e["raw_lparam"]) and e["y"] == signed(e["raw_lparam"] >> 16)
    if source != "IMessageFilter/own-EDIT" or e.get("focused") is not True or e.get("area") != "editor" \
            or e.get("focus_handle") != e["own_handle"]:
        return False
    kinds = {0x100:"key_down",0x101:"key_up",0x102:"char",0x104:"key_down",0x105:"key_up",0x106:"char"}
    return kinds.get(msg) == kind and type(e.get("key_code")) is int and e["key_code"] == e["raw_wparam"] \
        and type(e.get("modifiers")) is int and (e["raw_lparam"] & 65535)>0 and (kind == "char" or bool(e["raw_lparam"] & 0x80000000) == (kind == "key_up"))


def canonical_key(vk):
    return {160:16,161:16,162:17,163:17,164:18,165:18}.get(vk,vk)


def native_proof(spec, events, check, trial):
    if type(check.get("event_count")) is not int or check["event_count"] != len(events):
        return False, "missing/extra raw events"
    if check.get("released") is not True:
        return False, "held input or missing release snapshot"
    if not isinstance(check.get("visible_result"), str) or not check["visible_result"]:
        return False, "visible result missing"
    for i, e in enumerate(events, 1):
        if e.get("event_index") != i or not raw_valid(e):
            return False, "raw mismatch, event index gap or wrong owned focus"
        t = timestamp(e.get("t_ms")) if type(e.get("t_ms")) in (int,float) else None
        if t is None or t < 0 or i > 1 and t < events[i-2]["t_ms"]:
            return False, "invalid relative time"
        if abs((timestamp(e["ts"])-timestamp(trial["ts"]))*1000-t)>150:
            return False, "native event relative/UTC time mismatch"
        if i>1:
            previous=events[i-2]
            native_delta=(e["native_timestamp_ms"]-previous["native_timestamp_ms"])&0xffffffff
            if abs(native_delta-(t-previous["t_ms"]))>250:
                return False, "native message timestamps inconsistent with elapsed event time"
    if spec == "invalid_key" and (check.get("text")!="" or check.get("selection_length")!=0):
        return False, "rejected key changed native EDIT state"
    if spec in ("bounds_reject", "invalid_key"):
        return not events, "owned event0 (not proof of rejection)"
    if not events:
        return False, "native events missing"
    if trial["suite"] == "pointer":
        if type(trial.get("target_handle")) is not int or trial["target_handle"]==0 or any(
                e["kind"]!="activate" and e.get("own_handle")!=trial["target_handle"] for e in events):
            return False, "wrong owned target HWND"
        downs = [e for e in events if e["kind"] == "down"]
        ups = [e for e in events if e["kind"] == "up"]
        if spec.endswith("move"):
            area = "edge" if spec == "edge_move" else "target"
            return not downs and not ups and any(e["kind"] == "move" and e.get("area") == area for e in events), "native move zone"
        button = "right" if spec == "right_menu" else "middle" if spec == "middle_click" else "left"
        ok = len(downs) == len(ups) == 1 and downs[0]["event_index"] < ups[0]["event_index"] \
            and all(e.get("button") == button and e.get("area") == "target" for e in downs+ups) \
            and downs[0]["native_message"] not in (0x203,0x206,0x209)
        if spec == "right_menu":
            ok &= check.get("context_opened") is True
        if spec == "inactive_click":
            ok &= trial.get("secondary_prepared_inactive") is True and bool(downs) and any(
                e["kind"] == "activate" and e.get("active_before") is False and e.get("own_handle")==trial.get("secondary_form_handle") and e["event_index"] < downs[0]["event_index"] for e in events)
        return ok, "native click/button/zone/visible state"
    held, pressed, pairs = {}, [], []
    for e in events:
        kind, vk = e["kind"], canonical_key(e.get("key_code"))
        if kind == "char":
            continue
        if kind == "key_down":
            if vk in held:
                return False, "repeated keydown not an independent press"
            held[vk] = e
            pressed.append(vk)
        elif kind == "key_up":
            if vk not in held:
                return False, "unpaired keyup"
            pairs.append((held.pop(vk),e))
        else:
            return False, "unexpected non-keyboard event"
    if held:
        return False, "no native key release"
    expected = {"plain_key":[65],"enter_tab":[13,9],"select_all":[17,65],"shift_select":[16,37],
                "alt_command":[18,74],"multi_modifier":[17,16,74],"short_hold":[16],"long_hold":[16],"modifier_release":[17,65,66]}
    if pressed != expected.get(spec):
        return False, "wrong native key sequence"
    initial = "ALPHA BRAVO" if spec in ("select_all","shift_select","modifier_release") else ""
    if trial.get("initial_text") != initial:
        return False, "initial native state missing/mismatched"
    if spec in ("short_hold","long_hold"):
        ms = (pairs[0][1]["native_timestamp_ms"]-pairs[0][0]["native_timestamp_ms"]) & 0xffffffff
        duration = 120 if spec == "short_hold" else 900
        return duration-40 <= ms <= duration+600 and check.get("text") == initial, "native down/up hold duration"
    required = {"select_all":2,"modifier_release":2,"shift_select":1,"alt_command":4,"multi_modifier":3}.get(spec,0)
    if required:
        main = 37 if spec == "shift_select" else 74 if spec in ("alt_command","multi_modifier") else 65
        principal = next(e for e in events if e["kind"] == "key_down" and canonical_key(e["key_code"]) == main)
        if principal.get("modifiers") != required or any(u["event_index"] < principal["event_index"] for d,u in pairs if canonical_key(d["key_code"]) in (16,17,18)):
            return False, "modifier lifetime mismatch"
    chars = [e["key_code"] for e in events if e["kind"] == "char"]
    text, start, length = check.get("text"), check.get("selection_start"), check.get("selection_length")
    ok = False
    if spec == "plain_key":
        ok = text == "a" and length == 0 and 97 in chars
    elif spec == "enter_tab":
        ok = text == "\r\n\t" and 13 in chars and 9 in chars
    elif spec == "select_all":
        ok = text == initial and start == 0 and length == len(initial)
    elif spec == "shift_select":
        ok = text == initial and start == 10 and length == 1
    elif spec in ("alt_command","multi_modifier"):
        ok = text == initial and check.get("command_count") == 1
    elif spec == "modifier_release":
        b = next(e for e in events if e["kind"] == "key_down" and e["key_code"] == 66)
        ok = text == "b" and length == 0 and 98 in chars and b.get("modifiers") == 0 and any(
            e["kind"] == "key_up" and canonical_key(e["key_code"]) == 17 and e["event_index"] < b["event_index"] for e in events)
    return ok, "native EDIT exact text/selection/own command"


def action_matches(spec, action):
    kind = action.get("kind")
    if spec in ("center_move","edge_move","scaled_move","bounds_reject","padding_reject"):
        pos = action.get("position")
        return kind == "move" and isinstance(pos,list) and len(pos)==2 and all(type(n) is int for n in pos) \
            and (pos == [-1,-1] if spec == "bounds_reject" else all(n>=0 for n in pos))
    if spec in ("left_click","right_menu","middle_click","inactive_click","small_target"):
        button = "right" if spec == "right_menu" else "middle" if spec == "middle_click" else "left"
        pos = action.get("position")
        return kind == "click" and action.get("button","left")==button and type(action.get("count",1)) is int and action.get("count",1)==1 \
            and isinstance(pos,list) and len(pos)==2 and all(type(n) is int and n>=0 for n in pos)
    holds = {"plain_key":("a",80),"short_hold":("shift",120),"long_hold":("shift",900)}
    if spec in holds:
        return kind == "key_hold" and (action.get("key"),action.get("duration_ms"))==holds[spec]
    if spec == "enter_tab":
        return kind == "key_hold" and action.get("key") in ("enter","tab") and action.get("duration_ms")==80
    chords = {"select_all":(["ctrl"],"a"),"shift_select":(["shift"],"left"),"alt_command":(["alt"],"j"),
              "multi_modifier":(["ctrl","shift"],"j"),"invalid_key":(["ctrl"],"__invalid__")}
    if spec in chords:
        return kind == "key_chord" and (action.get("modifiers"),action.get("key"))==chords[spec]
    if spec == "modifier_release":
        return action_matches("select_all",action) or kind=="key_hold" and action.get("key")=="b" and action.get("duration_ms")==80
    return False


def associated(spec, trial, check, events, calls, observations):
    lo, hi = timestamp(trial.get("ts")), timestamp(check.get("ts"))
    steps = [c for c in calls if c["name"]=="computer_step" and c["start"] is not None and lo <= c["start"] <= hi]
    candidates = [c for c in steps if action_matches(spec,c["action"])]
    expected_count = 2 if spec in ("enter_tab","modifier_release") else 1
    if len(candidates)!=expected_count:
        return False, "missing/ambiguous matching action count", []
    candidates.sort(key=lambda c:c["start"])
    completed_others=[c for c in steps if c not in candidates and c["result"] and c["result"]["end"] is not None
                      and c["result"]["end"]<=hi]
    if any(trial["suite"]=="pointer" or c["action"].get("kind") in ("key_chord","key_hold","text_input") for c in completed_others):
        return False, "extra target actions in first-attempt interval", []
    if spec=="enter_tab" and [c["action"].get("key") for c in candidates]!=["enter","tab"]:
        return False, "Enter/Tab call order mismatch", []
    if spec=="modifier_release" and [c["action"].get("kind") for c in candidates]!=["key_chord","key_hold"]:
        return False, "chord/ordinary-key call order mismatch", []
    for c in candidates:
        r = c["result"]
        if c["duplicate_request"] or not all(isinstance(c.get(k),str) and c[k] for k in ("session_id","request_id","based_on")) \
                or not r or r["end"] is None or not c["start"] <= r["end"] <= hi or not r["metadata"]:
            return False, "missing/duplicate tool identity, timestamps or result", []
        data = r["metadata"]
        if data["request_id"] is not None and data["request_id"] != c["request_id"]:
            return False, "result request_id mismatch", []
        reject = spec in ("bounds_reject","invalid_key")
        if reject:
            if not data["error"] or data["error_code"]!="invalid_action" or data["input_outcome"]!="not_started" or data["cleanup_outcome"] not in ("not_needed","released"):
                return False, "explicit invalid_action/not_started rejection missing", []
        elif data["error"] or data["input_outcome"]!="dispatched" or data["cleanup_outcome"] not in ("not_needed","released"):
            return False, "tool outcome not a completed clean dispatch", []
    # Every raw event must belong to exactly one real call envelope; no positional pairing.
    used = set()
    for e in events:
        t=timestamp(e.get("ts"))
        owners=[c for c in steps if c["result"] and c["result"]["end"] is not None and c["start"] <= t <= c["result"]["end"]]
        if len(owners)!=1 or owners[0] not in candidates:
            return False, "raw event outside unique expected call/result envelope", []
        used.add(owners[0]["id"])
    for c in candidates:
        if trial["suite"]!="keyboard":
            continue
        owned=[e for e in events if c["start"]<=timestamp(e["ts"])<=c["result"]["end"]]
        vocabulary={"a":65,"b":66,"j":74,"left":37,"enter":13,"tab":9,"shift":16,"ctrl":17,"alt":18}
        a=c["action"]
        expected_keys=[vocabulary.get(k) for k in a.get("modifiers",[])+[a.get("key")]]
        if spec!="invalid_key" and [canonical_key(e["key_code"]) for e in owned if e["kind"]=="key_down"]!=expected_keys:
            return False, "tool action/native key group mismatch", []
    if events and len(used)!=expected_count:
        return False, "a requested action has no native event evidence", []
    if len(candidates)>1 and any(candidates[i]["result"]["end"]>=candidates[i+1]["start"] for i in range(len(candidates)-1)):
        return False, "overlapping sequential actions", []
    if spec=="scaled_move":
        c=candidates[0]
        current=[o for o in observations if o.get("observation_id")==c["based_on"] and o.get("session_id")==c["session_id"] and o["time"]<=c["start"]]
        if len(current)!=1:
            return False, "actual based_on observation dimensions missing/ambiguous", []
        o=current[0]
        widths=(o.get("width_px"),o.get("height_px"))
        if not all(type(n) is int and n>0 for n in widths) or not o.get("surface_id") or not o.get("geometry_version"):
            return False, "actual observation geometry metadata missing", []
        larger=[old for old in observations if old["time"]<o["time"] and old.get("surface_id")==o["surface_id"]
                and old.get("geometry_version")==o["geometry_version"] and type(old.get("width_px")) is int
                and type(old.get("height_px")) is int and old["width_px"]>=widths[0] and old["height_px"]>=widths[1]
                and (old["width_px"]>widths[0] or old["height_px"]>widths[1])]
        if not larger or not all(0<=p<n for p,n in zip(c["action"]["position"],widths)):
            return False, "no proven smaller observation on same surface/geometry, or invalid image position", []
    return True,"unique time-bounded native/tool association",[dict(tool_use_id=c["id"],request_id=c["request_id"],action=c["action"],
        input_outcome=c["result"]["metadata"]["input_outcome"],cleanup_outcome=c["result"]["metadata"]["cleanup_outcome"]) for c in candidates]


def analyze(oracle, transcript, suite):
    if suite not in SPECS:
        raise ValueError("only pointer and keyboard suites are supported")
    calls, observations, model_ok, models=tool_envelopes(transcript)
    oracle=[r for r in oracle if isinstance(r,dict)]
    rows=[r for r in oracle if r.get("suite")==suite]
    trials=[r for r in rows if r.get("type")=="basic_trial"]
    runs={(r.get("run_id"),r.get("session_id")) for r in trials if identity(r)}
    single_run=len(runs)==1
    cases=[]
    for n,spec in enumerate(SPECS[suite],1):
        cid=f"{suite}-{n:02d}"
        item=dict(case_id=cid,spec=spec,first="unknown",final="unknown",checks=0,retry_checks=0,valid_input=False,
                  reason="missing trial",parameter_evidence=[],trial_attempts=0,retry_trials=0)
        matches=[r for r in trials if r.get("case_id")==cid]
        item["trial_attempts"]=len(matches);item["retry_trials"]=max(0,len(matches)-1)
        if spec=="padding_reject":
            item.update(first="blocked",final="blocked",reason="needs_capability: padding contract not connected; never upgrade")
            cases.append(item);continue
        if len(matches)!=1 or not single_run:
            item["reason"]="missing/duplicate trial or multiple runs; split evidence per run"
            cases.append(item);continue
        trial=matches[0];ident=identity(trial);start=timestamp(trial.get("ts"))
        if not ident or trial.get("trial")!=n or trial.get("platform")!="windows" or trial.get("spec")!=spec or start is None:
            item["reason"]="invalid case identity/spec/platform/time";cases.append(item);continue
        nonce=trial["nonce"]
        if sum(t.get("nonce")==nonce for t in trials)!=1:
            item["reason"]="reused nonce";cases.append(item);continue
        own=[r for r in rows if identity(r)==ident]
        checks=[r for r in own if r.get("type")=="basic_check"]
        item["checks"]=len(checks);item["retry_checks"]=max(0,len(checks)-1)
        if not checks:
            cases.append(item);continue
        if [r.get("check_index") for r in checks]!=list(range(1,len(checks)+1)) or [r.get("check_kind") for r in checks]!=["first"]+["final"]*(len(checks)-1):
            item["reason"]="duplicate/missing first or nonmonotonic check indices";cases.append(item);continue
        check_times=[timestamp(c.get("ts")) for c in checks]
        if any(t is None for t in check_times) or any(check_times[i]<=check_times[i-1] for i in range(1,len(check_times))):
            item["reason"]="missing or nonmonotonic check timestamps";cases.append(item);continue
        verdicts=[]
        for check in checks:
            end=timestamp(check.get("ts"))
            if end is None or end<start:
                verdicts.append("unknown");item["reason"]="invalid check timestamp";continue
            # A changed identity/missing timestamp cannot make an event silently disappear.
            interval=[r for r in oracle if r.get("type")=="basic_input" and
                      (identity(r)==ident or timestamp(r.get("ts")) is not None and start<=timestamp(r["ts"])<=end)]
            events=[r for r in interval if timestamp(r.get("ts")) is None or timestamp(r["ts"])<=end]
            malformed=any(identity(r)!=ident or timestamp(r.get("ts")) is None or not start<=timestamp(r["ts"])<=end for r in events)
            malformed |= any(timestamp(events[i]["ts"]) < timestamp(events[i-1]["ts"])
                             for i in range(1,len(events)) if timestamp(events[i].get("ts")) is not None and timestamp(events[i-1].get("ts")) is not None)
            selected=[trial,check]+events
            malformed |= any(type(r.get("utc_ms")) not in (int,float) or timestamp(r.get("utc_ms")) is None or abs(timestamp(r["ts"])*1000-r["utc_ms"])>50 for r in selected if timestamp(r.get("ts")) is not None)
            malformed |= any(t is not trial and timestamp(t.get("ts")) is not None and start<=timestamp(t["ts"])<=end for t in trials)
            if malformed:
                verdicts.append("unknown");item["reason"]="foreign case/nonce, missing time or overlapping trial";continue
            proof,reason=native_proof(spec,events,check,trial)
            tools_ok,tool_reason,parameters=associated(spec,trial,check,events,calls,observations) if proof else (False,"raw proof insufficient; no tool association claimed",[])
            state=check.get("state")
            expected_state="needs_tool_evidence" if spec in ("bounds_reject","invalid_key","scaled_move") else "matched"
            if state=="mismatch":
                result="failed"
            elif state=="blocked":
                result="blocked"
            elif state==expected_state and proof and tools_ok and model_ok:
                result="rejection_confirmed" if spec in ("bounds_reject","invalid_key") else "matched"
            else:
                result="unknown"
            verdicts.append(result)
            item["reason"]=reason+"; "+tool_reason
            item["parameter_evidence"]=parameters
        item["first"]=verdicts[0];item["final"]=verdicts[-1]
        item["valid_input"]=item["first"]=="matched" and spec not in ("bounds_reject","invalid_key","padding_reject")
        cases.append(item)
    first=sum(c["first"]=="matched" for c in cases);final=sum(c["final"]=="matched" for c in cases)
    completed=[r for r in rows if r.get("type")=="basic_suite_complete" and r.get("visited")==10
               and identity(r) is not None and identity(r)[:2] in runs]
    complete=single_run and len(trials)==10 and {r.get("case_id") for r in trials}=={c["case_id"] for c in cases} and len(completed)==1
    if complete:
        for t in trials:
            ends=[r for r in rows if r.get("type")=="basic_trial_end" and identity(r)==identity(t)]
            checks=[r for r in rows if r.get("type")=="basic_check" and identity(r)==identity(t)]
            if len(ends)!=1 or not checks or ends[0].get("final_check_index")!=checks[-1].get("check_index"):
                complete=False;break
            et,ct=timestamp(ends[0].get("ts")),timestamp(checks[-1].get("ts"))
            if et is None or ct is None or et<ct or timestamp(completed[0].get("ts")) is None or et>timestamp(completed[0]["ts"]):
                complete=False;break
        complete &= timestamp(completed[0].get("ts")) is not None and all(timestamp(t.get("ts")) is not None and timestamp(t["ts"])<=timestamp(completed[0]["ts"]) for t in trials)
    return dict(schema="windows-basic-analysis-v1",suite=suite,platform="windows",status="complete" if complete else "partial" if trials else "unknown",
                suite_complete=bool(complete),planned_cases=10,first_matched=first,final_matched=final,
                first_pass_rate=first*10,final_pass_rate=final*10,valid_input=sum(c["valid_input"] for c in cases),
                rejected=sum(c["first"]=="rejection_confirmed" for c in cases),blocked=sum(c["first"]=="blocked" for c in cases),
                retry_checks=sum(c["retry_checks"] for c in cases),retry_trials=sum(c["retry_trials"] for c in cases),
                first_failed=sum(c["first"]=="failed" for c in cases),first_unknown=sum(c["first"]=="unknown" for c in cases),
                seen_cases=sum(c["trial_attempts"]>0 for c in cases),model_verified=model_ok,models=models,
                gui_verified=False,ten_valid_trials_per_action_gate=False,disclaimer=DISCLAIMER,cases=cases)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--oracle",required=True);p.add_argument("--transcript",required=True)
    p.add_argument("--suite",choices=SPECS,required=True);p.add_argument("--output")
    args=p.parse_args()
    oracle,ow=load(args.oracle);transcript,tw=load(args.transcript)
    report=analyze(oracle,transcript,args.suite);report["parse_warnings"]={"oracle":ow,"transcript":tw}
    if ow or tw:
        # Truncated records can hide failed inputs; no passing aggregate from incomplete evidence.
        report.update(status="unknown",suite_complete=False,first_matched=0,final_matched=0,first_pass_rate=0,final_pass_rate=0,valid_input=0,rejected=0)
        for case in report["cases"]:
            if case["first"]!="blocked":case.update(first="unknown",final="unknown",valid_input=False,reason="incomplete/invalid JSONL evidence")
    text=json.dumps(report,ensure_ascii=False,indent=2)
    if args.output:
        with Path(args.output).open("x",encoding="utf-8") as f:f.write(text+"\n")
    else:print(text)


if __name__=="__main__":
    main()
