"""Sidecar policy regressions: source only; CC runs these on Windows."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location("native_text",Path(__file__).resolve().parents[1]/"scripts/analyze-windows-native-text.py")
M=importlib.util.module_from_spec(spec);spec.loader.exec_module(M)
RAW="第一行\r\n第二行\r\n第三行"
LF="第一行\n第二行\n第三行"


def record(record_type,t,**kw):
    r=dict(type=record_type,ts=t,run_id="new-run",session_id="fixture",suite="known-input",case_id="known-09",trial=9,nonce="NEW123",platform="windows",
           native_text_policy_version=M.VERSION,native_control=M.CONTROL,native_override_applied=True,actual_normalized=False,
           task_payload=RAW,canonical_expected_text=LF,canonical_expected_utf16_hex=M.hex16(LF),
           native_expected_text=RAW,native_expected_utf16_hex=M.hex16(RAW),expected_text=RAW)
    r.update(kw);return r


def oracle(actual=RAW):
    return [record("session",.1),record("trial",1),record("text_check",4,check_index=1,check_kind="first",actual_text=actual,
            actual_utf16_hex=M.hex16(actual),expected_utf16_hex=M.hex16(RAW),matched=actual==RAW)]


def call(ident,t,name,inp):
    return dict(type="assistant",timestamp=t,message=dict(model="glm-5.3-flash",content=[dict(type="tool_use",id=ident,name=name,input=inp)]))


def result(ident,t,data):
    return dict(type="user",timestamp=t,message=dict(content=[dict(type="tool_result",tool_use_id=ident,content=[dict(type="text",text=json.dumps(data))])]))


def trace(text=RAW):
    return [call("obs",1.1,"computer_observe",dict(session_id="computer")),result("obs",1.2,dict(session_id="computer",observation_id="o1",surface_id="s",geometry_version="g",width_px=1000,height_px=900)),
            call("text",2,"computer_step",dict(session_id="computer",request_id="r1",based_on="o1",action=dict(kind="text_input",text=text))),
            result("text",3,dict(request_id="r1",input_outcome="dispatched",cleanup_outcome="not_needed"))]


class NativeTextAnalyzerTests(unittest.TestCase):
    def analyze(self,o=None,t=None):return M.analyze(oracle() if o is None else o,trace() if t is None else t)
    def test_new_explicit_policy_exact(self):
        r=self.analyze();self.assertEqual(r["first"],"native_exact");self.assertFalse(r["checks"][0]["canonical_match"])
        self.assertTrue(r["checks"][0]["native_match"]);self.assertFalse(r["gui_verified"])
        self.assertEqual(r["checks"][0]["actual_text"],RAW)
    def test_no_old_run_upgrade(self):
        o=oracle();o[-1].update(matched=False,expected_text=LF,expected_utf16_hex=M.hex16(LF))
        for r in o:r.pop("native_text_policy_version")
        r=self.analyze(o);self.assertEqual(r["first"],"unknown");self.assertEqual(r["historical_matched_flags"],[False])
    def test_no_transcript_no_pass(self):self.assertEqual(self.analyze(t=[])["first"],"unknown")
    def test_mixed_and_extra_newlines_rejected(self):
        for actual in [LF,"第一行\r\n第二行\n第三行","第一行\n第二行\r\n第三行","第一行\r第二行\r第三行",
                       "第一行\r\r\n第二行\r\n第三行","第一行\r\n\n第二行\r\n第三行",RAW+"\r",RAW+"\n",RAW+"\r\n"]:
            with self.subTest(actual=repr(actual)):
                r=self.analyze(oracle(actual));self.assertEqual(r["first"],"mismatch")
                self.assertEqual(r["checks"][0]["actual_text"],actual)
    def test_raw_tool_payload_not_normalized(self):self.assertEqual(self.analyze(t=trace(LF))["first"],"mismatch")
    def test_wrong_policy_unknown(self):
        o=oracle();o[-1]["native_text_policy_version"]="future";self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_missing_canonical_metadata_unknown(self):
        o=oracle();o[1].pop("canonical_expected_text");self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_wrong_native_expected_unknown(self):
        o=oracle();o[-1]["native_expected_text"]=LF;self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_claiming_actual_normalized_rejected(self):
        o=oracle();o[-1]["actual_normalized"]=True;self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_wrong_actual_hex_unknown(self):
        o=oracle();o[-1]["actual_utf16_hex"]=M.hex16(LF);self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_wrong_platform_unknown(self):
        o=oracle();o[1]["platform"]="macos";self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_wrong_nonce_unknown(self):
        o=oracle();o[-1]["nonce"]="OTHER";self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_wrong_session_unknown(self):
        o=oracle();o[-1]["session_id"]="OTHER";self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_wrong_run_unknown(self):
        o=oracle();o[-1]["run_id"]="OLD";self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_missing_time_unknown(self):
        o=oracle();o[-1].pop("ts");self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_duplicate_first_unknown(self):
        o=oracle();o.append(copy.deepcopy(o[-1]));self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_duplicate_trial_unknown(self):
        o=oracle();o.insert(2,copy.deepcopy(o[1]));self.assertEqual(self.analyze(o)["first"],"unknown")
    def test_ambiguous_tool_inputs_unknown(self):
        t=trace();t.extend([call("text2",2.2,"computer_step",dict(session_id="computer",request_id="r2",based_on="o1",action=dict(kind="text_input",text=RAW))),result("text2",3.2,dict(request_id="r2",input_outcome="dispatched",cleanup_outcome="released"))])
        self.assertEqual(self.analyze(t=t)["first"],"unknown")
    def test_missing_result_unknown(self):self.assertEqual(self.analyze(t=trace()[:-1])["first"],"unknown")
    def test_wrong_model_unknown(self):
        t=trace();t[2]["message"]["model"]="other";self.assertEqual(self.analyze(t=t)["first"],"unknown")
    def test_wrong_request_id_unknown(self):
        t=trace();t[-1]=result("text",3,dict(request_id="other",input_outcome="dispatched",cleanup_outcome="released"))
        self.assertEqual(self.analyze(t=t)["first"],"unknown")
    def test_stale_or_missing_based_on_unknown(self):
        t=trace();t[2]["message"]["content"][0]["input"]["based_on"]="missing"
        self.assertEqual(self.analyze(t=t)["first"],"unknown")
    def test_cleanup_failure_unknown(self):
        t=trace();t[-1]=result("text",3,dict(request_id="r1",input_outcome="dispatched",cleanup_outcome="failed"))
        self.assertEqual(self.analyze(t=t)["first"],"unknown")
    def test_first_mismatch_not_replaced_by_final(self):
        o=oracle(LF);o.append(record("text_check",8,check_index=2,check_kind="final",actual_text=RAW,actual_utf16_hex=M.hex16(RAW),expected_utf16_hex=M.hex16(RAW),matched=True))
        t=trace();more=trace()
        for r in more:
            r["timestamp"]+=4
            b=r["message"]["content"][0]
            if b["type"]=="tool_use":
                b["id"]+="2"
                if b["name"]=="computer_step":b["input"].update(request_id="r2",based_on="o2")
            else:
                b["tool_use_id"]+="2"
                data=json.loads(b["content"][0]["text"])
                if "observation_id" in data:data["observation_id"]="o2"
                if "request_id" in data:data["request_id"]="r2"
                b["content"][0]["text"]=json.dumps(data)
        r=self.analyze(o,t+more);self.assertEqual(r["first"],"mismatch");self.assertEqual(r["final"],"native_exact");self.assertEqual(r["retry_checks"],1)
    def test_no_global_newline_equivalence(self):
        o=oracle();o[1]["task_payload"]="a\r\nb\nc";self.assertEqual(self.analyze(o)["first"],"unknown")


if __name__=="__main__":unittest.main()
