"""Synthetic, pure regression source for CC. These are NOT GUI trials."""
import base64
import copy
import importlib.util
import json
from pathlib import Path
import struct
import unittest
import zlib

S = importlib.util.spec_from_file_location("geometry_analyzer", Path(__file__).resolve().parents[1]/"scripts/analyze-geometry-gui.py")
A = importlib.util.module_from_spec(S)
S.loader.exec_module(A)


def png(w=64, h=48, filter_byte=0):
    def chunk(k, d):
        return struct.pack(">I", len(d))+k+d+struct.pack(">I", zlib.crc32(k+d)&0xffffffff)
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR", struct.pack(">IIBBBBB", w,h,8,2,0,0,0))+chunk(b"IDAT",zlib.compress((bytes([filter_byte])+b"\0"*(w*3))*h))+chunk(b"IEND",b"")


def fixture(n=1, native=None, image_size=None):
    iw,ih,nw,nh=(1000,700,2000,1400) if n==4 else (64,48,64,48)
    if native is not None:nw,nh=native
    if image_size is not None:iw,ih=image_size
    x,y=nw//2,nh//2
    ident=dict(run_id="run",session_id="app",suite="geometry",case_id="geometry-%02d"%n,nonce="abc123",trial=n)
    common=dict(ident,schema="windows-geometry-v1",layout_generation=1,window_handle=10,pid=42,dpi=144 if n==5 else 96,dpi_reliable=True,physical_coordinates=True,
                display_device="DISPLAY1",display_width=nw,display_height=nh,display_x=0,display_y=0,target_handle=11,target_x=x-8,target_y=y-8,target_width=16,target_height=16)
    def row(record_type,t,**kw):
        return dict(common,type=record_type,ts=t,t_ms=(t-100)*1000,**kw)
    events=[row("trial_start",100,spec=A.SPECS[n-1],mode="manual_environment" if n in (5,7,8) else "pure_mapping" if n==6 else "gui",target_present=True)]
    trace=[]
    def obs(oid="o1",version="g1",ww=iw,hh=ih):
        return dict(session_id="s1",observation_id=oid,surface_id="primary",geometry_version=version,topology_generation=version,input_sequence=0 if oid=="o1" else 1,
                    width_px=ww,height_px=hh,native_unit="physical_pixels",selected_display_ids=["d1"],mapping_regions=[dict(display_id="d1",native_bounds=dict(x=0,y=0,width=nw,height=nh),image_rect=dict(x=0,y=0,width=ww,height=hh))])
    def call(cid,name,inp,start,end,data,image=None,error=False):
        trace.append(dict(type="assistant",timestamp=start,message=dict(model=A.MODEL,content=[dict(type="tool_use",id=cid,name="mcp__computer__"+name,input=inp)])))
        content=[dict(type="text",text=json.dumps(data))]
        if image is not None:content.append(dict(type="image",mimeType="image/png",data=base64.b64encode(image).decode()))
        trace.append(dict(type="user",timestamp=end,message=dict(content=[dict(type="tool_result",tool_use_id=cid,content=content,is_error=error)])))
    limit={1:(4096,4096),2:(1920,1920),3:(1440,1440),4:(1000,700)}.get(n,(1366,768))
    call("open","computer_open",dict(max_width=limit[0],max_height=limit[1]),101,102,dict(session_id="s1",state="ready",max_image_px=4096))
    call("pre","computer_observe",dict(session_id="s1"),102.1,102.5,obs(),png(iw,ih))
    action=dict(kind="click",position=[iw//2,ih//2],button="left",count=1)
    call("step","computer_step",dict(session_id="s1",request_id="r1",based_on="o1",action=action),103,106,
         dict(request_id="r1",input_outcome="dispatched",cleanup_outcome="not_needed",events_completed=3,events_total=3))
    for typ,t,idx,msg,wp in [("down",104,1,0x201,1),("up",105,2,0x202,0)]:
        events.append(row("input_event",t,event_index=idx,role="target",own_handle=11,source="queue/own-HWND",kind=typ,native_message=msg,raw_wparam=wp,
                          raw_lparam=(8<<16)|8,native_timestamp_ms=int(t*1000),client_x=8,client_y=8,screen_x=x,screen_y=y))
    call("observe","computer_observe",dict(session_id="s1"),106.1,106.5,obs("o2"),png(iw,ih))
    events.append(row("check",107,check_index=1,local_state="pending_tool_evidence",hit=True,target_present=True,trigger="visible_check"))
    call("close","computer_close",dict(session_id="s1"),108,109,dict(session_id="s1",state="closed",cleanup_outcome="not_needed"))
    events.extend([row("heartbeat",110,target_present=True,hit=True),row("heartbeat",111,target_present=True,hit=True)])
    trace.append(dict(type="result",subtype="success",is_error=False))
    bindings=[dict(ident,type="geometry_binding",trace_complete=True,call_ids=["open","pre","step","observe","close"],reviewer="CC")]
    return events,trace,bindings


def use(trace,cid):
    return next(b for r in trace for b in r.get("message",{}).get("content",[]) if b.get("type")=="tool_use" and b.get("id")==cid)


def meta(trace,cid):
    block=next(b for r in trace for b in r.get("message",{}).get("content",[]) if b.get("tool_use_id")==cid)
    return block,json.loads(block["content"][0]["text"])


def change_meta(trace,cid,**kw):
    b,m=meta(trace,cid);m.update(kw);b["content"][0]["text"]=json.dumps(m)



def removal_fixture():
    e,t,b=fixture(9)
    for r in e:
        if r.get("type")=="input_event":r.update(role="remove",own_handle=12)
        if r["ts"]>=105.1:r.update(layout_generation=2,target_present=False)
    removal=dict(e[0],type="target_removed",ts=105.1,t_ms=5100,layout_generation=2,target_present=False)
    e.insert(3,removal)
    b[0].update(target_absent_observation_id="o2",target_absence_visually_reviewed=True)
    return e,t,b


def environment_fixture(n=7):
    e,t,b=fixture(n)
    e=[r for r in e if r["type"]!="input_event"]
    note=dict(e[0],type="environment_notification",ts=103,t_ms=3000,layout_generation=2,notification_id="notification-1",native_message=0x7e if n==7 else 0x2e0,
              before_device="DISPLAY1",before_width=64,before_height=48,before_dpi=96,resolution_changed=n==7,dpi_changed=n==8,environment_changed=True)
    if n==7:note["display_width"]=128
    else:note["dpi"]=144
    for r in e[1:]:r.update(layout_generation=2,display_width=note["display_width"],dpi=note["dpi"])
    e.insert(1,note)
    def pair(cid,newid,start,end):
        rows=copy.deepcopy([r for r in t if any(x.get("id")==cid or x.get("tool_use_id")==cid for x in r.get("message",{}).get("content",[]))])
        rows[0]["timestamp"]=start;rows[1]["timestamp"]=end
        rows[0]["message"]["content"][0]["id"]=newid;rows[1]["message"]["content"][0]["tool_use_id"]=newid
        return rows
    step=pair("step","step",104,104.5)
    change_meta(step,"step",input_outcome="not_started",events_completed=0,events_total=0,error=dict(code="geometry_changed"));meta(step,"step")[0]["is_error"]=True
    close1=pair("close","close1",104.6,104.7)
    open2=pair("open","open2",105,105.2);change_meta(open2,"open2",session_id="s2")
    post=pair("pre","postchange",105.3,105.5);use(post,"postchange")["input"]["session_id"]="s2"
    block,m=meta(post,"postchange");m.update(session_id="s2",observation_id="o2",geometry_version="g2",topology_generation="g2",width_px=128 if n==7 else 64)
    m["mapping_regions"][0]["native_bounds"]["width"]=m["width_px"];m["mapping_regions"][0]["image_rect"]["width"]=m["width_px"]
    block["content"][0]["text"]=json.dumps(m);block["content"][1]["data"]=base64.b64encode(png(m["width_px"],48)).decode()
    close2=pair("close","close",108,109);use(close2,"close")["input"]["session_id"]="s2";change_meta(close2,"close",session_id="s2")
    t=t[:4]+step+close1+open2+post+close2+[t[-1]]
    b[0]["call_ids"]=["open","pre","step","close1","open2","postchange","close"]
    b[0]["environment_change"]=dict(actual_change=True,reviewer="CC",notification_id="notification-1",before_observation_id="o1",after_observation_id="o2")
    return e,t,b


class GeometryTests(unittest.TestCase):
    def status(self,data):return A.analyze(*data)["cases"][0]["final"]
    def test_positive_native(self):self.assertEqual(self.status(fixture()),"diagnostic_match")
    def test_bounds(self):
        for n in (2,3,4,5,10):self.assertEqual(self.status(fixture(n)),"diagnostic_match",n)
    def test_no_gui_claim(self):
        r=A.analyze(*fixture());self.assertFalse(r["gui_verified"]);self.assertFalse(r["ten_valid_trials_per_action_gate"]);self.assertIsNone(r["completion_rate"])
    def test_missing_png(self):
        e,t,b=fixture(10);meta(t,"pre")[0]["content"].pop();self.assertEqual(self.status((e,t,b)),"unknown")
    def test_mismatched_png(self):
        e,t,b=fixture(10);meta(t,"pre")[0]["content"][1]["data"]=base64.b64encode(png(65,48)).decode();self.assertEqual(self.status((e,t,b)),"unknown")
    def test_truncated_png(self):
        e,t,b=fixture(10);meta(t,"pre")[0]["content"][1]["data"]=base64.b64encode(png()[:-5]).decode();self.assertEqual(self.status((e,t,b)),"unknown")
    def test_crc_corrupt(self):
        p=bytearray(png());p[30]^=1
        with self.assertRaises(ValueError):A.png_size(base64.b64encode(p).decode())
    def test_bad_filter(self):
        with self.assertRaises(ValueError):A.png_size(base64.b64encode(png(filter_byte=5)).decode())
    def test_bad_base64(self):
        with self.assertRaises(ValueError):A.png_size("!notbase64!")
    def test_png_trailing(self):
        with self.assertRaises(ValueError):A.png_size(base64.b64encode(png()+b"junk").decode())
    def test_png_good(self):self.assertEqual(A.png_size(base64.b64encode(png()).decode()),(64,48))
    def test_png_missing_idat(self):
        p=png();p=p[:33]+p[-12:]
        with self.assertRaises(ValueError):A.png_size(base64.b64encode(p).decode())
    def test_missing_nonce(self):
        e,t,b=fixture();e[0].pop("nonce");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_bad_binding_nonce(self):
        e,t,b=fixture();b[0]["nonce"]="foreign";self.assertEqual(self.status((e,t,b)),"unknown")
    def test_duplicate_case(self):
        e,t,b=fixture();e.append(dict(e[0]));self.assertEqual(self.status((e,t,b)),"unknown")
    def test_foreign_tool(self):
        e,t,b=fixture();t.insert(0,dict(type="assistant",timestamp=100,message=dict(model=A.MODEL,content=[dict(type="tool_use",id="bad",name="Bash",input={})])));self.assertEqual(self.status((e,t,b)),"unknown")
    def test_wrong_model(self):
        e,t,b=fixture();t[0]["message"]["model"]="other";self.assertEqual(self.status((e,t,b)),"unknown")
    def test_no_model(self):
        e,t,b=fixture();t[0]["message"].pop("model");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_duplicate_call(self):
        e,t,b=fixture();t.append(copy.deepcopy(t[0]));self.assertEqual(self.status((e,t,b)),"unknown")
    def test_duplicate_result(self):
        e,t,b=fixture();t.append(copy.deepcopy(t[1]));self.assertEqual(self.status((e,t,b)),"unknown")
    def test_nested_other_call_not_borrowed(self):
        e,t,b=fixture();block,m=meta(t,"step");block["content"][0]["text"]=json.dumps(dict(other_result=m));self.assertEqual(self.status((e,t,b)),"unknown")
    def test_wrong_request(self):
        e,t,b=fixture();change_meta(t,"step",request_id="foreign");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_failed_input(self):
        e,t,b=fixture();change_meta(t,"step",input_outcome="partial");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_missing_down(self):
        e,t,b=fixture();e.pop(1);self.assertEqual(self.status((e,t,b)),"unknown")
    def test_raw_param_mismatch(self):
        e,t,b=fixture();e[1]["raw_lparam"]=0;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_mapping_mismatch(self):
        e,t,b=fixture();e[1]["screen_x"]+=15;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_wrong_based_on(self):
        e,t,b=fixture();use(t,"step")["input"]["based_on"]="foreign";self.assertEqual(self.status((e,t,b)),"unknown")
    def test_missing_close(self):
        e,t,b=fixture();t=[r for r in t if not any(x.get("id")=="close" or x.get("tool_use_id")=="close" for x in r.get("message",{}).get("content",[]))];self.assertEqual(self.status((e,t,b)),"unknown")
    def test_missing_postclose_heartbeat(self):
        e,t,b=fixture();e=e[:-2];self.assertEqual(self.status((e,t,b)),"unknown")
    def test_incomplete_trace(self):
        e,t,b=fixture();b[0]["trace_complete"]=False;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_pure_mapping_not_gui(self):self.assertEqual(self.status(fixture(6)),"not_gui")
    def test_missing_environment(self):
        for n in (7,8):self.assertEqual(self.status(fixture(n)),"needs_environment")
    def test_no_high_dpi(self):
        e,t,b=fixture(5)
        for r in e:r["dpi"]=96
        self.assertEqual(self.status((e,t,b)),"needs_environment")
    def test_first_final_separate(self):
        e,t,b=fixture();earlier=dict(e[3],ts=102.5,t_ms=2500,check_index=1,hit=False);e[3]["check_index"]=2;e.insert(1,earlier)
        r=A.analyze(e,t,b)["cases"][0];self.assertEqual(r["first"],"unknown");self.assertEqual(r["final"],"diagnostic_match")
    def test_removal_zero_input_not_certificate(self):self.assertEqual(self.status(fixture(9)),"unknown")

    def test_removal_positive_complete_chain(self):self.assertEqual(self.status(removal_fixture()),"diagnostic_match")
    def test_removal_no_visual_binding(self):
        e,t,b=removal_fixture();b[0].pop("target_absence_visually_reviewed");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_removal_stale_generation(self):
        e,t,b=removal_fixture();e[3]["layout_generation"]=1;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_removal_no_postimage(self):
        e,t,b=removal_fixture();meta(t,"observe")[0]["content"].pop();self.assertEqual(self.status((e,t,b)),"unknown")
    def test_removal_heartbeat_target_returns(self):
        e,t,b=removal_fixture();e[-1]["target_present"]=True;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_resolution_positive_chain(self):self.assertEqual(self.status(environment_fixture()),"diagnostic_match")
    def test_dpi_positive_chain(self):self.assertEqual(self.status(environment_fixture(8)),"diagnostic_match")
    def test_change_no_supervisor_attestation(self):
        e,t,b=environment_fixture();b[0].pop("environment_change");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_change_missing_post_observation(self):
        e,t,b=environment_fixture();meta(t,"postchange")[0]["content"].pop();self.assertEqual(self.status((e,t,b)),"unknown")
    def test_change_fake_resolution(self):
        e,t,b=environment_fixture();e[1]["display_width"]=64;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_change_wrong_raw_notification(self):
        e,t,b=environment_fixture();e[1]["native_message"]=5;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_change_stale_observation_error_is_not_geometry_error(self):
        e,t,b=environment_fixture();change_meta(t,"step",error=dict(code="stale_observation"));self.assertEqual(self.status((e,t,b)),"unknown")
    def test_change_nonzero_dispatch(self):
        e,t,b=environment_fixture();change_meta(t,"step",events_completed=1);self.assertEqual(self.status((e,t,b)),"unknown")
    def test_change_wrong_request(self):
        e,t,b=environment_fixture();change_meta(t,"step",request_id="wrong");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_change_missing_topology_difference(self):
        e,t,b=environment_fixture();change_meta(t,"postchange",topology_generation="g1");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_change_no_heartbeat(self):
        e,t,b=environment_fixture();e=e[:-2];self.assertEqual(self.status((e,t,b)),"unknown")
    def test_bad_nonce_change(self):
        e,t,b=environment_fixture();e[1]["nonce"]="other";self.assertNotEqual(self.status((e,t,b)),"diagnostic_match")
    def test_no_pixel_payload_in_output(self):
        data=fixture(10);payload=meta(data[1],"pre")[0]["content"][1]["data"];self.assertNotIn(payload,json.dumps(A.analyze(*data)))
    def test_padded_gap_rejected(self):
        o=dict(width_px=100,height_px=100,native_unit="physical_pixels",mapping_regions=[dict(image_rect=dict(x=0,y=0,width=10,height=10),native_bounds=dict(x=-100,y=0,width=10,height=10))])
        self.assertIsNone(A.mapped(o,[50,50]));self.assertEqual(A.mapped(o,[0,0]),(-100,0))
    def test_duplicate_region_not_arbitrarily_selected(self):
        o=dict(width_px=100,height_px=100,native_unit="physical_pixels",mapping_regions=[dict(image_rect=dict(x=0,y=0,width=100,height=100),native_bounds=dict(x=0,y=0,width=100,height=100))]*2)
        self.assertIsNone(A.mapped(o,[50,50]))
    def test_bad_call_input_types_unknown(self):
        e,t,b=fixture();use(t,"step")["input"]["session_id"]={};self.assertEqual(self.status((e,t,b)),"unknown")
    def test_bad_binding_list_unknown(self):
        e,t,b=fixture();b[0]["call_ids"]=None;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_missing_check_unknown(self):
        e,t,b=fixture();e=[r for r in e if r["type"]!="check"];self.assertEqual(self.status((e,t,b)),"unknown")
    def test_wrong_size_limit_request(self):
        e,t,b=fixture(2);t[0]["message"]["content"][0]["input"]["max_width"]=1440;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_duplicate_observation(self):
        e,t,b=fixture();change_meta(t,"observe",observation_id="o1");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_own_window_scope_only(self):
        e,t,b=fixture();e[1]["own_handle"]=999;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_app_display_dimension_not_json_selfproof(self):
        e,t,b=fixture();e[0]["display_width"]=65;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_dpi_virtualization_unknown(self):
        e,t,b=fixture();e[1]["physical_coordinates"]=False;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_ambiguous_metadata(self):
        e,t,b=fixture();block,m=meta(t,"step");block["content"].append(dict(type="text",text=json.dumps(m)));self.assertEqual(self.status((e,t,b)),"unknown")


    def test_native_size_capped_not_pass(self):
        self.assertEqual(self.status(fixture(1,native=(8192,48),image_size=(4096,24))),"needs_capability")
    def test_aspect_without_actual_scale_needs_environment(self):
        self.assertEqual(self.status(fixture(4,native=(64,48),image_size=(64,48))),"needs_environment")
    def test_wrapped_mcp_envelope(self):
        e,t,b=fixture()
        for row in t:
            if row["type"]=="user":
                block=row["message"]["content"][0];block["content"]=json.dumps(dict(content=block["content"],isError=False))
        self.assertEqual(self.status((e,t,b)),"diagnostic_match")
    def test_png_claude_source_encoding(self):
        e,t,b=fixture()
        for cid in ("pre","observe"):
            image=meta(t,cid)[0]["content"][1]
            image["source"]=dict(type="base64",media_type="image/png",data=image.pop("data"));image.pop("mimeType")
        self.assertEqual(self.status((e,t,b)),"diagnostic_match")
    def test_png_excess_decoded_pixels(self):
        def chunk(k,d):return struct.pack(">I",len(d))+k+d+struct.pack(">I",zlib.crc32(k+d)&0xffffffff)
        raw=b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",16,16,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(b"\0"*100000))+chunk(b"IEND",b"")
        with self.assertRaises(ValueError):A.png_size(base64.b64encode(raw).decode())
    def test_png_missing_pixels_with_valid_crcs(self):
        def chunk(k,d):return struct.pack(">I",len(d))+k+d+struct.pack(">I",zlib.crc32(k+d)&0xffffffff)
        raw=b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",16,16,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(b"\0"))+chunk(b"IEND",b"")
        with self.assertRaises(ValueError):A.png_size(base64.b64encode(raw).decode())
    def test_close_failed_cleanup_unknown(self):
        e,t,b=fixture();change_meta(t,"close",cleanup_outcome="failed");self.assertEqual(self.status((e,t,b)),"unknown")
    def test_event_outside_call_unknown(self):
        e,t,b=fixture();e[1].update(ts=102.5,t_ms=2500);self.assertEqual(self.status((e,t,b)),"unknown")
    def test_duplicate_binding_unknown(self):
        e,t,b=fixture();b.append(copy.deepcopy(b[0]));self.assertEqual(self.status((e,t,b)),"unknown")
    def test_missing_terminal_unknown(self):
        e,t,b=fixture();t.pop();self.assertEqual(self.status((e,t,b)),"unknown")
    def test_bad_timestamp_unknown(self):
        e,t,b=fixture();e[1]["ts"]="no timestamp";self.assertEqual(self.status((e,t,b)),"unknown")
    def test_close_before_removal_observation_unknown(self):
        e,t,b=removal_fixture();next(r for r in t if r.get("type")=="assistant" and r["message"]["content"][0]["id"]=="close")["timestamp"]=105.8;self.assertEqual(self.status((e,t,b)),"unknown")
    def test_removal_no_input_proof_is_not_global_safety(self):
        r=A.analyze(*removal_fixture());self.assertFalse(r["gui_verified"]);self.assertIn("NOT global",r["cases"][0]["reason"])


    def test_unvalidated_ancillary_never_silently_passes(self):
        k=b"tEXt";d=b"unvalidated"
        chunk=struct.pack(">I",len(d))+k+d+struct.pack(">I",zlib.crc32(k+d)&0xffffffff)
        data=png();data=data[:33]+chunk+data[33:]
        with self.assertRaises(ValueError):A.png_size(base64.b64encode(data).decode())

if __name__=="__main__":unittest.main()
