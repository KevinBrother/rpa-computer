#!/usr/bin/env python3
"""Supervisor-only Windows geometry diagnostic. Never publish this to GUI agents.

Three independent inputs: owned application JSONL, unedited CC tool stream JSONL,
and supervisor identity bindings/environment attestations. Matching is by exact
call ID, request ID, observation ID, case/nonce and bounded timestamps; never N:N.
PNG bytes are checked here only. No pixel/source/oracle information goes to GLM.
A diagnostic_match is NOT screenshot review or GUI acceptance certification.
"""
from __future__ import annotations
import argparse
import base64
import binascii
from collections import Counter
import importlib.util
import json
import math
from pathlib import Path
import struct
import zlib

_SPEC = importlib.util.spec_from_file_location("_geometry_primitives", Path(__file__).with_name("analyze-basic-input-gui.py"))
BASE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(BASE)
ts = BASE.timestamp
MODEL = "glm-5.3-flash"
TOOLS = {"computer_"+x for x in ("describe","open","observe","step","get_step","pause","resume","close")}
SPECS = ("native_size","bound_1920","bound_1440","aspect_mapping","high_dpi","negative_origin","resolution_stale","dpi_stale","target_removal","png_dimensions")
FIELDS = ("run_id","session_id","suite","case_id","nonce","trial")
MAX_PNG = 80*1024*1024
MAX_LINE = 112*1024*1024


def integer(x):return type(x) is int

def number(x):return type(x) in (int,float) and math.isfinite(x)

def identity(row):
    v=tuple(row.get(k) for k in FIELDS)
    return v if all(isinstance(x,str) and x for x in v[:-1]) and integer(v[-1]) and v[-1]>0 else None


def read_rows(path):
    rows=[]
    if Path(path).stat().st_size>512*1024*1024:raise ValueError("JSONL exceeds 512MiB limit; split supervisor runs")
    with Path(path).open("rb") as f:
        while True:
            line=f.readline(MAX_LINE+1)
            if not line:break
            if len(line)>MAX_LINE:raise ValueError("oversized JSONL line")
            if not line.strip():continue
            row=json.loads(line.decode("utf-8-sig"))
            if not isinstance(row,dict):raise ValueError("non-object JSONL record")
            rows.append(row)
            if len(rows)>200000:raise ValueError("too many records")
    return rows


def png_size(encoded):
    """Bounded strict PNG structure/CRC/zlib/scanline validation (8-bit, no interlace).

    Unsupported PNG encodings are unknown, not a false match. No IHDR-only pass.
    Runtime produces RGB/RGBA PNG; grayscale 8-bit is also validated. Ancillary
    extensions not interpreted here are rejected as unsupported, never ignored. No files,
    shell, decoders with unbounded allocation, or image pixels are emitted.
    """
    if not isinstance(encoded,str) or len(encoded)>MAX_PNG*4//3+4:raise ValueError("PNG base64 missing/oversized")
    try:raw=base64.b64decode(encoded,validate=True)
    except (ValueError,binascii.Error) as exc:raise ValueError("invalid base64") from exc
    if len(raw)>MAX_PNG or raw[:8]!=b"\x89PNG\r\n\x1a\n":raise ValueError("invalid PNG signature/size")
    pos=8;header=None;compressed=[];ended=False;idat_ended=False;seen_idat=False;seen_palette=False;count=0
    while pos<len(raw):
        if pos+12>len(raw):raise ValueError("truncated chunk")
        size=struct.unpack_from(">I",raw,pos)[0];kind=raw[pos+4:pos+8];end=pos+12+size
        if size>MAX_PNG or end>len(raw):raise ValueError("truncated/oversized chunk")
        data=raw[pos+8:pos+8+size];crc=struct.unpack_from(">I",raw,pos+8+size)[0]
        if zlib.crc32(kind+data)&0xffffffff!=crc:raise ValueError("PNG CRC mismatch")
        if any(not 65<=x<=90 and not 97<=x<=122 for x in kind) or kind[2]&32:raise ValueError("invalid PNG chunk type")
        count+=1
        if count>10000:raise ValueError("too many PNG chunks")
        if header is None and kind!=b"IHDR":raise ValueError("IHDR must be first")
        if kind==b"IHDR":
            if header is not None or size!=13:raise ValueError("duplicate/bad IHDR")
            w,h,depth,color,compression,filtering,interlace=struct.unpack(">IIBBBBB",data)
            channels={0:1,2:3,4:2,6:4}.get(color)
            if not(0<w<=4096 and 0<h<=4096) or depth!=8 or channels is None or compression or filtering or interlace:raise ValueError("unsupported/oversized PNG encoding")
            header=(w,h,channels)
        elif kind==b"IDAT":
            if idat_ended:raise ValueError("noncontiguous IDAT")
            seen_idat=True;compressed.append(data)
        elif kind==b"IEND":
            if size or not seen_idat or end!=len(raw):raise ValueError("bad IEND/trailing bytes")
            ended=True;break
        else:
            if seen_idat:idat_ended=True
            # PLTE is optional only for RGB/RGBA here; reject malformed palettes.
            if kind==b"PLTE":
                if seen_palette or seen_idat or header[2] in (1,2) or not 0<size<=768 or size%3:raise ValueError("bad PLTE")
                seen_palette=True
            elif not kind[0]&32:raise ValueError("unknown critical chunk")
            else:raise ValueError("unsupported ancillary PNG chunk")
        pos=end
    if not ended or not header:raise ValueError("incomplete PNG")
    w,h,channels=header;stride=1+w*channels;expected=stride*h
    if expected>MAX_PNG:raise ValueError("decoded PNG too large")
    try:
        decoder=zlib.decompressobj();pixels=decoder.decompress(b"".join(compressed),expected+1)
    except zlib.error as exc:raise ValueError("corrupt PNG zlib stream") from exc
    if len(pixels)!=expected or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail:raise ValueError("truncated/extra/oversized PNG pixel data")
    if any(pixels[i]>4 for i in range(0,expected,stride)):raise ValueError("invalid scanline filter")
    return w,h


