#!/usr/bin/env python3
"""Known-09 Windows native expected sidecar. Never reclassifies unversioned runs.

Diagnostic only; no GUI certification or suite-wide score. Read-only B2 JSON/time/
metadata primitives are reused; old layered/focus/B2 analyzers are not modified.
Actual text and tool payload are preserved and compared as UTF-16 code units.
"""
from __future__ import annotations
import argparse
from collections import Counter
import importlib.util
import json
from pathlib import Path

_SPEC=importlib.util.spec_from_file_location("_native_text_primitives",Path(__file__).with_name("analyze-basic-input-gui.py"))
BASE=importlib.util.module_from_spec(_SPEC);_SPEC.loader.exec_module(BASE)
VERSION="windows-winforms-known09-crlf-v1"
CONTROL="System.Windows.Forms.TextBox.Multiline"
RAW="第一行\r\n第二行\r\n第三行"
CANONICAL="第一行\n第二行\n第三行"
TOOLS={"computer_"+x for x in ("describe","open","observe","step","get_step","pause","resume","close")}
ts=BASE.timestamp


def hex16(text):
    data=text.encode("utf-16-le",errors="surrogatepass")
    return " ".join(f"{int.from_bytes(data[i:i+2],'little'):04X}" for i in range(0,len(data),2))


def exact(a,b):return isinstance(a,str) and isinstance(b,str) and hex16(a)==hex16(b)


def identity(r):
    values=tuple(r.get(k) for k in ("run_id","session_id","suite","case_id","nonce","trial"))
    return values if all(isinstance(v,str) and v for v in values[:-1]) and type(values[-1]) is int else None


def explicit_policy(r):
    return r.get("platform")=="windows" and r.get("native_text_policy_version")==VERSION and r.get("native_control")==CONTROL \
        and r.get("native_override_applied") is True and r.get("actual_normalized") is False \
        and exact(r.get("task_payload"),RAW) and exact(r.get("canonical_expected_text"),CANONICAL) \
        and r.get("canonical_expected_utf16_hex")==hex16(CANONICAL) and exact(r.get("native_expected_text"),RAW) \
        and r.get("native_expected_utf16_hex")==hex16(RAW) and exact(r.get("expected_text"),RAW)


def trace_data(rows):
    calls=[];results={};models=[];policy=True;missing_model=False
    for r in rows:
        if not isinstance(r,dict):continue
        msg=r.get("message")
        if not isinstance(msg,dict):continue
        if r.get("type")=="assistant":
            model=msg.get("model")
            if not isinstance(model,str):missing_model=True
            else:models.append(model)
        blocks=msg.get("content")
        if not isinstance(blocks,list):continue
        for b in blocks:
            if not isinstance(b,dict):continue
            time=ts(r.get("timestamp",r.get("ts")))
            if r.get("type")=="assistant" and b.get("type")=="tool_use":
                name=b.get("name");name=name.split("__")[-1] if isinstance(name,str) else ""
                policy &= name in TOOLS
                inp=b.get("input");inp=inp if isinstance(inp,dict) else {}
                action=inp.get("action");action=action if isinstance(action,dict) else {}
                calls.append(dict(id=b.get("id") if isinstance(b.get("id"),str) else None,name=name,start=time,inp=inp,action=action))
            if r.get("type")=="user" and b.get("type")=="tool_result" and isinstance(b.get("tool_use_id"),str):
                results.setdefault(b["tool_use_id"],[]).append(dict(end=time,meta=BASE.result_metadata(b)))
    counts=Counter(c["id"] for c in calls)
    requests=Counter((c["inp"]["session_id"],c["inp"]["request_id"]) for c in calls if c["name"]=="computer_step"
                     and all(isinstance(c["inp"].get(k),str) for k in ("session_id","request_id")))
    for c in calls:
        res=results.get(c["id"],[])
        c["result"]=res[0] if c["id"] and counts[c["id"]]==1 and len(res)==1 else None
        c["unique_request"]=all(isinstance(c["inp"].get(k),str) and c["inp"][k] for k in ("session_id","request_id","based_on"))
        if c["unique_request"]:c["unique_request"]=requests[(c["inp"]["session_id"],c["inp"]["request_id"])]==1
    return calls, bool(models) and not missing_model and all(m=="glm-5.3-flash" for m in models), policy and not any(k not in counts for k in results)


def observation_proof(call,calls,lo):
    available=[]
    for c in calls:
        r=c["result"]
        if c["name"] not in ("computer_open","computer_observe","computer_step","computer_get_step") or not r or not r["meta"]:continue
        if c["start"] is None or r["end"] is None or not lo<=c["start"]<=r["end"]<=call["start"]:continue
        o=r["meta"]["observation"]
        if r["meta"]["error"] or not o or o.get("session_id")!=call["inp"].get("session_id"):continue
        if not all(type(o.get(k)) is int and o[k]>0 for k in ("width_px","height_px")):continue
        if not o.get("geometry_version") or not o.get("surface_id"):continue
        available.append((r["end"],o))
    chosen=[x for x in available if x[1].get("observation_id")==call["inp"].get("based_on")]
    if len(chosen)!=1 or any(x[0]>chosen[0][0] for x in available):return False
    return not any(c is not call and c["name"]=="computer_step" and c["inp"].get("session_id")==call["inp"].get("session_id")
                   and c["start"] is not None and chosen[0][0]<c["start"]<call["start"] for c in calls)


