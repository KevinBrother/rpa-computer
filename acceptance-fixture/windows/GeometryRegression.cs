// Geometry-only pure requirements. No HWND, IO, input or system configuration.
using System;
using System.Collections.Generic;
using System.Web.Script.Serialization;
namespace AcceptanceFixture {
    static class GeometryRegression {
        static void Check(List<SelfTestResult> r,string name,bool ok) {r.Add(new SelfTestResult("geometry-"+name,ok,"pure source contract; NOT GUI evidence"));}
        static bool Reject(string[] args) {try{BasicArguments.Parse(args);return false;}catch(ArgumentException){return true;}}
        public static List<SelfTestResult> Run() {
            var r=new List<SelfTestResult>();
            foreach(var a in new[]{new[]{"--export-geometry-cases","C:\\space dir\\geometry.json"},new[]{"--export-geometry-cases=C:\\space dir\\geometry.json"}})
                Check(r,"export-real-path-"+a.Length,BasicArguments.Parse(a).GeometryExportPath=="C:\\space dir\\geometry.json");
            Check(r,"suite-equals",BasicArguments.Parse(new[]{"--suite=geometry"}).Suite=="geometry");
            Check(r,"export-missing",Reject(new[]{"--export-geometry-cases"}));
            Check(r,"export-empty",Reject(new[]{"--export-geometry-cases="}));
            Check(r,"export-whitespace",Reject(new[]{"--export-geometry-cases"," "}));
            Check(r,"export-duplicate",Reject(new[]{"--export-geometry-cases=a","--export-geometry-cases","b"}));
            foreach(string mode in new[]{"--self-test","--export-cases=x","--export-platform-cases=x","--export-focus-cases=x"})
                Check(r,"export-conflict-"+mode,Reject(new[]{"--export-geometry-cases=x",mode}));
            var ids=new HashSet<string>();foreach(var c in GeometryCatalog.Cases)ids.Add(c.Id);
            Check(r,"catalog-ten-unique",GeometryCatalog.Cases.Length==10&&ids.Count==10&&ids.Contains("geometry-01")&&ids.Contains("geometry-10"));
            var j=new JavaScriptSerializer().DeserializeObject(GeometryCatalog.ManifestJson()) as Dictionary<string,object>;
            var cases=(object[])j["cases"];
            Check(r,"manifest-independent",(string)j["suite"]=="geometry"&&(int)j["total"]==10&&cases.Length==10&&!(bool)j["gui_verified"]);
            Check(r,"manifest-no-ten-trial-claim",!(bool)j["ten_valid_trials_per_action_gate"]);
            var expectedSpecs=new[]{"native_size","bound_1920","bound_1440","aspect_mapping","high_dpi","negative_origin","resolution_stale","dpi_stale","target_removal","png_dimensions"};
            var expectedModes=new[]{"gui","gui","gui","gui","manual_environment","pure_mapping","manual_environment","manual_environment","gui","gui"};
            for(int i=0;i<10;i++) {
                var entry=(Dictionary<string,object>)cases[i];
                Check(r,"manifest-case-"+(i+1),(string)entry["id"]=="geometry-"+(i+1).ToString("00")&&(string)entry["spec"]==expectedSpecs[i]&&(string)entry["mode"]==expectedModes[i]);
            }
            Check(r,"negative-origin-not-gui",GeometryCatalog.Cases[5].Mode=="pure_mapping");
            Check(r,"resolution-environment",GeometryCatalog.Cases[6].Mode=="manual_environment");
            Check(r,"dpi-environment",GeometryCatalog.Cases[7].Mode=="manual_environment");
            Check(r,"target-not-topology",GeometryCatalog.Cases[8].Spec=="target_removal");
            Check(r,"png-byte-proof",GeometryCatalog.Cases[9].Spec=="png_dimensions");
            Check(r,"map-negative-origin",GeometryMath.MapAxis(50,100,-1920,1920)==-960);
            Check(r,"map-origin",GeometryMath.MapAxis(0,100,-1920,1920)==-1920);
            Check(r,"map-end-clamped",GeometryMath.MapAxis(99,100,-50,20)==-31);
            Check(r,"map-floors-region-axis",GeometryMath.MapAxis(1,2,0,3)==1);
            Check(r,"map-invalid",GeometryMath.TryMapAxis(-1,100,0,100)==null&&GeometryMath.TryMapAxis(100,100,0,100)==null&&GeometryMath.TryMapAxis(0,0,0,100)==null);
            Check(r,"map-overflow",GeometryMath.TryMapAxis(9,10,int.MaxValue,100)==null);
            Check(r,"highdpi-missing-capability",GeometryJudge.Local(GeometryCatalog.Cases[4],false,0,false,false)=="needs_capability");
            Check(r,"highdpi-96-needs-environment",GeometryJudge.Local(GeometryCatalog.Cases[4],false,96,true,false)=="needs_environment");
            Check(r,"highdpi-hit-still-pending",GeometryJudge.Local(GeometryCatalog.Cases[4],true,144,true,false)=="pending_tool_evidence");
            Check(r,"pure-never-gui",GeometryJudge.Local(GeometryCatalog.Cases[5],false,96,false,false)=="not_gui");
            Check(r,"change-needs-real-environment",GeometryJudge.Local(GeometryCatalog.Cases[6],true,96,true,false)=="needs_environment");
            Check(r,"change-needs-tool-rejection",GeometryJudge.Local(GeometryCatalog.Cases[7],false,144,true,true)=="pending_tool_evidence");
            Check(r,"target-removal-not-pass",GeometryJudge.Local(GeometryCatalog.Cases[8],true,96,true,false)=="pending_tool_evidence");
            Check(r,"hit-not-proof",GeometryJudge.Local(GeometryCatalog.Cases[0],true,96,true,false)=="pending_tool_evidence");
            Check(r,"no-hit-unknown",GeometryJudge.Local(GeometryCatalog.Cases[0],false,96,true,false)=="unknown");
            Check(r,"down-alone-rejected",!GeometryJudge.ClickPair(0x201,0,1,2,10,10,10,10));
            Check(r,"up-before-down-rejected",!GeometryJudge.ClickPair(0x201,0x202,2,1,10,10,10,10));
            Check(r,"raw-pair-accepted",GeometryJudge.ClickPair(0x201,0x202,1,2,10,10,10,10));
            Check(r,"different-endpoint-rejected",!GeometryJudge.ClickPair(0x201,0x202,1,2,10,10,500,500));
            return r;
        }
    }
}