def extract(block):
    """Only declared MCP envelope levels; never search nested arbitrary metadata."""
    content=block.get("content");error=block.get("is_error") is True
    for _ in range(3):
        if isinstance(content,str):
            if len(content)>MAX_LINE:return None
            try:obj=json.loads(content)
            except ValueError:return None
            if not isinstance(obj,dict):return None
            if "content" not in obj:return dict(meta=obj,images=[],error=error)
            error |= obj.get("isError") is True;content=obj["content"];continue
        if not isinstance(content,list):return None
        texts=[x for x in content if isinstance(x,dict) and x.get("type")=="text"]
        images=[x for x in content if isinstance(x,dict) and x.get("type")=="image"]
        if len(texts)!=1 or not isinstance(texts[0].get("text"),str):return None
        text=texts[0]["text"]
        if len(text)>MAX_LINE:return None
        try:obj=json.loads(text)
        except ValueError:return None
        if not isinstance(obj,dict):return None
        if "content" in obj:
            if images:return None
            error |= obj.get("isError") is True;content=obj["content"];continue
        return dict(meta=obj,images=images,error=error)
    return None


def trace_data(rows):
    calls=[];results={};models=[];valid=True;terminal=False
    for row in rows:
        if row.get("type")=="result":terminal |= row.get("subtype")=="success" and row.get("is_error") is False
        message=row.get("message",{})
        if not isinstance(message,dict):continue
        if row.get("type")=="assistant":
            model=message.get("model");models.append(model);valid &= model==MODEL
        blocks=message.get("content",[])
        if not isinstance(blocks,list):continue
        time=ts(row.get("timestamp",row.get("ts")))
        for block in blocks:
            if not isinstance(block,dict):continue
            if row.get("type")=="assistant" and block.get("type")=="tool_use":
                name=block.get("name","");short=name.split("__")[-1] if isinstance(name,str) else ""
                valid &= short in TOOLS
                inp=block.get("input");cid=block.get("id")
                valid &= isinstance(inp,dict) and isinstance(cid,str) and bool(cid) and time is not None
                calls.append(dict(id=cid,name=short,inp=inp if isinstance(inp,dict) else {},start=time))
            elif row.get("type")=="user" and block.get("type")=="tool_result":
                cid=block.get("tool_use_id")
                if not isinstance(cid,str):valid=False;continue
                results.setdefault(cid,[]).append(dict(end=time,payload=extract(block)))
    counts=Counter(c["id"] for c in calls if isinstance(c["id"],str))
    requests=Counter((c["inp"].get("session_id"),c["inp"].get("request_id")) for c in calls if c["name"]=="computer_step" and all(isinstance(c["inp"].get(k),str) for k in ("session_id","request_id")))
    for c in calls:
        rs=results.get(c["id"],[]) if isinstance(c["id"],str) else []
        c["result"]=rs[0] if len(rs)==1 and counts[c["id"]]==1 else None
        r=c["result"]
        valid &= bool(r and r["payload"] and r["end"] is not None and c["start"] is not None and c["start"]<=r["end"])
        if c["name"]=="computer_step":
            valid &= all(isinstance(c["inp"].get(k),str) and c["inp"][k] for k in ("session_id","request_id","based_on"))
            key=(c["inp"].get("session_id"),c["inp"].get("request_id"))
            valid &= all(isinstance(v,str) for v in key) and requests.get(key,0)==1
    valid &= all(cid in counts for cid in results)
    return dict(calls=calls,valid=bool(valid and terminal and models),models=sorted({m for m in models if isinstance(m,str)}))