def analyze(oracle,transcript):
    rows=[r for r in oracle if isinstance(r,dict)]
    checks=[r for r in rows if r.get("type")=="text_check" and (r.get("case_id")=="known-09" or r.get("suite")=="known-input" and r.get("trial")==9)]
    out=dict(schema="windows-native-text-analysis-v1",scope="known-input/known-09 only",required_policy=VERSION,
             first="unknown",final="unknown",retry_checks=max(0,len(checks)-1),checks=[],
             historical_matched_flags=[r.get("matched") for r in checks],gui_verified=False,legacy63_modified=False,
             overall_suite_result=None,reason="not a complete explicitly versioned sidecar run; no historical reclassification")
    trials=[r for r in rows if r.get("type")=="trial" and r.get("suite")=="known-input" and r.get("case_id")=="known-09"]
    if len(trials)!=1 or not checks:return out
    trial=trials[0];ident=identity(trial);start=ts(trial.get("ts"))
    if not ident or trial.get("trial")!=9 or start is None or not explicit_policy(trial):return out
    sessions=[r for r in rows if r.get("type")=="session" and (r.get("run_id"),r.get("session_id"))==ident[:2]]
    if len(sessions)!=1:return out
    session=sessions[0]
    if session.get("platform")!="windows" or session.get("native_text_policy_version")!=VERSION or session.get("native_control")!=CONTROL or ts(session.get("ts")) is None or ts(session["ts"])>start:return out
    times=[ts(r.get("ts")) for r in checks]
    if any(t is None for t in times) or any(identity(r)!=ident for r in checks) or times[0]<=start or any(times[i]<=times[i-1] for i in range(1,len(times))):return out
    if [r.get("check_index") for r in checks]!=list(range(1,len(checks)+1)) or [r.get("check_kind") for r in checks]!=["first"]+["final"]*(len(checks)-1):return out
    if any(r is not trial and r.get("type")=="trial" and (ts(r.get("ts")) is None or start<=ts(r["ts"])<=times[-1]) for r in rows):return out
    calls,model_ok,tool_ok=trace_data(transcript);out.update(model_verified=model_ok,tool_inventory_allowed=tool_ok)
    previous=start
    for check,end in zip(checks,times):
        item=dict(check_index=check["check_index"],state="unknown",canonical_match=None,native_match=None,
                  reason="missing/ambiguous raw or tool evidence",actual_text=check.get("actual_text"))
        out["checks"].append(item)
        candidates=[c for c in calls if c["name"]=="computer_step" and c["action"].get("kind")=="text_input"
                    and (c["start"] is None or previous<c["start"]<end)]
        other_keys=any(c["name"]=="computer_step" and c["action"].get("kind") not in ("text_input","click","move","scroll","drag")
                       and (c["start"] is None or previous<c["start"]<end) for c in calls)
        previous=end
        if not explicit_policy(check) or not isinstance(check.get("actual_text"),str) or type(check.get("matched")) is not bool:continue
        if check.get("actual_utf16_hex")!=hex16(check["actual_text"]) or check.get("expected_utf16_hex")!=hex16(RAW):continue
        item.update(canonical_match=exact(check["actual_text"],CANONICAL),native_match=exact(check["actual_text"],RAW))
        if len(candidates)!=1 or other_keys or not model_ok or not tool_ok:continue
        c=candidates[0];r=c["result"]
        if c["start"] is None or not c["unique_request"] or not r or r["end"] is None or not c["start"]<=r["end"]<end or not r["meta"]:continue
        m=r["meta"]
        if m["error"] or m["input_outcome"]!="dispatched" or m["cleanup_outcome"] not in ("released","not_needed") or m["request_id"]!=c["inp"]["request_id"]:continue
        if not observation_proof(c,calls,start) or not isinstance(c["action"].get("text"),str):continue
        payload=c["action"]["text"]
        item.update(raw_tool_payload=payload,raw_payload_match=exact(payload,RAW),tool_use_id=c["id"],request_id=c["inp"]["request_id"],based_on=c["inp"]["based_on"])
        if check["matched"]!=item["native_match"]:
            item["reason"]="app flag contradicts exact native comparison";continue
        item["state"]="native_exact" if item["raw_payload_match"] and item["native_match"] else "mismatch"
        item["reason"]="strict UTF-16 against explicit native expected; canonical comparison separately retained"
    out.update(first=out["checks"][0]["state"],final=out["checks"][-1]["state"],reason="versioned known-09 diagnostic only; old9/10 and GUI gates unchanged")
    return out


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--oracle",required=True);p.add_argument("--transcript",required=True);p.add_argument("--output",required=True)
    args=p.parse_args();o,ow=BASE.load(args.oracle);t,tw=BASE.load(args.transcript)
    out=analyze(o,t);out["parse_warnings"]=dict(oracle=ow,transcript=tw)
    if ow or tw:
        out.update(first="unknown",final="unknown",reason="incomplete JSONL; no policy classification")
        for item in out["checks"]:item["state"]="unknown"
    with Path(args.output).open("x",encoding="utf-8") as f:json.dump(out,f,ensure_ascii=True,indent=2);f.write("\n")


if __name__=="__main__":main()
