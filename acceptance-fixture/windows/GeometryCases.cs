// Independent Windows geometry manifest; never part of canonical legacy63.
using System;
using System.Text;
namespace AcceptanceFixture {
    sealed class GeometryCase {
        public readonly string Id,Spec,Mode,Instruction;
        public GeometryCase(int n,string spec,string mode,string instruction) {Id="geometry-"+n.ToString("00");Spec=spec;Mode=mode;Instruction=instruction;}
    }
    static class GeometryCatalog {
        public static readonly GeometryCase[] Cases={
            new GeometryCase(1,"native_size","gui","Open with max_width=4096 max_height=4096. Observe, click TARGET once, then Check. Native size requires actual scale 1; larger capture => needs_capability."),
            new GeometryCase(2,"bound_1920","gui","Open with max_width=1920 max_height=1920. Observe, click TARGET once, then Check. Bound is not an exact output size."),
            new GeometryCase(3,"bound_1440","gui","Open with max_width=1440 max_height=1440. Observe, click TARGET once, then Check. Derive position from this image."),
            new GeometryCase(4,"aspect_mapping","gui","Open with max_width=1000 max_height=700. Observe, click TARGET once, then Check. Real aspect and per-region mapping are supervisor checks."),
            new GeometryCase(5,"high_dpi","manual_environment","Requires real HWND DPI >96. Otherwise STOP and Close: needs_environment. Observe, click TARGET once, then Check."),
            new GeometryCase(6,"negative_origin","pure_mapping","NOT GUI. Pure negative-origin mapping only; no input required. This does not prove production mapper or multiple physical displays."),
            new GeometryCase(7,"resolution_stale","manual_environment","Manual supervisor resolution change only. Observe before change. Wait for supervisor. Submit ONE target click with OLD based_on; it must be rejected without input. Close the faulted session; Open a new session and Observe after change, then Close. Never retry input."),
            new GeometryCase(8,"dpi_stale","manual_environment","Manual supervisor DPI change only. Observe before change. Wait for supervisor. Submit ONE target click with OLD based_on; it must be rejected without input. Close the faulted session; Open a new session and Observe after change, then Close. Never retry input."),
            new GeometryCase(9,"target_removal","gui","Observe; click REMOVE TARGET once. Observe disappearance. STOP: no more input, including Check/Next. Close. Removal is semantic, NOT necessarily a topology change."),
            new GeometryCase(10,"png_dimensions","gui","Observe, click TARGET once, then Check. Supervisor must validate real returned PNG bytes, CRC/decompression and dimensions; metadata alone is insufficient.")
        };
        public static string ManifestJson() {
            var b=new StringBuilder("{\"schema\":\"windows-geometry-v1\",\"platform\":\"windows\",\"suite\":\"geometry\",\"total\":10,\"gui_verified\":false,\"ten_valid_trials_per_action_gate\":false,\"cases\":[");
            for(int i=0;i<Cases.Length;i++) {var c=Cases[i];if(i>0)b.Append(',');
                b.Append("{\"id\":").Append(GestureExporter.JsonString(c.Id)).Append(",\"spec\":").Append(GestureExporter.JsonString(c.Spec))
                 .Append(",\"mode\":").Append(GestureExporter.JsonString(c.Mode)).Append(",\"instruction\":").Append(GestureExporter.JsonString(c.Instruction))
                 .Append(",\"status\":").Append(GestureExporter.JsonString(c.Mode=="pure_mapping"?"not_gui":c.Mode=="manual_environment"?"needs_environment":"pending_CC_GUI"))
                 .Append(",\"trials_per_run\":1}");
            }
            return b.Append("]}\n").ToString();
        }
    }
    static class GeometryMath {
        // Fixture-side pure check, NOT a production mapper certification.
        public static int? TryMapAxis(int p,int image,int origin,int size) {
            if(image<=0||size<=0||p<0||p>=image)return null;
            long offset=Math.Min(size-1,(long)Math.Floor((double)p*size/image));
            long value=(long)origin+offset;return value<int.MinValue||value>int.MaxValue?(int?)null:(int)value;
        }
        public static int MapAxis(int p,int image,int origin,int size) {var n=TryMapAxis(p,image,origin,size);if(!n.HasValue)throw new ArgumentException("invalid mapping");return n.Value;}
    }
    static class GeometryJudge {
        public static bool ClickPair(int down,int up,long d,long u,int dx,int dy,int ux,int uy) {
            return down==0x201&&up==0x202&&d>0&&u>d&&Math.Abs((long)dx-ux)<=3&&Math.Abs((long)dy-uy)<=3;
        }
        public static string Local(GeometryCase c,bool hit,int dpi,bool dpiReliable,bool environmentChanged) {
            if(c.Mode=="pure_mapping")return "not_gui";
            if(c.Spec=="high_dpi"&&!dpiReliable)return "needs_capability";
            if(c.Spec=="high_dpi"&&dpi<=96)return "needs_environment";
            if((c.Spec=="resolution_stale"||c.Spec=="dpi_stale")&&!environmentChanged)return "needs_environment";
            if(c.Spec=="target_removal"||c.Mode=="manual_environment"&&environmentChanged)return "pending_tool_evidence";
            return hit?"pending_tool_evidence":"unknown";
        }
    }
}