def result(c):return c["result"]["payload"]["meta"]

def error(c):
    m=result(c)
    return c["result"]["payload"]["error"] or m.get("is_error") is True or m.get("step_is_error") is True or isinstance(m.get("error"),dict)


def observation(c):
    if error(c):return None
    m=result(c);o=m.get("observation",m)
    if not isinstance(o,dict) or not all(isinstance(o.get(k),str) and o[k] for k in ("session_id","observation_id","surface_id","geometry_version","topology_generation")):return None
    if not all(integer(o.get(k)) and o[k]>0 for k in ("width_px","height_px")):return None
    images=c["result"]["payload"]["images"]
    if len(images)!=1:return None
    image=images[0]
    if image.get("mimeType")=="image/png":encoded=image.get("data")
    else:
        source=image.get("source",{})
        encoded=source.get("data") if isinstance(source,dict) and source.get("type")=="base64" and source.get("media_type")=="image/png" else None
    try:size=png_size(encoded)
    except ValueError:return None
    if size!=(o["width_px"],o["height_px"]):return None
    return dict(o,time=c["result"]["end"],call_id=c["id"])


def rect(r):return isinstance(r,dict) and all(integer(r.get(k)) for k in ("x","y","width","height")) and r["width"]>0 and r["height"]>0


def mapped(o,p):
    if not isinstance(p,list) or len(p)!=2 or not all(integer(v) and 0<=v<o[k] for v,k in zip(p,("width_px","height_px"))):return None
    if o.get("native_unit")!="physical_pixels":return None
    regions=o.get("mapping_regions")
    if not isinstance(regions,list) or not 0<len(regions)<=64:return None
    matches=[]
    for r in regions:
        if not isinstance(r,dict) or not rect(r.get("image_rect")) or not rect(r.get("native_bounds")):return None
        i,n=r["image_rect"],r["native_bounds"]
        if i["x"]<0 or i["y"]<0 or i["x"]+i["width"]>o["width_px"] or i["y"]+i["height"]>o["height_px"]:return None
        if i["x"]<=p[0]<i["x"]+i["width"] and i["y"]<=p[1]<i["y"]+i["height"]:
            matches.append(tuple(n[k]+math.floor((p[j]-i[k])*n[axis]/i[axis]) for j,(k,axis) in enumerate((("x","width"),("y","height")))))
    return matches[0] if len(matches)==1 else None


def raw_valid(e):
    fields=("native_message","raw_wparam","raw_lparam","native_timestamp_ms","event_index","own_handle","client_x","client_y","screen_x","screen_y")
    if not all(integer(e.get(k)) for k in fields) or e["own_handle"]==0 or not 0<=e["native_timestamp_ms"]<=0xffffffff:return False
    if e.get("source")!="queue/own-HWND" or e.get("physical_coordinates") is not True:return False
    table={0x200:"move",0x201:"down",0x202:"up",0x203:"down",0x204:"down",0x205:"up",0x206:"down",0x207:"down",0x208:"up",0x209:"down",0x100:"key_down",0x101:"key_up",0x102:"char",0x104:"key_down",0x105:"key_up",0x106:"char",0x20a:"wheel",0x20e:"wheel"}
    if table.get(e["native_message"])!=e.get("kind"):return False
    if e["kind"] in ("move","down","up"):
        signed=lambda v:(v&65535)-(65536 if v&32768 else 0)
        if e["client_x"]!=signed(e["raw_lparam"]) or e["client_y"]!=signed(e["raw_lparam"]>>16):return False
        if e["native_message"]==0x201 and e["raw_wparam"]&1==0 or e["native_message"]==0x202 and e["raw_wparam"]&1:return False
    return isinstance(e.get("role"),str) and bool(e["role"])


