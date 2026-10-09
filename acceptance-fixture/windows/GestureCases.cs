// Pure C#5 gesture catalog, raw event model and fixed logical 860x300 layout.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
namespace AcceptanceFixture {
    sealed class GestureEvent {
        public string Kind = "", Button = "left", Area = "", Source = "WndProc/own-control";
        public int NativeCount, NativeMessage, NativeTimeMs, MouseEventClicks = -1;
        public bool DoubleClickMsg, Held;
        public double X, Y, Dx, Dy, TMs;
        public long WParam, LParam;
    }
    sealed class GestureObserved {
        public string Selection = "";
        public int AVStart, AVEnd, AHStart, AHEnd, BVStart, BVEnd, BHStart, BHEnd;
        public int AVMax, AHMax, BVMax, BHMax;
    }
    sealed class GestureCase {
        public readonly string Id, Spec, Instruction;
        public GestureCase(string id, string spec, string instruction) { Id=id; Spec=spec; Instruction=instruction; }
        public string Field(string key) {
            foreach (string pair in Spec.Split(';')) {
                string[] kv = pair.Split(new char[] {'='}, 2);
                if (kv.Length == 2 && kv[0] == key) return kv[1];
            }
            return "";
        }
        public int IntField(string key) { int n; return int.TryParse(Field(key), out n) ? n : 0; }
    }
    static class GestureCatalog {
        public const string SelectWordSentence = "alpha beta gamma delta epsilon";
        public const string DragSelectSentence = "the quick brown fox jumps over the lazy dog";
        public static readonly string[] Suites = {"multiclick", "drag", "scroll"};
        public static readonly GestureCase[] MulticlickCases = {
            new GestureCase("multiclick-01", "flow=single;expect_count=1", "Click the big TARGET rectangle exactly once (a single left click)."),
            new GestureCase("multiclick-02", "flow=multi;expect_count=2", "Double-click the big TARGET rectangle (two rapid left clicks)."),
            new GestureCase("multiclick-03", "flow=multi;expect_count=3", "Triple-click the big TARGET rectangle (three rapid left clicks)."),
            new GestureCase("multiclick-04", "flow=select_word", "Double-click the word alpha in the sentence below to select it."),
            new GestureCase("multiclick-05", "flow=select_line", "Triple-click the sentence below to select the whole line (per this platform's convention)."),
            new GestureCase("multiclick-06", "flow=two_targets", "Click zone A once, then click zone B once (two separate single clicks)."),
            new GestureCase("multiclick-07", "flow=two_positions", "Click once near the LEFT edge of the TARGET, then once near the RIGHT edge (far apart)."),
            new GestureCase("multiclick-08", "flow=right_between", "Left-click the TARGET once, then RIGHT-click it once, then left-click it once."),
            new GestureCase("multiclick-09", "flow=slow_two", "Click the TARGET once, wait at least 2 seconds, then click it once again."),
            new GestureCase("multiclick-10", "flow=invalid_count", "Attempt a click with an INVALID count of 0 using your click tool. The app must receive no clicks at all."),
        };
        public static readonly GestureCase[] DragCases = {
            new GestureCase("drag-01", "axis=h;dir=lr", "Press and hold the left button inside zone START, drag horizontally to zone END, release inside END."),
            new GestureCase("drag-02", "axis=h;dir=rl", "Press and hold inside zone START (right side), drag horizontally left to zone END, release inside END."),
            new GestureCase("drag-03", "axis=v;dir=tb", "Press and hold inside zone START (top), drag vertically down to zone END, release inside END."),
            new GestureCase("drag-04", "axis=v;dir=bt", "Press and hold inside zone START (bottom), drag vertically up to zone END, release inside END."),
            new GestureCase("drag-05", "path=polyline", "Press inside START, drag through WAYPOINT, continue to zone END, release inside END."),
            new GestureCase("drag-06", "span=short", "Drag from START to END (short distance) with the button held the whole time."),
            new GestureCase("drag-07", "span=long;min_ms=750", "Drag from START to END across the canvas, keeping the button held for at least 0.75 seconds."),
            new GestureCase("drag-08", "release=inner_edge", "Press inside START, drag to the END zone, and release INSIDE it but near its edge (not at the center)."),
            new GestureCase("drag-09", "flow=text_select", "Drag-select the words quick brown in the sentence below (press before quick, release after brown)."),
            new GestureCase("drag-10", "path=curve", "Press inside START, draw a smooth curved path (direction changes at least twice) and release inside END."),
        };
        public static readonly GestureCase[] ScrollCases = {
            new GestureCase("scroll-01", "panel=A;axis=v;dir=down;ticks=3", "Scroll Panel A DOWN by about 3 wheel ticks."),
            new GestureCase("scroll-02", "panel=A;axis=v;dir=up;ticks=3", "Scroll Panel A UP by about 3 wheel ticks."),
            new GestureCase("scroll-03", "panel=A;axis=h;dir=right;ticks=3", "Scroll Panel A to the RIGHT by about 3 wheel ticks."),
            new GestureCase("scroll-04", "panel=A;axis=h;dir=left;ticks=3", "Scroll Panel A to the LEFT by about 3 wheel ticks."),
            new GestureCase("scroll-05", "panel=A;axis=both;dir=down;dir2=right;ticks=3", "Scroll Panel A DOWN and to the RIGHT at the same time (about 3 ticks each)."),
            new GestureCase("scroll-06", "panel=B;axis=v;dir=down;ticks=3", "Scroll Panel B DOWN by about 3 wheel ticks. Panel A must not move."),
            new GestureCase("scroll-07", "panel=A;axis=v;dir=down;ticks=1", "Scroll Panel A DOWN by a small amount (about 1 wheel tick)."),
            new GestureCase("scroll-08", "panel=A;axis=v;dir=down;ticks=20;flow=saturate", "Scroll Panel A DOWN by many ticks (about 20) until it saturates at the bottom."),
            new GestureCase("scroll-09", "flow=zero_delta", "Attempt a scroll with a ZERO delta (0, 0). The page must not scroll."),
            new GestureCase("scroll-10", "flow=invalid_args", "Attempt a scroll with INVALID arguments (e.g. an unknown direction). No wheel event may arrive."),
        };
        public static GestureCase[] CasesFor(string suite) {
            switch(suite) { case "multiclick": return MulticlickCases; case "drag": return DragCases; case "scroll": return ScrollCases; default: return new GestureCase[0]; }
        }
        public static bool IsGesture(string suite) { return Suites.Contains(suite); }
    }
    sealed class GestureZone {
        public string Name; public double X,Y,W,H;
        public GestureZone(string name, double x, double y, double w, double h) {Name=name;X=x;Y=y;W=w;H=h;}
        public bool Contains(GestureEvent e) { return e.X >= X && e.X <= X+W && e.Y >= Y && e.Y <= Y+H; }
    }
    static class GestureLayout {
        public static GestureZone[] Zones(GestureCase c) {
            if(c.Field("flow")=="two_targets") return new[] {new GestureZone("zoneA",100,90,220,120),new GestureZone("zoneB",540,90,220,120)};
            if(c.Field("axis")=="h") return c.Field("dir")=="rl"
                ? new[] {new GestureZone("START",640,100,160,100),new GestureZone("END",60,100,160,100)}
                : new[] {new GestureZone("START",60,100,160,100),new GestureZone("END",640,100,160,100)};
            if(c.Field("axis")=="v") return c.Field("dir")=="bt"
                ? new[] {new GestureZone("START",350,180,160,100),new GestureZone("END",350,20,160,100)}
                : new[] {new GestureZone("START",350,20,160,100),new GestureZone("END",350,180,160,100)};
            if(c.Field("path")=="polyline") return new[] {new GestureZone("START",40,110,140,90),new GestureZone("WAYPOINT",360,20,140,80),new GestureZone("END",680,190,140,90)};
            if(c.Field("path")=="curve") return new[] {new GestureZone("START",40,200,130,80),new GestureZone("END",690,20,130,80)};
            if(c.Field("span")=="short") return new[] {new GestureZone("START",300,110,120,80),new GestureZone("END",440,110,120,80)};
            if(c.Field("span")=="long") return new[] {new GestureZone("START",30,110,120,80),new GestureZone("END",710,110,120,80)};
            if(c.Field("release")=="inner_edge") return new[] {new GestureZone("START",80,110,140,90),new GestureZone("END",600,110,180,90)};
            if(c.Field("flow")=="text_select") return new GestureZone[0];
            return new[] {new GestureZone("TARGET",280,90,300,120)};
        }
    }
    static class GesturePlatformNotes {
        public static string Note(string suite, string id, string platform) {
            if(suite=="multiclick" && id=="multiclick-03") return platform=="macos"
                ? "macos: NSEvent.clickCount must reach 3"
                : "windows: three real down/up pairs and at least one WM_LBUTTONDBLCLK; third native count may be 1 or 2, never require 3";
            if(suite=="multiclick" && id=="multiclick-05") return platform=="macos"
                ? "macos: triple click selects the whole line/sentence"
                : "windows: standard EDIT has no triple-click line selection; pass = 3 downs + nonempty visible selection";
            if(suite=="scroll") return platform=="macos"
                ? "macos: scrollWheel raw deltas logged; contract positive-down/right = -raw"
                : "windows: WM_MOUSEWHEEL/WM_MOUSEHWHEEL raw deltas logged; contract dy=-raw, dx=+raw";
            return "";
        }
    }
    static class GestureExporter {
        public static string JsonString(string value) {
            var b = new StringBuilder("\"");
            foreach(char c in value) {
                if(c=='"') b.Append("\\\""); else if(c=='\\') b.Append("\\\\");
                else if(c<32) b.Append("\\u"+((int)c).ToString("x4")); else b.Append(c);
            }
            return b.Append('"').ToString();
        }
        public static string ManifestJson() {
            string old = CaseExporter.ManifestJson();
            int close = old.LastIndexOf("  }", StringComparison.Ordinal);
            var b = new StringBuilder(old.Substring(0, close).TrimEnd());
            foreach(string suite in GestureCatalog.Suites) {
                b.Append(",\n    ").Append(JsonString(suite)).Append(": {\"total\":10,\"cases\":[");
                GestureCase[] cases = GestureCatalog.CasesFor(suite);
                for(int i=0;i<cases.Length;i++) {
                    if(i>0) b.Append(','); var c=cases[i];
                    b.Append("{\"id\":").Append(JsonString(c.Id)).Append(",\"spec\":").Append(JsonString(c.Spec))
                     .Append(",\"instruction\":").Append(JsonString(c.Instruction)).Append(",\"platform_expected\":{\"macos\":")
                     .Append(JsonString(GesturePlatformNotes.Note(suite,c.Id,"macos"))).Append(",\"windows\":")
                     .Append(JsonString(GesturePlatformNotes.Note(suite,c.Id,"windows"))).Append("}}");
                }
                b.Append("]}");
            }
            return b.Append("\n  }\n}\n").ToString();
        }
    }
}
