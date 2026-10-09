// Pure judges, C#5/.NET Framework. No GUI or injected inputs here.
using System;
using System.Collections.Generic;
using System.Linq;
namespace AcceptanceFixture {
    sealed class GestureVerdict {
        public bool Matched; public string Reason, Observed;
        public GestureVerdict(bool ok, string reason, string observed) {Matched=ok;Reason=reason;Observed=observed;}
    }
    static class GestureJudge {
        public const string WholeLineRequirement="three released presses with native DBLCLK and selection of the entire sentence";
        public static bool Finite(double v) { return !double.IsNaN(v) && !double.IsInfinity(v); }
        public static bool ValidStream(IList<GestureEvent> events, bool drag) {
            string held=null; double last=-1;
            foreach(var e in events) {
                if(!Finite(e.TMs)||e.TMs<0||e.TMs<last||!Finite(e.X)||!Finite(e.Y)||!Finite(e.Dx)||!Finite(e.Dy)) return false;
                last=e.TMs;
                if(e.Kind=="down") {if(held!=null) return false;held=e.Button;}
                else if(e.Kind=="up") {if(held!=e.Button||e.Held) return false;held=null;}
                else if(e.Kind=="move") {
                    if(e.Held && (held!=e.Button || !drag)) return false;
                    if(drag && held!=null && !e.Held) return false;
                } else return false;
            }
            return held==null;
        }
        public static GestureVerdict Multiclick(GestureCase c, IList<GestureEvent> events, GestureObserved obs,
                                               double intervalMs, double slopX, double slopY) {
            var d=events.Where(e=>e.Kind=="down").ToArray(); var target=d.Where(e=>e.Area=="target").ToArray();
            string summary="downs="+d.Length+" ups="+events.Count(e=>e.Kind=="up")+" native="+string.Join(",",d.Select(e=>e.NativeCount))
                +" dblclk="+events.Count(e=>e.DoubleClickMsg)+" selection='"+obs.Selection+"'";
            Func<bool,string,GestureVerdict> v=(ok,why)=>new GestureVerdict(ok,why,summary);
            if(!ValidStream(events,false)) return v(false,"invalid order/timing or missing release");
            string flow=c.Field("flow");
            if(flow=="invalid_count") return v(events.Count==0,"no native input permitted; tool rejection needs separate evidence");
            if(d.Length==0||events.Any(e=>e.Area=="outside")) return v(false,"missing target hit or outside input");
            if(!events.Where(e=>e.Kind=="up").Select(e=>e.Area).SequenceEqual(d.Select(e=>e.Area))) return v(false,"release outside target");
            Func<GestureEvent[],int,bool> multi=(ds,n)=> {
                if(n<2||ds.Length!=n||!ds.All(e=>e.Button=="left")) return false;
                for(int i=1;i<ds.Length;i++) if(ds[i].TMs-ds[i-1].TMs>intervalMs||Math.Abs(ds[i].X-ds[i-1].X)>slopX||Math.Abs(ds[i].Y-ds[i-1].Y)>slopY) return false;
                // DOWN,UP,DBLCLK,UP,DOWN,UP is valid. Clicks==2 is ONE press.
                return ds[0].NativeCount==1 && ds.Any(e=>e.DoubleClickMsg && e.NativeCount==2);
            };
            switch(flow) {
                case "single": return v(d.Length==1&&target.Length==1&&d[0].Button=="left"&&d[0].NativeCount==1,"one released single click");
                case "multi": return v(d.Length==c.IntField("expect_count")&&multi(target,c.IntField("expect_count")),"N real down/up pairs and native DBLCLK within bounds");
                case "select_word": case "select_line": {
                    int n=flow=="select_word"?2:3;
                    // Windows EDIT can include the single word separator. Preserve raw data;
                    // this is an exact case-local allowance, not whitespace normalization.
                    bool selection=n==2?(obs.Selection=="alpha"||obs.Selection=="alpha\u0020")
                        :obs.Selection==GestureCatalog.SelectWordSentence;
                    return v(d.Length==n&&multi(d.Where(e=>e.Area=="text").ToArray(),n)&&selection,
                        n==2?"native word selection":WholeLineRequirement);
                }
                case "two_targets": return v(d.Length==2&&d.Select(e=>e.Area).SequenceEqual(new[]{"zoneA","zoneB"})&&d.All(e=>e.Button=="left"&&e.NativeCount==1),"A then B released singles");
                case "two_positions": return v(d.Length==2&&target.Length==2&&d.All(e=>e.Button=="left"&&e.NativeCount==1)&&Math.Abs(d[1].X-d[0].X)>=200,"far-apart released singles >=200px");
                case "right_between": return v(d.Length==3&&target.Length==3&&d.Select(e=>e.Button).SequenceEqual(new[]{"left","right","left"})&&d.All(e=>e.NativeCount==1),"left/right/left resets");
                case "slow_two": return v(d.Length==2&&target.Length==2&&d.All(e=>e.Button=="left"&&e.NativeCount==1)&&d[1].TMs-d[0].TMs>=2000,"released singles >=2000ms apart");
                default: return v(false,"unknown flow");
            }
        }
        private static bool SegmentNear(GestureEvent a,GestureEvent b,GestureZone z) {
            double lo=0,hi=1;
            double[] origin={a.X,a.Y},delta={b.X-a.X,b.Y-a.Y},low={z.X+z.W/2-60,z.Y+z.H/2-60};
            for(int i=0;i<2;i++) {
                if(Math.Abs(delta[i])<0.0001) {if(origin[i]<low[i]||origin[i]>low[i]+120)return false;}
                else {double t1=(low[i]-origin[i])/delta[i],t2=(low[i]+120-origin[i])/delta[i];lo=Math.Max(lo,Math.Min(t1,t2));hi=Math.Min(hi,Math.Max(t1,t2));if(lo>hi)return false;}
            }
            return true;
        }
        public static GestureVerdict Drag(GestureCase c, IList<GestureEvent> events, GestureObserved obs, GestureZone[] zones) {
            Func<bool,string,GestureVerdict> v=(ok,why)=>new GestureVerdict(ok,why,"held_moves="+events.Count(e=>e.Kind=="move"&&e.Held)+" selection='"+obs.Selection+"'");
            var downs=events.Where(e=>e.Kind=="down").ToArray();var ups=events.Where(e=>e.Kind=="up").ToArray();
            if(!ValidStream(events,true)||downs.Length!=1||ups.Length!=1||downs[0].Button!="left") return v(false,"one ordered press/motion/release required");
            string area=c.Field("flow")=="text_select"?"text":"canvas";
            var moves=events.Where(e=>e.Kind=="move"&&e.Area==area&&e.Held&&e.Button=="left").ToArray();
            if(moves.Length==0||downs[0].Area!=area||ups[0].Area!=area) return v(false,"held motion and released up in owned target required");
            if(c.Field("flow")=="text_select") return v(obs.Selection=="quick brown","exact visible drag selection");
            var start=zones.FirstOrDefault(z=>z.Name=="START");var end=zones.FirstOrDefault(z=>z.Name=="END");
            if(start==null||end==null||!start.Contains(downs[0])||!end.Contains(ups[0])) return v(false,"START press and END release required");
            var pts=new List<GestureEvent>();pts.Add(downs[0]);pts.AddRange(moves);pts.Add(ups[0]);
            double spanX=pts.Max(e=>e.X)-pts.Min(e=>e.X),spanY=pts.Max(e=>e.Y)-pts.Min(e=>e.Y);
            if(c.Field("axis")=="h") return v(spanX>=Math.Abs(end.X-start.X)*0.6,"horizontal START->END");
            if(c.Field("axis")=="v") return v(spanY>=Math.Abs(end.Y-start.Y)*0.6,"vertical START->END");
            if(c.Field("path")=="polyline") {
                var wp=zones.First(z=>z.Name=="WAYPOINT");
                return v(pts.Zip(pts.Skip(1),(a,b)=>SegmentNear(a,b,wp)).Any(hit=>hit),"held path through WAYPOINT within 60px");
            }
            if(c.Field("path")=="curve") {
                int changes=0,last=0;
                for(int i=1;i<moves.Length;i++) {int sign=Math.Sign(moves[i].X-moves[i-1].X);if(sign!=0){if(last!=0&&sign!=last)changes++;last=sign;}}
                return v(moves.Length>=3&&changes>=2,"two observed horizontal direction changes required; coalescing tolerated, absent evidence not invented");
            }
            if(c.Field("span")=="short") return v(spanX>=40||spanY>=40,"short drag displacement >=40px");
            if(c.Field("span")=="long") return v(spanX>=500&&ups[0].TMs-downs[0].TMs>=c.IntField("min_ms"),"long drag span >=500px and hold >=750ms");
            if(c.Field("release")=="inner_edge") {
                var u=ups[0];double edge=Math.Min(Math.Min(Math.Abs(u.X-end.X),Math.Abs(u.X-end.X-end.W)),Math.Min(Math.Abs(u.Y-end.Y),Math.Abs(u.Y-end.Y-end.H)));
                double dx=u.X-end.X-end.W/2,dy=u.Y-end.Y-end.H/2;
                return v(edge<=45&&Math.Sqrt(dx*dx+dy*dy)>=40,"release inside edge band <=45px, >=40px from center");
            }
            return v(false,"unknown drag spec");
        }
        public static GestureVerdict Scroll(GestureCase c, IList<GestureEvent> events, GestureObserved o) {
            int av=o.AVEnd-o.AVStart,ah=o.AHEnd-o.AHStart,bv=o.BVEnd-o.BVStart,bh=o.BHEnd-o.BHStart;
            var wheel=events.Where(e=>e.Kind=="wheel").ToArray();
            string summary="A v:"+o.AVStart+"->"+o.AVEnd+" h:"+o.AHStart+"->"+o.AHEnd+" B v:"+o.BVStart+"->"+o.BVEnd+" h:"+o.BHStart+"->"+o.BHEnd;
            Func<bool,string,GestureVerdict> v=(ok,why)=>new GestureVerdict(ok,why,summary);
            if(wheel.Any(e=>!Finite(e.Dx)||!Finite(e.Dy)||!Finite(e.TMs)||e.TMs<0)) return v(false,"invalid native wheel");
            string flow=c.Field("flow");
            if(flow=="zero_delta"||flow=="invalid_args") return v(av==0&&ah==0&&bv==0&&bh==0&&wheel.Length==0,"no wheel events or displacement; not a scrolling success");
            if(wheel.Length==0) return v(false,"no native wheel events");
            string target=c.Field("panel")=="B"?"panelB":"panelA";
            if(wheel.Any(e=>e.Area!=target)) return v(false,"wrong panel received wheel");
            double dx=wheel.Sum(e=>e.Dx),dy=-wheel.Sum(e=>e.Dy),minMove=c.IntField("ticks")<=1?15:45;
            if(target=="panelB") return v(av==0&&ah==0&&bh==0&&bv>=minMove&&dy>0,"B down, A untouched");
            if(bv!=0||bh!=0) return v(false,"Panel B changed");
            string axis=c.Field("axis"),dir=c.Field("dir");
            if(axis=="v"&&dir=="down") {
                bool saturated=flow=="saturate"&&o.AVMax>0&&o.AVEnd>=o.AVMax&&av>0;
                return v(ah==0&&av>0&&(flow!="saturate"||saturated)&&(saturated||av>=minMove)&&dy>0,"A down with actual displacement, saturation explicit");
            }
            if(axis=="v"&&dir=="up") return v(ah==0&&av<=-minMove&&dy<0,"A up actual displacement");
            if(axis=="h") return v(av==0&&Math.Abs(ah)>=minMove&&(dir=="right"?ah>0&&dx>0:ah<0&&dx<0),"A horizontal native HWHEEL and actual displacement");
            if(axis=="both") return v(av>=minMove&&ah>=minMove&&dx>0&&dy>0,"A right/down actual displacement");
            return v(false,"unknown scroll spec");
        }
    }
}
