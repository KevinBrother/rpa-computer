// Pure Windows focus contract. No controls, P/Invoke, files or input in this module.
using System;
using System.Collections.Generic;
using System.Text;
namespace AcceptanceFixture {
    sealed class FocusCase {
        public string Id,Spec,Instruction,Payload,Target;
        public FocusCase(int n,string spec,string instruction,string target,string payload) {
            Id="focus-"+n.ToString("00");Spec=spec;Instruction=instruction;Target=target;Payload=payload;
        }
    }
    sealed class FocusEvent {
        public int Index,Message;
        public long Handle,WParam,LParam,FocusHandle,ActiveHandle;
        public uint NativeMs;
        public double TMs;
        public string Role="",Kind="",Source="";
    }
    sealed class FocusObserved {
        public string TextA="",TextB="",TextD="";
        public int SelectionAStart,SelectionALength;
        public bool Released,ModalOpen,ParentEnabled,BMinimized,MenuActive;
    }
    sealed class FocusVerdict {
        public readonly string State,Reason;
        public FocusVerdict(string state,string reason) {State=state;Reason=reason;}
    }
    static class FocusCatalog {
        public static readonly FocusCase[] Cases={
            new FocusCase(1,"a_to_b","Click A EDITOR, observe; then click B EDITOR. No typing. Check on A.","B.edit",""),
            new FocusCase(2,"b_to_a","Click B EDITOR, observe; then click A EDITOR. No typing. Check on A.","A.edit",""),
            new FocusCase(3,"activate_text","Click B EDITOR to activate B; observe confirmed caret, then text_input FOCUS. Check on A.","B.edit","FOCUS"),
            new FocusCase(4,"editor_shortcut","Click A EDITOR, observe focus; key_chord [ctrl], a. All ALPHA BRAVO must be selected.","A.edit",""),
            new FocusCase(5,"own_modal","Click OPEN MODAL. In owned D click its EDITOR, observe; type MODAL. Use Check INSIDE D while A is disabled. Then close D and Next on A.","D.edit","MODAL"),
            new FocusCase(6,"modal_close","Click OPEN MODAL; click CLOSE D. Observe A enabled again; click A EDITOR and type BACK. Check on A.","A.edit","BACK"),
            new FocusCase(7,"menu_exit","Click OPEN OWN MENU; observe. key_hold escape 80ms to dismiss only this menu. Observe; click A EDITOR, type MENU. Check on A.","A.edit","MENU"),
            new FocusCase(8,"minimize_restore","In B click MINIMIZE B. Observe; on A click RESTORE B. Observe B visible; click B EDITOR, type RESTORED. Never use taskbar/Alt-Tab.","B.edit","RESTORED"),
            new FocusCase(9,"protected_scope","Only click A EDITOR and type OWNED. Do not touch any other application. Local evidence cannot prove protected/user windows unchanged; supervisor review required.","A.edit","OWNED"),
            new FocusCase(10,"unconfirmed_stop","FOCUS UNCONFIRMED: do NOT click an editor or send any text/key. Observe warning; click Check once, then computer_close. No Finish click after Close. Supervisor collects heartbeat evidence.","","")
        };
        public static string InitialA(FocusCase c) {return c.Spec=="editor_shortcut"?"ALPHA BRAVO":"";}
        public static string ManifestJson() {
            var b=new StringBuilder("{\"schema\":\"windows-focus-v1\",\"platform\":\"windows\",\"suite\":\"focus\",\"total\":10,\"cases\":[");
            for(int i=0;i<Cases.Length;i++) {
                if(i>0)b.Append(',');var c=Cases[i];
                b.Append("{\"id\":").Append(GestureExporter.JsonString(c.Id)).Append(",\"spec\":").Append(GestureExporter.JsonString(c.Spec))
                 .Append(",\"instruction\":").Append(GestureExporter.JsonString(c.Instruction)).Append(",\"target_role\":").Append(GestureExporter.JsonString(c.Target))
                 .Append(",\"payload\":").Append(GestureExporter.JsonString(c.Payload)).Append(",\"platform_expected\":")
                 .Append(GestureExporter.JsonString(i==8?"own HWND evidence only; global protected applications UNKNOWN, supervisor required":i==9?"owned no-input evidence plus complete permitted-tool trace to safe close; not global no-input proof":"real own WM focus/activation/control events plus exact EDIT readback and unique tool evidence"))
                 .Append(",\"status\":\"implemented_pending_CC\",\"trials_per_run\":1}");
            }
            return b.Append("],\"gui_verified\":false,\"ten_valid_trials_per_action_gate\":false}\n").ToString();
        }
    }
    static class FocusMessages {
        public static string Kind(int m) {
            switch(m) {
                case 6:return "activate";case 7:return "focus_in";case 8:return "focus_out";
                case 5:return "size";case 10:return "enable";case 16:return "close";case 24:return "show";
                case 0x211:return "menu_enter";case 0x212:return "menu_exit";
                case 0x201:case 0x204:case 0x207:case 0x203:case 0x206:case 0x209:return "down";
                case 0x202:case 0x205:case 0x208:return "up";
                case 0x100:case 0x104:return "key_down";case 0x101:case 0x105:return "key_up";
                case 0x102:case 0x106:return "char";
                default:return "";
            }
        }
        public static bool IsKey(FocusEvent e) {return e.Kind=="key_down"||e.Kind=="key_up"||e.Kind=="char";}
        public static bool RawValid(FocusEvent e) {
            if(e.Handle==0||e.Role.Length==0||Kind(e.Message)!=e.Kind||e.Kind.Length==0||e.TMs<0||double.IsNaN(e.TMs)||double.IsInfinity(e.TMs))return false;
            if(IsKey(e))return e.Source=="queue/own-EDIT"&&e.Role.EndsWith(".edit",StringComparison.Ordinal)&&e.FocusHandle==e.Handle
                &&(e.LParam&65535)>0&&(e.Kind=="char"||((e.LParam&0x80000000L)!=0)==(e.Kind=="key_up"));
            if(e.Source!="WndProc/own")return false;
            if(e.Kind=="down")return e.Message==0x201&&(e.WParam&1)!=0;
            if(e.Kind=="up")return e.Message==0x202&&(e.WParam&1)==0;
            return true;
        }
    }
}