def successful_step(c):
    m=result(c)
    return not error(c) and m.get("request_id")==c["inp"].get("request_id") and m.get("input_outcome")=="dispatched" and m.get("cleanup_outcome") in ("not_needed","released")


def basis(c,observations,calls):
    available=[o for o in observations if o["session_id"]==c["inp"].get("session_id") and o["time"]<=c["start"]]
    matching=[o for o in available if o["observation_id"]==c["inp"].get("based_on")]
    if len(matching)!=1:return None
    o=matching[0]
    if any(x["time"]>o["time"] for x in available):return None
    if any(x is not c and x["name"]=="computer_step" and x["inp"].get("session_id")==o["session_id"] and o["time"]<x["start"]<c["start"] for x in calls):return None
    return o


def click_evidence(c,o,events,role):
    a=c["inp"].get("action")
    if not isinstance(a,dict) or a.get("kind")!="click" or a.get("button","left")!="left" or a.get("count",1)!=1 or not successful_step(c):return False
    p=mapped(o,a.get("position"))
    if p is None:return False
    es=[e for e in events if e.get("type")=="input_event" and c["start"]<=ts(e["ts"])<=c["result"]["end"] and e.get("role")==role and e.get("kind") in ("down","up")]
    if len(es)!=2 or [e.get("native_message") for e in es]!=[0x201,0x202]:return False
    if not all(raw_valid(e) and abs(e["screen_x"]-p[0])<=1 and abs(e["screen_y"]-p[1])<=1 for e in es):return False
    if es[0]["own_handle"]!=es[1]["own_handle"] or es[0]["event_index"]>=es[1]["event_index"] or es[0]["layout_generation"]!=es[1]["layout_generation"]:return False
    if role=="target":
        return all(e["own_handle"]==e.get("target_handle") and e["target_x"]<=e["screen_x"]<e["target_x"]+e["target_width"] and e["target_y"]<=e["screen_y"]<e["target_y"]+e["target_height"] for e in es)
    return True


def fit_proof(o,op):
    """Single selected physical display only; multi-region GUI stays unknown here."""
    regions=o.get("mapping_regions")
    if not isinstance(regions,list) or len(regions)!=1:return False
    r=regions[0]
    if not isinstance(r,dict) or not rect(r.get("native_bounds")) or not rect(r.get("image_rect")):return False
    n,i=r["native_bounds"],r["image_rect"]
    if o.get("native_unit")!="physical_pixels" or o.get("selected_display_ids")!=[r.get("display_id")]:return False
    mw,mh=op["inp"].get("max_width",1366),op["inp"].get("max_height",768)
    if not all(integer(v) and 16<=v<=4096 for v in (mw,mh)):return False
    scale=min(1,mw/n["width"],mh/n["height"])
    w=max(16,math.floor(n["width"]*scale+0.5));h=max(16,math.floor(n["height"]*scale+0.5))
    return (o["width_px"],o["height_px"])==(w,h) and i==dict(x=0,y=0,width=w,height=h)


