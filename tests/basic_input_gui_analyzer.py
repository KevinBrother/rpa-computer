"""B2 synthetic regression source. Run only by CC on Windows; no GUI required."""
import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("basic_analyzer", Path(__file__).resolve().parents[1] / "scripts/analyze-basic-input-gui.py")
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


def record(record_type, t, **kw):
    r = dict(type=record_type, ts=t, utc_ms=t*1000, run_id="r", session_id="app", suite="pointer",
             case_id="pointer-04", nonce="N", trial=4, platform="windows")
    r.update(kw)
    return r


def evidence():
    events = []
    for i, (kind, msg) in enumerate((("down", 0x201), ("up", 0x202)), 1):
        events.append(record("basic_input", 2+i*.1, event_index=i, kind=kind, button="left", area="target",
            native_message=msg, raw_wparam=1 if i==1 else 0, raw_lparam=(100<<16)|120, x=120, y=100,
            native_timestamp_ms=100+i*100, t_ms=1000+i*100, source="WndProc/own-pointer", own_handle=1,
            focus_handle=1, focused=True, key_code=0, modifiers=0, active_before=True))
    return [record("basic_trial", 1, spec="left_click", platform_expected="native own HWND",
                   initial_text="", target_handle=1, observation_requirement="current", secondary_prepared_inactive=False),
            *events, record("basic_check", 4, check_index=1, check_kind="first", state="matched", event_count=2,
                           released=True, text="", selection_start=0, selection_length=0, command_count=0,
                           context_opened=False, visible_result="matched native click"),
            record("basic_trial_end", 5, final_check_index=1)]


def transcript(action=None, result=None):
    a = action or dict(kind="click", position=[120,100], button="left", count=1)
    out = result or dict(input_outcome="dispatched", cleanup_outcome="not_needed", is_error=False)
    return [dict(type="assistant", timestamp=1.5, message=dict(model="glm-5.3-flash", content=[dict(
        type="tool_use", id="call", name="mcp__computer__computer_step", input=dict(
            session_id="mcp",request_id="req",based_on="obs",action=a))])),
        dict(type="user",timestamp=3,message=dict(content=[dict(type="tool_result",tool_use_id="call",content=[
            dict(type="text",text=__import__('json').dumps(out))])]))]


