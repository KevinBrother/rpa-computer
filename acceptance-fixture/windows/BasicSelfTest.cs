// PURE synthetic source only. CC executes; never construct controls here.
using System;
using System.Collections.Generic;
using System.Linq;
namespace AcceptanceFixture {
    static class BasicSelfTest {
        static BasicEvent Mouse(int msg, string kind, int index) {
            return new BasicEvent {Index=index, NativeMessage=msg, Kind=kind, Button="left", Area="target",
                X=120,Y=100,RawLParam=(100L<<16)|120,RawWParam=msg==0x201?1:0,
                NativeTimeMs=(uint)(100+index*100),TMs=index*100,Source="WndProc/own-pointer",OwnHandle=1,FocusHandle=1};
        }
        static BasicEvent Key(int msg, int vk, int index, uint time) {
            bool up=msg==0x101||msg==0x105;
            return new BasicEvent {Index=index,NativeMessage=msg,Kind=up?"key_up":"key_down",KeyCode=vk,
                RawWParam=vk,RawLParam=up?0xc0000001L:1,NativeTimeMs=time,TMs=time,
                Source="IMessageFilter/own-EDIT",Area="editor",OwnHandle=1,FocusHandle=1,Focused=true};
        }
        static List<BasicEvent> Sequence(params int[] sequence) {
            var result=new List<BasicEvent>();var held=new HashSet<int>();
            foreach(int token in sequence) {
                int vk=Math.Abs(token);bool chars=token>=1000;if(chars)vk-=1000;
                bool down=token>0&&!chars;
                if(down)held.Add(vk);
                var e=Key(chars?0x102:down?0x100:0x101,vk,result.Count+1,(uint)(100+result.Count*100));
                if(chars)e.Kind="char";
                e.Modifiers=(held.Contains(16)?1:0)|(held.Contains(17)?2:0)|(held.Contains(18)?4:0);
                if(token<0)held.Remove(vk);result.Add(e);
            }
            return result;
        }
        static bool BadArgs(params string[] args) {
            try { BasicArguments.Parse(args); return false; } catch(ArgumentException) { return true; }
        }
        public static List<SelfTestResult> Run() {
            var r=new List<SelfTestResult>();
            Action<string,bool> add=(name,ok)=>r.Add(new SelfTestResult("basic-"+name,ok,"synthetic only; CC execution required"));
            foreach(string s in BasicCatalog.Suites) {
                add(s+"-10",BasicCatalog.CasesFor(s).Length==10);
                add(s+"-unique",BasicCatalog.CasesFor(s).Select(c=>c.Id).Distinct().Count()==10);
            }
            add("legacy-export-63",GestureExporter.ManifestJson().Split(new[]{"\"id\""},StringSplitOptions.None).Length-1==63);
            add("default-legacy",BasicArguments.Parse(new string[0]).Suite=="legacy");
            add("valid-pointer",BasicArguments.Parse(new[]{"--suite=pointer","--seed","42"}).Suite=="pointer");
            add("unknown-suite",BadArgs("--suite","bogus"));
            add("missing-value",BadArgs("--suite"));
            add("duplicate",BadArgs("--suite=pointer","--suite","keyboard"));
            add("unknown-option",BadArgs("--wat"));
            add("bad-seed",BadArgs("--seed=-1"));
            add("missing-path",BadArgs("--export-platform-cases="));
            add("conflicting-modes",BadArgs("--self-test","--export-cases","x"));
            add("boolean-value",BadArgs("--self-test=false"));
            var click=BasicCatalog.CasesFor("pointer")[3];
            var e=new List<BasicEvent>{Mouse(0x201,"down",1),Mouse(0x202,"up",2)};
            var o=new BasicObserved {Released=true,TargetHandle=1};
            add("real-click-structure",BasicJudge.Check(click,e,o).State=="matched");
            add("missing-up",BasicJudge.Check(click,e.Take(1).ToList(),o).State!="matched");
            e[0].NativeMessage=0x204;add("raw-message-mismatch",BasicJudge.Check(click,e,o).State!="matched");e[0].NativeMessage=0x201;
            e[0].X=121;add("raw-coordinate-mismatch",BasicJudge.Check(click,e,o).State!="matched");e[0].X=120;
            e[1].Index=3;add("missing-event-index",BasicJudge.Check(click,e,o).State!="matched");e[1].Index=2;
            e[1].TMs=-1;add("bad-time",BasicJudge.Check(click,e,o).State!="matched");e[1].TMs=200;
            add("held-not-release",BasicJudge.Check(click,e,new BasicObserved{Released=false}).State!="matched");
            add("padding-always-blocked",BasicJudge.Check(BasicCatalog.CasesFor("pointer")[8],e,o).State=="blocked");
            add("zero-not-rejection",BasicJudge.Check(BasicCatalog.CasesFor("pointer")[9],new List<BasicEvent>(),o).State=="needs_tool_evidence");
            var hold=BasicCatalog.CasesFor("keyboard")[6];
            var keys=new List<BasicEvent>{Key(0x100,16,1,100),Key(0x101,16,2,220)};
            add("short-hold-native-pair",BasicJudge.Check(hold,keys,o).State=="matched");
            add("hold-no-up",BasicJudge.Check(hold,keys.Take(1).ToList(),o).State!="matched");
            keys[0].Focused=false;add("wrong-focus",BasicJudge.Check(hold,keys,o).State!="matched");keys[0].Focused=true;
            keys[1].NativeTimeMs=110;add("short-duration",BasicJudge.Check(hold,keys,o).State!="matched");keys[1].NativeTimeMs=220;
            keys[1].RawLParam=1;add("raw-up-bit-mismatch",BasicJudge.Check(hold,keys,o).State!="matched");
            add("illegal-key-no-events-pending",BasicJudge.Check(BasicCatalog.CasesFor("keyboard")[9],new List<BasicEvent>(),o).State=="needs_tool_evidence");
            var pc=BasicCatalog.CasesFor("pointer");
            var move=new List<BasicEvent>{Mouse(0x200,"move",1)};move[0].Button="none";
            add("center-move",BasicJudge.Check(pc[0],move,o).State=="matched");
            add("scaled-needs-actual-observation",BasicJudge.Check(pc[2],move,o).State=="needs_tool_evidence");
            move[0].Area="edge";add("edge-move",BasicJudge.Check(pc[1],move,o).State=="matched");
            var right=new List<BasicEvent>{Mouse(0x204,"down",1),Mouse(0x205,"up",2)};
            right[0].Button=right[1].Button="right";right[0].RawWParam=2;
            add("right-without-menu",BasicJudge.Check(pc[4],right,o).State=="mismatch");
            o.ContextOpened=true;add("right-own-menu",BasicJudge.Check(pc[4],right,o).State=="matched");
            var middle=new List<BasicEvent>{Mouse(0x207,"down",1),Mouse(0x208,"up",2)};
            middle[0].Button=middle[1].Button="middle";middle[0].RawWParam=16;
            add("middle-message-pair",BasicJudge.Check(pc[5],middle,o).State=="matched");
            var inactive=new List<BasicEvent>{new BasicEvent{Index=1,Kind="activate",NativeMessage=0x21,
                Source="WndProc/own-Form",OwnHandle=2,TMs=100,ActiveBefore=false},Mouse(0x201,"down",2),Mouse(0x202,"up",3)};
            add("inactive-missing-preparation",BasicJudge.Check(pc[6],inactive,o).State!="matched");
            o.SecondaryPreparedInactive=true;o.SecondaryFormHandle=2;
            add("inactive-first-click",BasicJudge.Check(pc[6],inactive,o).State=="matched");
            inactive[1].OwnHandle=99;add("inactive-wrong-hwnd",BasicJudge.Check(pc[6],inactive,o).State!="matched");
            add("small-target",BasicJudge.Check(pc[7],e,o).State=="matched");
            e[0].Area="decoy";add("small-decoy-rejected",BasicJudge.Check(pc[7],e,o).State!="matched");e[0].Area="target";
            var kc=BasicCatalog.CasesFor("keyboard");
            o.Text="a";add("plain-key-char",BasicJudge.Check(kc[0],Sequence(65,1097,-65),o).State=="matched");
            o.Text="\r\n\t";add("enter-tab-native",BasicJudge.Check(kc[1],Sequence(13,1013,-13,9,1009,-9),o).State=="matched");
            o.Text="ALPHA BRAVO";o.SelectionStart=0;o.SelectionLength=11;
            add("ctrl-a-selection",BasicJudge.Check(kc[2],Sequence(17,65,-65,-17),o).State=="matched");
            o.SelectionStart=10;o.SelectionLength=1;
            add("shift-selection",BasicJudge.Check(kc[3],Sequence(16,37,-37,-16),o).State=="matched");
            o.Text="";o.CommandCount=1;
            add("own-alt-command",BasicJudge.Check(kc[4],Sequence(18,74,-74,-18),o).State=="matched");
            add("multi-modifier-command",BasicJudge.Check(kc[5],Sequence(17,16,74,-74,-16,-17),o).State=="matched");
            var longHold=new List<BasicEvent>{Key(0x100,16,1,100),Key(0x101,16,2,1000)};
            add("long-hold",BasicJudge.Check(kc[7],longHold,o).State=="matched");
            o.Text="b";o.SelectionLength=0;
            add("modifier-release-before-plain",BasicJudge.Check(kc[8],Sequence(17,65,-65,-17,66,1098,-66),o).State=="matched");
            add("modifier-still-held",BasicJudge.Check(kc[8],Sequence(17,65,-65,66,1098,-66,-17),o).State!="matched");
            add("both-export-modes-conflict",BadArgs("--export-cases=x","--export-platform-cases=y"));
            add("next-option-not-value",BadArgs("--suite","--self-test"));
            add("whitespace-suite",BadArgs("--suite= "));
            add("platform-export-argument",BasicArguments.Parse(new[]{"--export-platform-cases=x"}).PlatformExportPath=="x");
            return r;
        }
    }
}
