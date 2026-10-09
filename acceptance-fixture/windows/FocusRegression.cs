// Pure regression source only. CC executes. StageA FocusSelfTest stays unchanged.
using System;
using System.Collections.Generic;
using System.Linq;
namespace AcceptanceFixture {
    static class FocusRegression {
        static FocusEvent E(int msg,string role,int seq,long wp=0) {
            return new FocusEvent {Index=seq,Message=msg,Role=role,Handle=role=="A.edit"?11:role=="B.edit"?21:role=="B"?20:10,
                WParam=wp,LParam=0,NativeMs=(uint)(seq*100),TMs=seq*100,Kind=FocusMessages.Kind(msg),
                Source="WndProc/own",FocusHandle=role=="B.edit"?21:11};
        }
        static List<FocusEvent> Transfer() {
            return new List<FocusEvent>{E(7,"A.edit",1),E(0x201,"A.edit",2,1),E(0x202,"A.edit",3),
                E(6,"B",4,1),E(7,"B.edit",5),E(0x201,"B.edit",6,1),E(0x202,"B.edit",7)};
        }
        static List<FocusEvent> Scenario(int n,out FocusObserved o) {
            var e=new List<FocusEvent>();o=new FocusObserved{Released=true,ParentEnabled=true};
            var handles=new Dictionary<string,long>{{"A",10},{"A.edit",11},{"A.open_modal",12},{"A.open_menu",13},{"A.restore",14},
                {"B",20},{"B.edit",21},{"B.minimize",22},{"D",30},{"D.edit",31},{"D.close",32}};
            Action<int,string,long> add=(msg,role,wp)=>{
                bool key=msg>=0x100&&msg<=0x106;
                e.Add(new FocusEvent{Index=e.Count+1,Message=msg,Kind=FocusMessages.Kind(msg),Role=role,Handle=handles[role],
                    WParam=wp,LParam=key?(msg==0x101?0xc0000001L:1):0,NativeMs=(uint)(e.Count*100),TMs=e.Count*100,
                    FocusHandle=handles[role],Source=key?"queue/own-EDIT":"WndProc/own"});
            };
            Action<string> click=role=>{add(0x201,role,1);add(0x202,role,0);};
            Action<string,string> type=(role,text)=>{foreach(char ch in text){add(0x100,role,231);add(0x102,role,ch);add(0x101,role,231);}};
            if(n==1){click("A.edit");add(6,"B",1);click("B.edit");}
            if(n==2){add(6,"B",1);click("B.edit");add(6,"A",1);click("A.edit");}
            if(n==3){add(6,"B",1);click("B.edit");type("B.edit","FOCUS");o.TextB="FOCUS";}
            if(n==4){click("A.edit");add(0x100,"A.edit",17);add(0x100,"A.edit",65);add(0x101,"A.edit",65);add(0x101,"A.edit",17);o.TextA="ALPHA BRAVO";o.SelectionALength=11;}
            if(n==5||n==6){click("A.open_modal");add(10,"A",0);add(6,"D",1);}
            if(n==5){click("D.edit");type("D.edit","MODAL");o.TextD="MODAL";o.ModalOpen=true;o.ParentEnabled=false;}
            if(n==6){click("D.close");add(16,"D",0);add(10,"A",1);click("A.edit");type("A.edit","BACK");o.TextA="BACK";}
            if(n==7){click("A.open_menu");add(0x211,"A",0);add(0x212,"A",0);click("A.edit");type("A.edit","MENU");o.TextA="MENU";}
            if(n==8){click("B.minimize");add(5,"B",1);click("A.restore");add(5,"B",0);add(6,"B",1);click("B.edit");type("B.edit","RESTORED");o.TextB="RESTORED";}
            if(n==9){click("A.edit");type("A.edit","OWNED");o.TextA="OWNED";}
            return e;
        }
        static bool Bad(params string[] args) {try{BasicArguments.Parse(args);return false;}catch(ArgumentException){return true;}}
        public static List<SelfTestResult> Run() {
            var r=new List<SelfTestResult>();Action<string,bool> add=(n,b)=>r.Add(new SelfTestResult("focus-"+n,b,"pure synthetic; not GUI proof"));
            add("catalog-ten",FocusCatalog.Cases.Length==10);
            add("unique-ids",FocusCatalog.Cases.Select(x=>x.Id).Distinct().Count()==10);
            add("focus-export-argument",BasicArguments.Parse(new[]{"--export-focus-cases=x"}).FocusExportPath=="x");
            add("focus-export-conflict",Bad("--self-test","--export-focus-cases=x"));
            add("focus-export-b2-conflict",Bad("--export-platform-cases=x","--export-focus-cases=y"));
            add("focus-export-missing",Bad("--export-focus-cases"));
            add("focus-export-duplicate",Bad("--export-focus-cases=x","--export-focus-cases=y"));
            add("unknown-suite-still-rejected",Bad("--suite=focus-bogus"));
            var o=new FocusObserved {Released=true,ParentEnabled=true};
            var ev=Transfer();
            add("a-to-b-native",FocusJudge.Check(FocusCatalog.Cases[0],ev,o).State=="matched");
            var missing=ev.Where(x=>x.Message!=6).ToList();for(int i=0;i<missing.Count;i++)missing[i].Index=i+1;
            add("activation-required",FocusJudge.Check(FocusCatalog.Cases[0],missing,o).State!="matched");
            ev=Transfer();ev[5].Role="A.edit";add("wrong-window",FocusJudge.Check(FocusCatalog.Cases[0],ev,o).State!="matched");
            ev=Transfer();ev[5].WParam=0;add("raw-button-mismatch",FocusJudge.Check(FocusCatalog.Cases[0],ev,o).State!="matched");
            ev=Transfer();ev[6].Index=12;add("missing-event",FocusJudge.Check(FocusCatalog.Cases[0],ev,o).State!="matched");
            ev=Transfer();o.Released=false;add("held-input",FocusJudge.Check(FocusCatalog.Cases[0],ev,o).State!="matched");o.Released=true;
            add("case9-empty-not-pass",FocusJudge.Check(FocusCatalog.Cases[8],new List<FocusEvent>(),o).State!="matched");
            add("case10-zero-only-needs-trace",FocusJudge.Check(FocusCatalog.Cases[9],new List<FocusEvent>(),o).State=="needs_tool_evidence");
            var typed=E(0x102,"A.edit",1,88);typed.Source="queue/own-EDIT";typed.LParam=1;
            add("case10-text-rejected",FocusJudge.Check(FocusCatalog.Cases[9],new List<FocusEvent>{typed},o).State=="mismatch");
            typed.FocusHandle=99;add("wrong-key-focus",!FocusMessages.RawValid(typed));
            add("modal-missing-native",FocusJudge.Check(FocusCatalog.Cases[4],new List<FocusEvent>(),o).State!="matched");
            add("restore-missing-native",FocusJudge.Check(FocusCatalog.Cases[7],new List<FocusEvent>(),o).State!="matched");
            for(int n=1;n<=10;n++) {
                FocusObserved state;var scenario=Scenario(n,out state);
                string expected=n==9?"needs_supervisor":n==10?"needs_tool_evidence":"matched";
                add("semantic-case-"+n,FocusJudge.Check(FocusCatalog.Cases[n-1],scenario,state).State==expected);
            }
            FocusObserved observed;var raw=Scenario(3,out observed);
            raw.First(x=>x.Kind=="char").WParam=88;
            add("raw-char-payload-mismatch",FocusJudge.Check(FocusCatalog.Cases[2],raw,observed).State!="matched");
            raw=Scenario(1,out observed);raw.Last().FocusHandle=999;
            add("mouseup-wrong-focus",FocusJudge.Check(FocusCatalog.Cases[0],raw,observed).State!="matched");
            raw=Scenario(5,out observed);observed.ParentEnabled=true;
            add("nonmodal-cannot-pass-modal",FocusJudge.Check(FocusCatalog.Cases[4],raw,observed).State!="matched");
            raw=Scenario(7,out observed);raw.RemoveAll(x=>x.Kind=="menu_exit");
            add("menu-exit-required",FocusJudge.Check(FocusCatalog.Cases[6],raw,observed).State!="matched");
            return r;
        }
    }
}