def assess(start,events,checks,bindings,data):
    n=int(start["case_id"].split("-")[-1]);unknown=("unknown","missing, ambiguous or inconsistent proof")
    if n==6:return ("not_gui","pure fixture mapping is not production/physical display proof")
    if n==5 and start.get("dpi_reliable") is not True:return ("needs_capability","reliable GetDpiForWindow / own PMv2 HWND evidence unavailable")
    if n==5 and not(integer(start.get("dpi")) and start["dpi"]>96):return ("needs_environment","actual reliable HWND DPI >96 required")
    if n in (7,8) and not any(e.get("type")=="environment_notification" and e.get("environment_changed") is True for e in events):return ("needs_environment","manual real environment change not evidenced")
    if "check_index" not in checks or not data["valid"] or len(bindings)!=1:return unknown
    binding=bindings[0]
    ids=binding.get("call_ids")
    if binding.get("trace_complete") is not True or binding.get("reviewer")!="CC" or not isinstance(ids,list) or not ids or any(not isinstance(i,str) for i in ids) or len(set(ids))!=len(ids):return unknown
    calls=[c for c in data["calls"] if c["id"] in ids]
    if len(calls)!=len(ids):return unknown
    if any(c["start"]<ts(start["ts"]) for c in calls):return unknown
    opens=[c for c in calls if c["name"]=="computer_open"]
    if not opens:return unknown
    sessions=[result(c).get("session_id") for c in opens]
    if any(not isinstance(s,str) or not s for s in sessions) or len(set(sessions))!=len(sessions):return unknown
    if any(c["inp"].get("session_id") in sessions and c not in calls for c in data["calls"]):return unknown
    if any(c["name"] not in ("computer_open","computer_describe") and c["inp"].get("session_id") not in sessions for c in calls):return unknown
    closes=[c for c in calls if c["name"]=="computer_close"]
    for session in sessions:
        close=[c for c in closes if c["inp"].get("session_id")==session]
        if len(close)!=1:return unknown
        c=close[0];m=result(c)
        if error(c) or m.get("session_id")!=session or m.get("state")!="closed" or m.get("cleanup_outcome") not in ("not_needed","released"):return unknown
        if any(x is not c and x["inp"].get("session_id")==session and x["start"]>=c["start"] for x in calls):return unknown
    end=max(c["result"]["end"] for c in closes)
    hearts=[e for e in events if e.get("type")=="heartbeat" and ts(e["ts"])>=end]
    if len(hearts)<2 or max(ts(e["ts"]) for e in hearts)-min(ts(e["ts"]) for e in hearts)<0.4:return unknown
    if any(c["result"]["end"]>end for c in calls):return unknown
    begin=min(c["start"] for c in calls)
    if any(begin<=c["start"]<=end and c not in calls for c in data["calls"]):return unknown
    observations=[]
    for c in calls:
        if c["name"] in ("computer_open","computer_observe","computer_step"):
            o=observation(c)
            if o is not None:
                if c["name"]!="computer_open" and o["session_id"]!=c["inp"].get("session_id"):return unknown
                observations.append(o)
    if len({(o["session_id"],o["observation_id"]) for o in observations})!=len(observations):return unknown
    steps=[c for c in calls if c["name"]=="computer_step"]
    if not steps:return unknown
    # No unmatched native input may be hidden behind a purportedly complete trace.
    inputs=[e for e in events if e.get("type")=="input_event" and ts(start["ts"])<=ts(e["ts"])<=end]
    if any(not raw_valid(e) for e in inputs):return unknown
    if len({e["event_index"] for e in inputs})!=len(inputs) or any(a["event_index"]>=b["event_index"] for a,b in zip(inputs,inputs[1:])):return unknown
    if any(e.get("kind")!="move" and not any(c["start"]<=ts(e["ts"])<=c["result"]["end"] for c in steps) for e in inputs):return unknown
    if n in (7,8):return environment_case(n,start,events,binding,calls,observations,steps,hearts,end,ts(checks["ts"]))
    if any(not successful_step(c) for c in steps):return unknown
    if n==9:
        removed=[e for e in events if e.get("type")=="target_removed"]
        if len(removed)!=1:return unknown
        removal=removed[0];rt=ts(removal["ts"])
        if removal.get("target_present") is not False or removal.get("layout_generation",0)<=start.get("layout_generation",0):return unknown
        chosen=[c for c in steps if c["start"]<=rt<=c["result"]["end"]]
        if len(chosen)!=1 or len(steps)!=1:return unknown
        c=chosen[0];o=basis(c,observations,calls)
        if not o or not click_evidence(c,o,inputs,"remove"):return unknown
        post=[o for o in observations if o["time"]>rt and o["observation_id"]==binding.get("target_absent_observation_id")]
        if len(post)!=1 or post[0]["time"]>ts(checks["ts"]) or binding.get("target_absence_visually_reviewed") is not True:return unknown
        if any(e["kind"]!="move" and ts(e["ts"])>rt for e in inputs):return unknown
        if any(c["name"]=="computer_close" and c["start"]<=post[0]["time"] for c in closes):return unknown
        if not all(h.get("target_present") is False and h.get("layout_generation",0)>=removal["layout_generation"] for h in hearts):return unknown
        return ("diagnostic_match","owned target disappearance + explicit CC visual binding + complete tool stop/Close; NOT global window safety")
    if len(opens)!=1:return unknown
    # Every dispatched action must be a witnessed owned TARGET or Check click.
    # An extra successful step on some other window is not silently ignored.
    for step in steps:
        prior=basis(step,observations,calls)
        if not prior or not any(click_evidence(step,prior,inputs,role) for role in ("target","check")):return unknown
    op=opens[0]
    desired={1:(4096,4096),2:(1920,1920),3:(1440,1440),4:(1000,700)}.get(n)
    if desired and (op["inp"].get("max_width"),op["inp"].get("max_height"))!=desired:return unknown
    cutoff=ts(checks["ts"])
    chosen=[]
    for c in steps:
        if c["result"]["end"]>cutoff:continue
        o=basis(c,observations,calls)
        if o and click_evidence(c,o,inputs,"target"):chosen.append((c,o))
    if len(chosen)!=1:return unknown
    c,o=chosen[0]
    if not fit_proof(o,op):return unknown
    region=o["mapping_regions"][0]["native_bounds"]
    # Independent app display mode vs observation native bounds, not JSON self-compare.
    if any(start.get(k)!=region[v] for k,v in (("display_width","width"),("display_height","height"),("display_x","x"),("display_y","y"))):return unknown
    if n==1 and (o["width_px"],o["height_px"])!=(region["width"],region["height"]):return ("needs_capability","4096 tool cap cannot return this display at scale 1")
    if n==4 and (o["width_px"],o["height_px"])==(region["width"],region["height"]):return ("needs_environment","capture too small to exercise real downscale")
    if not any(x["time"]>=c["result"]["end"] and x["time"]<=cutoff and x["observation_id"]!=o["observation_id"] for x in observations):return unknown
    return ("diagnostic_match","real PNG + explicit limits + physical per-region mapping + owned DOWN/UP; CC visual acceptance still separate")


