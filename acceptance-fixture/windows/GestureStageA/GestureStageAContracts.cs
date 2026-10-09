// Regression calls the real GestureJudge with frozen raw CC evidence, not a fake judge.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Web.Script.Serialization;
namespace AcceptanceFixture {
    static class GestureStageAContracts {
        internal const string OracleSha256="2fe4a01115cce6cdddda1aae133111b598516638638f123682f95835c5061add";
        private sealed class Sample {
            internal GestureCase Case;
            internal List<GestureEvent> Events;
            internal string Selection;
            internal double Interval, SlopX, SlopY;
        }
        private static string S(Dictionary<string,object> row, string key) { return Convert.ToString(row[key],CultureInfo.InvariantCulture); }
        private static double D(Dictionary<string,object> row, string key) { return Convert.ToDouble(row[key],CultureInfo.InvariantCulture); }
        private static int I(Dictionary<string,object> row, string key) { return Convert.ToInt32(row[key],CultureInfo.InvariantCulture); }
        private static bool B(Dictionary<string,object> row, string key) { return Convert.ToBoolean(row[key],CultureInfo.InvariantCulture); }
        private static Sample Read(List<Dictionary<string,object>> rows, string id) {
            var trial=rows.Single(r=>S(r,"type")=="trial" && r.ContainsKey("case_id") && S(r,"case_id")==id);
            var check=rows.Single(r=>S(r,"type")=="gesture_check" && S(r,"case_id")==id && I(r,"check_index")==1);
            var inputs=rows.Where(r=>S(r,"type")=="input_event" && S(r,"case_id")==id).ToArray();
            if (S(trial,"platform")!="windows" || S(check,"check_kind")!="first" || !B(check,"released"))
                throw new InvalidDataException("unexpected frozen trial/check identity "+id);
            for (int n=0;n<inputs.Length;n++) {
                if (I(inputs[n],"event_index")!=n+1 || S(inputs[n],"nonce")!=S(trial,"nonce") ||
                    S(inputs[n],"run_id")!=S(trial,"run_id") || S(inputs[n],"session_id")!=S(trial,"session_id"))
                    throw new InvalidDataException("event identity/order changed "+id);
            }
            return new Sample {
                Case=GestureCatalog.MulticlickCases.Single(c=>c.Id==id), Selection=S(check,"selection"),
                Interval=D(trial,"native_click_interval_ms"),SlopX=D(trial,"native_click_slop_x"),SlopY=D(trial,"native_click_slop_y"),
                Events=inputs.Select(r=>new GestureEvent {
                    Kind=S(r,"kind"),Button=S(r,"button"),Area=S(r,"area"),Source=S(r,"source"),
                    NativeCount=I(r,"native_count"),NativeMessage=I(r,"native_message"),NativeTimeMs=I(r,"native_timestamp_ms"),
                    MouseEventClicks=I(r,"mouse_event_clicks"),DoubleClickMsg=B(r,"double_click_msg"),Held=B(r,"held"),
                    X=D(r,"x"),Y=D(r,"y"),Dx=D(r,"raw_dx"),Dy=D(r,"raw_dy"),TMs=D(r,"t_ms"),
                    WParam=Convert.ToInt64(r["raw_wparam"]),LParam=Convert.ToInt64(r["raw_lparam"])
                }).ToList()
            };
        }
        private static void Expect(Sample s, string name, string selection, bool matched, IList<GestureEvent> events) {
            var obs=new GestureObserved {Selection=selection};
            var result=GestureJudge.Multiclick(s.Case,events,obs,s.Interval,s.SlopX,s.SlopY);
            GestureStageARunner.Check(name,result.Matched==matched,"expected matched="+matched+"; "+result.Observed+"; "+result.Reason);
            GestureStageARunner.Check(name+"-raw-preserved",string.Equals(obs.Selection,selection,StringComparison.Ordinal),
                "UTF16="+string.Join(" ",obs.Selection.Select(c=>((int)c).ToString("X4"))));
        }
        private static List<GestureEvent> Clone(Sample s) {
            return s.Events.Select(e=>new GestureEvent {Kind=e.Kind,Button=e.Button,Area=e.Area,Source=e.Source,
                NativeCount=e.NativeCount,NativeMessage=e.NativeMessage,NativeTimeMs=e.NativeTimeMs,MouseEventClicks=e.MouseEventClicks,
                DoubleClickMsg=e.DoubleClickMsg,Held=e.Held,X=e.X,Y=e.Y,Dx=e.Dx,Dy=e.Dy,TMs=e.TMs,WParam=e.WParam,LParam=e.LParam}).ToList();
        }
        internal static void Run(string path) {
            string hash;
            using(var sha=SHA256.Create()) hash=BitConverter.ToString(sha.ComputeHash(File.ReadAllBytes(path))).Replace("-","").ToLowerInvariant();
            if(hash!=OracleSha256) throw new InvalidDataException("frozen oracle SHA mismatch: "+hash);
            var serializer=new JavaScriptSerializer();
            var rows=File.ReadAllLines(path).Where(l=>!string.IsNullOrWhiteSpace(l)).Select(l=>serializer.Deserialize<Dictionary<string,object>>(l)).ToList();
            var word=Read(rows,"multiclick-04"); var line=Read(rows,"multiclick-05"); var omitted=Read(rows,"multiclick-08");
            // Integrity guards throw (exit 2), not test assertions that could be misreported as RED.
            if(word.Selection!="alpha " || word.Events.Count!=4 || line.Selection!="" || line.Events.Count!=6 || omitted.Events.Count!=4)
                throw new InvalidDataException("unexpected frozen selection/count");
            GestureStageARunner.Emit(new {type="replay",oracle_sha256=hash,origin="CC 2026-10-08 first attempt",gui_proof=false});
            Expect(word,"04-recorded-native-alpha-U0020",word.Selection,true,word.Events); // Expected current RED.
            Expect(word,"04-exact-alpha", "alpha",true,word.Events);
            foreach(string wrong in new[]{"","beta"," alpha","alpha  ","alpha\t","alpha\u00a0","alpha beta","alph"})
                Expect(word,"04-reject-"+GestureExporter.JsonString(wrong),wrong,false,word.Events);
            Expect(word,"04-missing-release","alpha",false,word.Events.Take(3).ToList());
            var noDbl=Clone(word); noDbl[2].DoubleClickMsg=false;noDbl[2].NativeMessage=GestureMessages.LDown;noDbl[2].NativeCount=1;
            Expect(word,"04-no-native-dblclk","alpha",false,noDbl);
            var late=Clone(word);late[2].TMs=late[0].TMs+word.Interval+1;late[3].TMs=late[2].TMs+3;
            Expect(word,"04-outside-doubleclick-time","alpha",false,late);
            var far=Clone(word);far[2].X=far[3].X=far[0].X+word.SlopX+1;
            Expect(word,"04-outside-doubleclick-slop","alpha",false,far);
            var outside=Clone(word);outside[3].Area="outside";
            Expect(word,"04-release-outside","alpha",false,outside);
            // Preserve ORIGINAL whole-line user goal. This is not a native triple-click guarantee.
            Expect(line,"05-recorded-empty-not-success",line.Selection,false,line.Events);
            Expect(line,"05-whole-sentence-positive",GestureCatalog.SelectWordSentence,true,line.Events);
            foreach(string partial in new[]{"alpha","alpha ","beta"," "})
                Expect(line,"05-partial-is-not-whole-line-"+GestureExporter.JsonString(partial),partial,false,line.Events); // Expected current RED.
            Expect(line,"05-whole-line-with-missing-release",GestureCatalog.SelectWordSentence,false,line.Events.Take(5).ToList());
            var delivery=GestureJudge.Multiclick(GestureCatalog.MulticlickCases.Single(c=>c.Id=="multiclick-03"),
                line.Events.Select(e=>new GestureEvent {Kind=e.Kind,Button=e.Button,Area="target",NativeCount=e.NativeCount,
                    DoubleClickMsg=e.DoubleClickMsg,TMs=e.TMs,X=e.X,Y=e.Y,Held=e.Held}).ToList(),new GestureObserved(),line.Interval,line.SlopX,line.SlopY);
            GestureStageARunner.Check("05-delivery-only-three-presses",delivery.Matched,
                "area remapped ONLY to isolate existing generic triple delivery judge; does not fulfill line goal: "+delivery.Observed);
            GestureStageARunner.Emit(new {type="capability_diagnostic",case_id=line.Case.Id,selection=line.Selection,
                delivery_satisfied=delivery.Matched,original_goal_satisfied=false,classification="pending_native_observation_and_coordinator_approval"});
            Expect(omitted,"08-recorded-final-left-omission-rejected",omitted.Selection,false,omitted.Events);
            GestureStageARunner.Check("08-native-right-present",omitted.Events.Any(e=>e.Kind=="down" && e.Button=="right" && e.NativeMessage==GestureMessages.RDown),
                "No fixture/kernel patch authorized for model omission");
        }
    }
}
