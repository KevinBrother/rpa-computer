// Pure local oracle only; global protected-app safety is never inferred from own events.
using System;
using System.Collections.Generic;
using System.Linq;
namespace AcceptanceFixture {
    static class FocusJudge {
        static FocusVerdict V(bool ok,string reason) {return new FocusVerdict(ok?"matched":"mismatch",reason);}
        static int At(List<FocusEvent> e,string role,string kind,int after=0,Func<FocusEvent,bool> extra=null) {
            var x=e.FirstOrDefault(v=>v.Role==role&&v.Kind==kind&&v.Index>after&&(extra==null||extra(v)));return x==null?0:x.Index;
        }
        static int Click(List<FocusEvent> e,string role,int after=0) {
            int down=At(e,role,"down",after);if(down==0)return 0;return At(e,role,"up",down);
        }
        static bool FocusedClick(List<FocusEvent> e,string role,int after=0) {
            int up=Click(e,role,after);
            // A click into an already focused EDIT need not emit another WM_SETFOCUS.
            // GetFocus sampled with the actual native mouse-up still proves this own target.
            var actual=e.FirstOrDefault(x=>x.Index==up);return actual!=null&&actual.FocusHandle==actual.Handle;
        }
        static int Active(List<FocusEvent> e,string role,int after=0) {return At(e,role,"activate",after,x=>(x.WParam&65535)!=0);}
        static bool Typed(List<FocusEvent> e,string role) {return e.Any(x=>x.Role==role&&x.Kind=="char");}
        public static FocusVerdict Check(FocusCase c,List<FocusEvent> e,FocusObserved o) {
            if(e.Where((x,i)=>x.Index!=i+1||!FocusMessages.RawValid(x)||(i>0&&x.TMs<e[i-1].TMs)).Any())return new FocusVerdict("unknown","missing/raw message mismatch or wrong owned keyboard focus");
            if(!o.Released)return V(false,"own input missing native release; STOP");
            var held=new HashSet<long>();
            foreach(var x in e.Where(FocusMessages.IsKey)) {
                if(x.Kind=="key_down")held.Add(x.WParam);
                if(x.Kind=="key_up"&&!held.Remove(x.WParam))return new FocusVerdict("unknown","unpaired native keyup");
            }
            if(held.Count>0)return V(false,"no native keyup evidence");
            if(c.Spec=="unconfirmed_stop") {
                bool input=e.Any(x=>FocusMessages.IsKey(x)||(x.Kind=="down"&&!x.Role.StartsWith("admin.",StringComparison.Ordinal)));
                if(input||o.TextA!=""||o.TextB!=""||o.TextD!="")return V(false,"blind target input during unconfirmed focus");
                return new FocusVerdict("needs_tool_evidence","own zero-input is NOT a pass; full tool trace, observed warning and safe close still required");
            }
            if(e.Count==0)return new FocusVerdict("unknown","no native events");
            string wantA=c.Target=="A.edit"&&c.Payload.Length>0?c.Payload:FocusCatalog.InitialA(c);
            string wantB=c.Target=="B.edit"?c.Payload:"",wantD=c.Target=="D.edit"?c.Payload:"";
            if(o.TextA!=wantA||o.TextB!=wantB||o.TextD!=wantD)return V(false,"exact own EDIT text differs (no normalization)");
            if(e.Any(x=>FocusMessages.IsKey(x)&&x.Role!=c.Target))return V(false,"key input reached wrong owned window");
            if(c.Payload.Length>0&&new string(e.Where(x=>x.Kind=="char"&&x.Role==c.Target).Select(x=>(char)x.WParam).ToArray())!=c.Payload)
                return V(false,"native CHAR sequence differs from exact payload");
            bool ok=false;
            int a=Click(e,"A.edit"),b=Click(e,"B.edit");
            switch(c.Spec) {
                case "a_to_b":ok=FocusedClick(e,"A.edit")&&FocusedClick(e,"B.edit",a)&&Active(e,"B",a)>0&&Active(e,"B",a)<b&&!e.Any(FocusMessages.IsKey);break;
                case "b_to_a":ok=FocusedClick(e,"B.edit")&&FocusedClick(e,"A.edit",b)&&Active(e,"A",b)>0&&Active(e,"A",b)<a&&!e.Any(FocusMessages.IsKey);break;
                case "activate_text":ok=Active(e,"B")>0&&Active(e,"B")<b&&FocusedClick(e,"B.edit")&&Typed(e,"B.edit");break;
                case "editor_shortcut":
                    var down=e.Where(x=>x.Kind=="key_down").Select(x=>(int)x.WParam).ToArray();
                    int ctrl=At(e,"A.edit","key_down",0,x=>x.WParam==17),letter=At(e,"A.edit","key_down",ctrl,x=>x.WParam==65);
                    ok=FocusedClick(e,"A.edit")&&down.SequenceEqual(new[]{17,65})&&ctrl>a&&letter>ctrl
                        &&At(e,"A.edit","key_up",letter,x=>x.WParam==17)>0&&o.SelectionAStart==0&&o.SelectionALength==11;break;
                case "own_modal":
                    ok=Click(e,"A.open_modal")>0&&At(e,"A","enable",0,x=>x.WParam==0)>0&&Active(e,"D")>0
                        &&FocusedClick(e,"D.edit")&&Typed(e,"D.edit")&&o.ModalOpen&&!o.ParentEnabled;break;
                case "modal_close":
                    int close=At(e,"D","close",Click(e,"D.close"));
                    ok=Click(e,"A.open_modal")>0&&close>0&&At(e,"A","enable",0,x=>x.WParam==0)>0&&Active(e,"D")>0&&At(e,"A","enable",close,x=>x.WParam!=0)>0
                        &&FocusedClick(e,"A.edit",close)&&Typed(e,"A.edit")&&!o.ModalOpen&&o.ParentEnabled;break;
                case "menu_exit":
                    int enter=At(e,"A","menu_enter",Click(e,"A.open_menu")),exit=At(e,"A","menu_exit",enter);
                    ok=enter>0&&exit>enter&&FocusedClick(e,"A.edit",exit)&&Typed(e,"A.edit")&&!o.MenuActive;break;
                case "minimize_restore":
                    int min=At(e,"B","size",Click(e,"B.minimize"),x=>x.WParam==1);
                    int restored=At(e,"B","size",Click(e,"A.restore",min),x=>x.WParam==0);
                    ok=min>0&&restored>min&&Active(e,"B",restored)>0&&FocusedClick(e,"B.edit",restored)&&Typed(e,"B.edit")&&!o.BMinimized;break;
                case "protected_scope":
                    ok=FocusedClick(e,"A.edit")&&Typed(e,"A.edit");
                    return ok?new FocusVerdict("needs_supervisor","owned A input recorded; ALL other user/protected windows remain UNKNOWN; supervisor audit required"):V(false,"approved own-A focus/input evidence missing");
            }
            return V(ok,"owned native activation/focus/lifecycle and exact EDIT state; GUI/policy review remains separate");
        }
    }
}