class BasicAnalyzerTests(unittest.TestCase):
    def analyze(self, rows=None, trace=None, suite="pointer"):
        return MOD.analyze(evidence() if rows is None else rows, transcript() if trace is None else trace, suite)

    def test_partial_not_100(self):
        r=self.analyze(); self.assertEqual(r["first_matched"],1); self.assertEqual(r["first_pass_rate"],10)
        self.assertEqual(r["valid_input"],1); self.assertFalse(r["gui_verified"]); self.assertFalse(r["suite_complete"])

    def test_missing_is_unknown(self):
        self.assertEqual(self.analyze([],[])["status"],"unknown")

    def test_raw_mismatch(self):
        r=evidence(); r[1]["native_message"]=0x204
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_raw_coords_mismatch(self):
        r=evidence(); r[1]["x"]=121
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_wrong_case_nonce_time(self):
        for field,value in (("case_id","pointer-05"),("nonce","other"),("ts",None)):
            r=evidence(); r[1][field]=value
            self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_utc_time_mismatch(self):
        r=evidence(); r[1]["utc_ms"]=9000
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_duplicate_first(self):
        r=evidence(); r.insert(4,dict(r[3]))
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_first_final_separate(self):
        r=evidence(); r[3]["state"]="mismatch"
        r.insert(4,dict(r[3],ts=4.5,utc_ms=4500,check_index=2,check_kind="final",state="matched"))
        r[-1]["final_check_index"]=2
        out=self.analyze(r); self.assertEqual(out["first_matched"],0); self.assertEqual(out["final_matched"],1)
        self.assertEqual(out["retry_checks"],1)

    def test_fake_tool_success(self):
        for result in (dict(message="success"),dict(input_outcome="partial",cleanup_outcome="released"),
                       dict(input_outcome="dispatched",cleanup_outcome="failed")):
            self.assertEqual(self.analyze(trace=transcript(result=result))["first_matched"],0)

    def test_missing_result(self):
        self.assertEqual(self.analyze(trace=transcript()[:1])["first_matched"],0)

    def test_duplicate_tool_envelope(self):
        t=transcript(); b=copy.deepcopy(t[0]["message"]["content"][0]); b["id"]="other"
        t[0]["message"]["content"].append(b)
        b=copy.deepcopy(t[1]["message"]["content"][0]); b["tool_use_id"]="other"
        t[1]["message"]["content"].append(b)
        self.assertEqual(self.analyze(trace=t)["first_matched"],0)

    def test_model_not_verified(self):
        t=transcript(); t[0]["message"]["model"]="other"
        r=self.analyze(trace=t); self.assertFalse(r["model_verified"]); self.assertEqual(r["first_matched"],0)

    def test_padding_never_upgraded(self):
        r=evidence()
        for row in r: row.update(case_id="pointer-09",trial=9)
        r[0]["spec"]="padding_reject"
        out=self.analyze(r); self.assertEqual(out["blocked"],1); self.assertEqual(out["valid_input"],0)

    def test_rejection_zero_not_pass(self):
        r=evidence(); r=[r[0],r[3],r[4]]
        for row in r: row.update(case_id="pointer-10",trial=10)
        r[0]["spec"]="bounds_reject"; r[1].update(event_count=0,state="needs_tool_evidence")
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_invalid_suite(self):
        with self.assertRaises(ValueError): self.analyze(suite="multiclick")

    def test_image_not_traversed_or_returned(self):
        t=transcript(); t[1]["message"]["content"][0]["content"].append(dict(type="image",source=dict(data="SECRET_IMAGE")))
        self.assertNotIn("SECRET_IMAGE",str(self.analyze(trace=t)))

    def test_keyboard_hold_missing_up_and_focus(self):
        trial=record("basic_trial",1,suite="keyboard",case_id="keyboard-07",trial=7,spec="short_hold",initial_text="",platform_expected="native")
        events=[]
        for i,(msg,kind,time) in enumerate(((0x100,"key_down",100),(0x101,"key_up",220)),1):
            events.append(dict(trial,type="basic_input",ts=2+i*.12,utc_ms=(2+i*.12)*1000,event_index=i,
                kind=kind,native_message=msg,raw_wparam=16,raw_lparam=1 if i==1 else 0xc0000001,
                native_timestamp_ms=time,t_ms=1000+i*120,source="IMessageFilter/own-EDIT",area="editor",
                own_handle=1,focus_handle=1,focused=True,key_code=16,modifiers=0))
        check=dict(trial,type="basic_check",ts=4,utc_ms=4000,check_kind="first",check_index=1,state="matched",
                   event_count=2,released=True,text="",selection_start=0,selection_length=0,command_count=0,visible_result="released")
        trace=transcript(dict(kind="key_hold",key="shift",duration_ms=120))
        rows=[trial,*events,check]
        self.assertEqual(self.analyze(rows,trace,"keyboard")["first_matched"],1)
        rows[1]["focused"]=False
        self.assertEqual(self.analyze(rows,trace,"keyboard")["first_matched"],0)
        rows[1]["focused"]=True; rows.pop(2); rows[-1]["event_count"]=1
        self.assertEqual(self.analyze(rows,trace,"keyboard")["first_matched"],0)

    def test_explicit_rejection_is_not_valid_input(self):
        r=evidence(); r=[r[0],r[3],r[4]]
        for row in r: row.update(case_id="pointer-10",trial=10)
        r[0]["spec"]="bounds_reject"; r[1].update(event_count=0,state="needs_tool_evidence")
        t=transcript(dict(kind="move",position=[-1,-1]),dict(error=dict(code="invalid_action"),input_outcome="not_started",cleanup_outcome="not_needed"))
        out=self.analyze(r,t)
        self.assertEqual(out["rejected"],1);self.assertEqual(out["valid_input"],0);self.assertEqual(out["first_matched"],0)

    def test_app_claim_matched_cannot_fake_rejection(self):
        r=evidence(); r=[r[0],r[3],r[4]]
        for row in r: row.update(case_id="pointer-10",trial=10)
        r[0]["spec"]="bounds_reject";r[1]["event_count"]=0
        self.assertEqual(self.analyze(r)["rejected"],0)

    def test_rejection_receiving_event_is_not_confirmed(self):
        r=evidence()
        for row in r: row.update(case_id="pointer-10",trial=10)
        r[0]["spec"]="bounds_reject";r[3]["state"]="needs_tool_evidence"
        t=transcript(dict(kind="move",position=[-1,-1]),dict(error=dict(code="invalid_action"),input_outcome="not_started",cleanup_outcome="not_needed"))
        self.assertEqual(self.analyze(r,t)["rejected"],0)

    def test_wrong_suite_event_not_silently_dropped(self):
        r=evidence();r[1]["suite"]="keyboard"
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_wrong_owned_target(self):
        r=evidence();r[1]["own_handle"]=99
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_wrong_native_button_flags(self):
        r=evidence();r[1]["raw_wparam"]=0
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_wrong_relative_native_time(self):
        for field in ("t_ms","native_timestamp_ms"):
            r=evidence();r[2][field]=90000
            self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_result_request_mismatch(self):
        t=transcript(result=dict(input_outcome="dispatched",cleanup_outcome="not_needed",request_id="other"))
        self.assertEqual(self.analyze(trace=t)["first_matched"],0)

    def test_nonmonotonic_checks_unknown(self):
        r=evidence();r.insert(4,dict(r[3],check_index=2,check_kind="final",ts=3.9,utc_ms=3900))
        self.assertEqual(self.analyze(r)["first_matched"],0)

    def test_serialized_mcp_metadata_envelope(self):
        t=transcript();block=t[1]["message"]["content"][0]
        block["content"]=__import__('json').dumps(dict(content=block["content"],isError=False))
        self.assertEqual(self.analyze(trace=t)["first_matched"],1)

    def test_nested_image_does_not_supply_metadata(self):
        t=transcript();t[1]["message"]["content"][0]["content"]=[dict(type="image",source=dict(input_outcome="dispatched"))]
        self.assertEqual(self.analyze(trace=t)["first_matched"],0)

    def test_scaled_move_requires_actual_metadata(self):
        r=evidence();r.pop(2)
        for row in r:row.update(case_id="pointer-03",trial=3)
        r[0]["spec"]="scaled_move";r[1].update(kind="move",button="none",native_message=0x200,raw_wparam=0)
        r[2].update(event_count=1,state="needs_tool_evidence")
        t=transcript(dict(kind="move",position=[120,100]))
        self.assertEqual(self.analyze(r,t)["first_matched"],0)
        for ident,time,w,h in (("larger",.5,1200,800),("obs",1.3,900,600)):
            t.append(dict(type="assistant",timestamp=time-.1,message=dict(model="glm-5.3-flash",content=[dict(
                type="tool_use",id=ident,name="computer_observe",input=dict(session_id="mcp"))])))
            metadata=dict(session_id="mcp",observation_id=ident,surface_id="surface",geometry_version="g",width_px=w,height_px=h)
            result=dict(type="tool_result",tool_use_id=ident,content=[dict(type="text",text=__import__('json').dumps(metadata))])
            t.append(dict(type="user",timestamp=time,message=dict(content=[result])))
        self.assertEqual(self.analyze(r,t)["first_matched"],1)
        # Merely setting max_width is not evidence: current result must really be smaller.
        payload=t[-1]["message"]["content"][0]["content"][0]
        obj=__import__('json').loads(payload["text"]);obj.update(width_px=1200,height_px=800)
        payload["text"]=__import__('json').dumps(obj)
        self.assertEqual(self.analyze(r,t)["first_matched"],0)

    def test_cli_rejects_invalid_suite_and_unknown_args(self):
        # CC runs CLI validation in subprocess; importing this file never executes the analyzer.
        import subprocess
        import sys
        script=str(Path(__file__).resolve().parents[1]/"scripts/analyze-basic-input-gui.py")
        for args in (("--suite","drag","--oracle","missing","--transcript","missing"),("--bogus",)):
            p=subprocess.run([sys.executable,script,*args],capture_output=True,text=True)
            self.assertEqual(p.returncode,2)


if __name__ == "__main__":
    unittest.main()