def environment_case(n,start,events,binding,calls,observations,steps,hearts,end,cutoff):
    unknown=("unknown","manual change needs native notification, supervisor attestation, pre/post observations, rejected old basis and zero dispatch")
    notifications=[e for e in events if e.get("type")=="environment_notification" and e.get("environment_changed") is True]
    proof=binding.get("environment_change")
    if not isinstance(proof,dict) or proof.get("actual_change") is not True or proof.get("reviewer")!="CC":return unknown
    notifications=[e for e in notifications if e.get("notification_id")==proof.get("notification_id") and isinstance(e.get("notification_id"),str)]
    if len(notifications)!=1 or len(steps)!=1:return unknown
    e=notifications[0];et=ts(e["ts"]);c=steps[0];m=result(c)
    key="resolution_changed" if n==7 else "dpi_changed"
    if e.get("native_message")!=(0x7e if n==7 else 0x2e0) or e.get(key) is not True or e.get("before_device")!=e.get("display_device"):return unknown
    if n==7:
        if not all(integer(e.get(k)) and e[k]>0 for k in ("before_width","before_height","display_width","display_height")) or (e["before_width"],e["before_height"])==(e["display_width"],e["display_height"]):return unknown
    elif not(e.get("dpi_reliable") is True and integer(e.get("before_dpi")) and e["before_dpi"]>0 and e["before_dpi"]!=e.get("dpi")):return unknown
    old=[o for o in observations if o["observation_id"]==c["inp"].get("based_on") and o["session_id"]==c["inp"].get("session_id") and o["time"]<et]
    new=[o for o in observations if o["observation_id"]==proof.get("after_observation_id") and o["time"]>et]
    if len(old)!=1 or len(new)!=1 or old[0]["observation_id"]!=proof.get("before_observation_id"):return unknown
    if new[0]["time"]>cutoff or c["result"]["end"]>cutoff:return unknown
    if e.get("before_device")!=start.get("display_device"):return unknown
    for o,app in ((old[0],start),(new[0],e)):
        regions=o.get("mapping_regions")
        if o.get("native_unit")!="physical_pixels" or not isinstance(regions,list) or len(regions)!=1:return unknown
        bounds=regions[0].get("native_bounds")
        if not rect(bounds) or any(bounds[k]!=app.get(f) for k,f in (("x","display_x"),("y","display_y"),("width","display_width"),("height","display_height"))):return unknown
    if old[0]["geometry_version"]==new[0]["geometry_version"] or old[0]["topology_generation"]==new[0]["topology_generation"]:return unknown
    a=c["inp"].get("action",{})
    if not isinstance(a,dict) or a.get("kind")!="click" or mapped(old[0],a.get("position")) is None or c["start"]<=et:return unknown
    if not error(c) or m.get("error",{}).get("code")!="geometry_changed" or m.get("input_outcome")!="not_started" or m.get("request_id")!=c["inp"].get("request_id") or m.get("events_completed")!=0 or m.get("cleanup_outcome")!="not_needed":return unknown
    if any(x.get("type")=="input_event" and x.get("kind")!="move" and et<=ts(x["ts"])<=end for x in events):return unknown
    if not all(h.get("layout_generation",0)>=e.get("layout_generation",0) for h in hearts):return unknown
    return ("diagnostic_match","manual environment change + exact stale request rejection/zero dispatch + post-change image; not an automatic environment test")


