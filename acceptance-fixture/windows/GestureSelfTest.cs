// Synthetic-only cases; coordinator CC+GLM must execute. No controls constructed.
using System;
using System.Collections.Generic;
using System.Linq;
namespace AcceptanceFixture {
    static class GestureSelfTest {
        private static GestureEvent E(string kind,string area="target",int count=1,double t=0,double x=400,double y=150,bool held=false,bool dbl=false) {
            return new GestureEvent {Kind=kind,Area=area,NativeCount=count,TMs=t,X=x,Y=y,Held=held,DoubleClickMsg=dbl};
        }
        private static GestureObserved Blank() {return new GestureObserved {AVMax=100,AHMax=100,BVMax=100,BHMax=100};}
        public static List<SelfTestResult> Run() {
            var results=new List<SelfTestResult>();
            Action<string,bool> add=(name,ok)=>results.Add(new SelfTestResult("gesture-"+name,ok,"synthetic; not GUI evidence"));
            foreach(string suite in GestureCatalog.Suites) {
                var cases=GestureCatalog.CasesFor(suite);add(suite+"-count-10",cases.Length==10);
                add(suite+"-unique-ids",cases.Select(c=>c.Id).Distinct().Count()==cases.Length);
            }
            add("all-30-unique",GestureCatalog.Suites.SelectMany(s=>GestureCatalog.CasesFor(s)).Select(c=>c.Id).Distinct().Count()==30);
            var o=Blank();var one=GestureCatalog.MulticlickCases[0];var triple=GestureCatalog.MulticlickCases[2];
            Func<GestureCase,GestureEvent[],bool> click=(c,es)=>GestureJudge.Multiclick(c,es,o,500,6,6).Matched;
            add("single-pass",click(one,new[]{E("down"),E("up")}));
            add("single-missing-up-fails",!click(one,new[]{E("down")}));
            var three=new[]{E("down",t:0),E("up",t:10),E("down",count:2,t:80,dbl:true),E("up",count:2,t:90),E("down",count:1,t:160),E("up",t:170)};
            add("third-native-count1-one-dblclk-valid",click(triple,three));
            add("clicks2-not-two-downs",!click(triple,three.Take(4).ToArray()));
            var noDbl=three.Select(e=>new GestureEvent {Kind=e.Kind,Area=e.Area,NativeCount=e.NativeCount,TMs=e.TMs,X=e.X,Y=e.Y}).ToArray();
            add("triple-no-dblclk-fails",!click(triple,noDbl));
            var late=three.ToArray();late[4]=E("down",t:2100);late[5]=E("up",t:2110);
            add("triple-timeout-fails",!click(triple,late));
            var far=three.ToArray();far[4]=E("down",t:160,x:600);far[5]=E("up",t:170,x:600);
            add("triple-position-change-fails",!click(triple,far));
            add("slow-under-two-seconds-fails",!click(GestureCatalog.MulticlickCases[8],new[]{E("down"),E("up"),E("down",t:1800),E("up",t:1810)}));
            add("invalid-count-no-events",click(GestureCatalog.MulticlickCases[9],new GestureEvent[0]));
            add("invalid-count-any-input-fails",!click(GestureCatalog.MulticlickCases[9],new[]{E("down"),E("up")}));
            var drag=GestureCatalog.DragCases[0];var zones=GestureLayout.Zones(drag);
            var down=E("down","canvas",t:0,x:110,y:130,held:true);var move=E("move","canvas",t:50,x:400,y:130,held:true);var up=E("up","canvas",t:100,x:690,y:130);
            add("coalesced-one-motion-valid",GestureJudge.Drag(drag,new[]{down,move,up},o,zones).Matched);
            add("endpoint-only-fails",!GestureJudge.Drag(drag,new[]{down,up},o,zones).Matched);
            add("wrong-order-fails",!GestureJudge.Drag(drag,new[]{move,down,up},o,zones).Matched);
            add("drag-no-release-fails",!GestureJudge.Drag(drag,new[]{down,move},o,zones).Matched);
            add("post-release-held-motion-fails",!GestureJudge.Drag(drag,new[]{down,move,up,E("move","canvas",t:110,held:true)},o,zones).Matched);
            var selected=Blank();selected.Selection="quick brown";
            add("text-selection-without-release-fails",!GestureJudge.Drag(GestureCatalog.DragCases[8],new[]{E("down","text"),E("move","text",held:true)},selected,new GestureZone[0]).Matched);
            for(int i=0;i<4;i++) {
                var c=GestureCatalog.DragCases[i];var z=GestureLayout.Zones(c);
                var ds=E("down","canvas",t:0,x:z[0].X+50,y:z[0].Y+30,held:true);
                var dm=E("move","canvas",t:50,x:(z[0].X+z[1].X)/2+50,y:(z[0].Y+z[1].Y)/2+30,held:true);
                var du=E("up","canvas",t:100,x:z[1].X+50,y:z[1].Y+30);
                add(c.Id+"-direction-valid",GestureJudge.Drag(c,new[]{ds,dm,du},o,z).Matched);
            }
            var shortCase=GestureCatalog.DragCases[5];var shortZones=GestureLayout.Zones(shortCase);
            add("short-coalesced-one-motion",GestureJudge.Drag(shortCase,new[]{E("down","canvas",x:350,y:140,held:true),E("move","canvas",t:30,x:420,y:140,held:true),E("up","canvas",t:60,x:490,y:140)},o,shortZones).Matched);
            var edgeCase=GestureCatalog.DragCases[7];var edgeZones=GestureLayout.Zones(edgeCase);
            add("edge-release-valid",GestureJudge.Drag(edgeCase,new[]{E("down","canvas",x:130,y:140,held:true),E("move","canvas",t:30,x:400,y:140,held:true),E("up","canvas",t:60,x:610,y:140)},o,edgeZones).Matched);
            add("center-release-not-edge",!GestureJudge.Drag(edgeCase,new[]{E("down","canvas",x:130,y:140,held:true),E("move","canvas",t:30,x:400,y:140,held:true),E("up","canvas",t:60,x:690,y:155)},o,edgeZones).Matched);
            var curveCase=GestureCatalog.DragCases[9];var curveZones=GestureLayout.Zones(curveCase);
            add("curve-valid-two-reversals",GestureJudge.Drag(curveCase,new[]{E("down","canvas",x:100,y:240,held:true),E("move","canvas",t:20,x:300,y:200,held:true),E("move","canvas",t:40,x:220,y:150,held:true),E("move","canvas",t:60,x:500,y:100,held:true),E("move","canvas",t:80,x:450,y:60,held:true),E("up","canvas",t:100,x:740,y:50)},o,curveZones).Matched);
            add("word-selection-native-valid",GestureJudge.Multiclick(GestureCatalog.MulticlickCases[3],new[]{E("down","text"),E("up","text",t:10),E("down","text",2,80,dbl:true),E("up","text",2,90)},new GestureObserved {Selection="alpha"},500,6,6).Matched);
            add("text-drag-valid",GestureJudge.Drag(GestureCatalog.DragCases[8],new[]{E("down","text"),E("move","text",t:20,held:true),E("up","text",t:40)},selected,new GestureZone[0]).Matched);
            var wheel=new GestureEvent {Kind="wheel",Area="panelA",Button="other",Dy=-360,TMs=10};
            var moved=Blank();moved.AVEnd=90;
            add("scroll-down-valid",GestureJudge.Scroll(GestureCatalog.ScrollCases[0],new[]{wheel},moved).Matched);
            add("scroll-no-offset-fails",!GestureJudge.Scroll(GestureCatalog.ScrollCases[0],new[]{wheel},o).Matched);
            var reverse=new GestureEvent {Kind="wheel",Area="panelA",Dy=360,TMs=10};
            add("scroll-raw-sign-not-normalized-away",!GestureJudge.Scroll(GestureCatalog.ScrollCases[0],new[]{reverse},moved).Matched);
            moved.BHEnd=5;add("other-panel-horizontal-change-fails",!GestureJudge.Scroll(GestureCatalog.ScrollCases[0],new[]{wheel},moved).Matched);
            var bottom=Blank();bottom.AVStart=bottom.AVEnd=100;
            add("saturation-no-displacement-fails",!GestureJudge.Scroll(GestureCatalog.ScrollCases[7],new[]{wheel},bottom).Matched);
            var right=Blank();right.AHEnd=90;
            var hwheel=new GestureEvent {Kind="wheel",Area="panelA",Dx=120,NativeMessage=0x020E};
            add("horizontal-real-hwheel-semantics",GestureJudge.Scroll(GestureCatalog.ScrollCases[2],new[]{hwheel},right).Matched);
            add("zero-no-events-valid",GestureJudge.Scroll(GestureCatalog.ScrollCases[8],new GestureEvent[0],o).Matched);
            add("zero-wheel-event-fails",!GestureJudge.Scroll(GestureCatalog.ScrollCases[8],new[]{wheel},o).Matched);
            add("invalid-any-wheel-fails",!GestureJudge.Scroll(GestureCatalog.ScrollCases[9],new[]{hwheel},o).Matched);
            var longCase=GestureCatalog.DragCases[6];var longZones=GestureLayout.Zones(longCase);
            add("long-duration-valid",GestureJudge.Drag(longCase,new[]{E("down","canvas",t:0,x:80,y:140,held:true),E("move","canvas",t:400,x:400,y:140,held:true),E("up","canvas",t:800,x:760,y:140)},o,longZones).Matched);
            add("long-too-short-fails",!GestureJudge.Drag(longCase,new[]{E("down","canvas",t:0,x:80,y:140,held:true),E("move","canvas",t:100,x:400,y:140,held:true),E("up","canvas",t:200,x:760,y:140)},o,longZones).Matched);
            var polyCase=GestureCatalog.DragCases[4];var polyZones=GestureLayout.Zones(polyCase);
            add("coalesced-waypoint-segment-valid",GestureJudge.Drag(polyCase,new[]{E("down","canvas",t:0,x:110,y:155,held:true),E("move","canvas",t:30,x:350,y:60,held:true),E("move","canvas",t:60,x:500,y:60,held:true),E("up","canvas",t:100,x:750,y:235)},o,polyZones).Matched);
            add("json-escape",GestureExporter.JsonString("\"\\\n\t") == "\"\\\"\\\\\\u000a\\u0009\"");
            return results;
        }
    }
}