def analyze(events,trace,supervisor):
    data=trace_data(trace);starts=[e for e in events if e.get("type")=="trial_start"]
    rows=[];counts=Counter((e.get("run_id"),e.get("case_id")) for e in starts)
    bindings=[b for b in supervisor if b.get("type")=="geometry_binding"]
    # A call cannot be borrowed by another case's binding.
    owners=Counter(cid for b in bindings if isinstance(b.get("call_ids"),list) for cid in b["call_ids"] if isinstance(cid,str))
    if any(owners.get(c["id"],0)!=1 for c in data["calls"] if isinstance(c["id"],str)):data["valid"]=False
    for start in starts:
        key=identity(start);cid=start.get("case_id")
        output=dict(case_id=cid,nonce=start.get("nonce"),first="unknown",final="unknown",reason="invalid identity, duplicate case or nonmonotonic evidence")
        rows.append(output)
        if not key or key[2]!="geometry" or cid not in {"geometry-%02d"%i for i in range(1,11)} or counts[(start.get("run_id"),cid)]!=1:continue
        n=int(cid[-2:])
        if start.get("spec")!=SPECS[n-1]:continue
        es=[e for e in events if identity(e)==key]
        if any(ts(e.get("ts")) is None or not number(e.get("t_ms")) for e in es):continue
        if any(ts(a["ts"])>ts(b["ts"]) or a["t_ms"]>b["t_ms"] for a,b in zip(es,es[1:])):continue
        if any(e.get("type")=="trial_start" and e is not start for e in es):continue
        bs=[b for b in bindings if identity(b)==key]
        if any(not isinstance(b.get("call_ids"),list) or any(not isinstance(i,str) or owners.get(i,0)!=1 for i in b["call_ids"]) for b in bs):continue
        checks=[e for e in es if e.get("type")=="check"]
        if checks and ([c.get("check_index") for c in checks]!=list(range(1,len(checks)+1))):continue
        if not checks:checks=[dict(start)]
        try:
            first=assess(start,es,checks[0],bs,data);final=assess(start,es,checks[-1],bs,data)
        except (KeyError,TypeError,ValueError,OverflowError):
            first=final=("unknown","malformed proof; no inference from incomplete fields")
        output.update(first=first[0],final=final[0],first_reason=first[1],reason=final[1],check_count=len([e for e in es if e.get("type")=="check"]))
    return dict(schema="windows-geometry-analysis-v1",cases=rows,observed_cases=len(rows),expected_cases=10,
                diagnostic_matches=sum(r["final"]=="diagnostic_match" for r in rows),completion_rate=None,
                gui_verified=False,ten_valid_trials_per_action_gate=False,models=data["models"],trace_complete_and_policy_ok=data["valid"],
                disclaimer="Diagnostic only. First/final separate; partial cases never report 100%. Pure mapping is not GUI. CC visual and ten-first-valid-trials gates remain pending.")


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--events",required=True);p.add_argument("--trace",required=True);p.add_argument("--supervisor",required=True);p.add_argument("--output")
    a=p.parse_args()
    try:r=analyze(read_rows(a.events),read_rows(a.trace),read_rows(a.supervisor))
    except (OSError,ValueError,UnicodeError,RecursionError) as exc:r=dict(error=str(exc),gui_verified=False,completion_rate=None)
    text=json.dumps(r,ensure_ascii=False,indent=2)+"\n"
    if a.output:
        with Path(a.output).open("x",encoding="utf-8") as f:f.write(text)
    else:print(text,end="")
    return 2 if "error" in r else 0


if __name__=="__main__":raise SystemExit(main())
