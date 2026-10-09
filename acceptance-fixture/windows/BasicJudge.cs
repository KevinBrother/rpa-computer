// Pure semantic decisions from raw owned-window events. No input synthesis.
using System;
using System.Collections.Generic;
using System.Linq;
namespace AcceptanceFixture {
    static class BasicJudge {
        static BasicVerdict V(bool ok,string reason) {return new BasicVerdict(ok?"matched":"mismatch",reason);}
        public static int CanonicalKey(int vk) {
            if(vk==160||vk==161)return 16;if(vk==162||vk==163)return 17;if(vk==164||vk==165)return 18;return vk;
        }
        public static string MouseKind(int msg) {
            if(msg==0x200)return "move";
            if(msg==0x201||msg==0x204||msg==0x207||msg==0x203||msg==0x206||msg==0x209)return "down";
            if(msg==0x202||msg==0x205||msg==0x208)return "up";
            return "";
        }
        public static string MouseButton(int msg) {
            if(msg>=0x201&&msg<=0x203)return "left";if(msg>=0x204&&msg<=0x206)return "right";
            if(msg>=0x207&&msg<=0x209)return "middle";return "none";
        }
        public static bool RawValid(BasicEvent e) {
            if(e.OwnHandle==0||e.TMs<0||double.IsNaN(e.TMs)||double.IsInfinity(e.TMs))return false;
            if(e.Kind=="activate")return e.NativeMessage==0x21&&e.Source=="WndProc/own-Form";
            if(e.Source=="WndProc/own-pointer") {
                int mask=e.Button=="left"?1:e.Button=="right"?2:e.Button=="middle"?16:0;
                if(e.Kind=="down"&&(e.RawWParam&mask)==0||e.Kind=="up"&&(e.RawWParam&mask)!=0)return false;
                return MouseKind(e.NativeMessage)==e.Kind&&MouseButton(e.NativeMessage)==e.Button
                    &&e.X==unchecked((short)(e.RawLParam&65535))&&e.Y==unchecked((short)((e.RawLParam>>16)&65535));
            }
            if(e.Source!="IMessageFilter/own-EDIT"||!e.Focused||e.FocusHandle!=e.OwnHandle||e.Area!="editor")return false;
            bool up=e.NativeMessage==0x101||e.NativeMessage==0x105;
            bool down=e.NativeMessage==0x100||e.NativeMessage==0x104;
            bool chars=e.NativeMessage==0x102||e.NativeMessage==0x106;
            if(!up&&!down&&!chars)return false;
            if(e.KeyCode!=e.RawWParam||e.Kind!=(up?"key_up":down?"key_down":"char"))return false;
            return (e.RawLParam&65535)>0 && (chars || ((e.RawLParam&0x80000000L)!=0)==up);
        }
        public static BasicVerdict Check(BasicCase c,List<BasicEvent> events,BasicObserved o) {
            if(c.Blocked)return new BasicVerdict("blocked","needs_capability: root has no screenshot-padding map/rejection evidence contract");
            if(events.Where((e,i)=>e.Index!=i+1||!RawValid(e)||(i>0&&e.TMs<events[i-1].TMs)).Any())return new BasicVerdict("unknown","missing/mismatched raw fields, order or owned focus");
            if(!o.Released)return V(false,"held input has no native release");
            if(c.Reject&&c.Suite=="keyboard"&&(o.Text!=c.InitialText||o.SelectionLength!=0))return V(false,"rejected key changed native EDIT state");
            if(c.Reject)return events.Count==0?new BasicVerdict("needs_tool_evidence","owned event0; explicit invalid_action/not_started still required"):V(false,"rejection case received target events");
            if(events.Count==0)return new BasicVerdict("unknown","no native events");
            return c.Suite=="pointer"?Pointer(c,events,o):Keyboard(c,events,o);
        }
        static BasicVerdict Pointer(BasicCase c,List<BasicEvent> e,BasicObserved o) {
            if(o.TargetHandle==0||e.Any(x=>x.Kind!="activate"&&x.OwnHandle!=o.TargetHandle))return V(false,"wrong owned target HWND");
            var downs=e.Where(x=>x.Kind=="down").ToList();var ups=e.Where(x=>x.Kind=="up").ToList();
            if(c.Spec.EndsWith("move",StringComparison.Ordinal)) {
                bool hit=e.Any(x=>x.Kind=="move"&&x.Area==(c.Spec=="edge_move"?"edge":"target"));
                if(!hit||downs.Count!=0||ups.Count!=0)return V(false,"required move zone missing or unexpected click");
                return c.Spec=="scaled_move"?new BasicVerdict("needs_tool_evidence","native target hit; actual smaller MCP observation dimensions still required"):V(true,"native move in required zone");
            }
            string button=c.Spec=="right_menu"?"right":c.Spec=="middle_click"?"middle":"left";
            if(downs.Count!=1||ups.Count!=1||downs[0].Index>=ups[0].Index)return V(false,"need exactly one native down then up");
            if(downs[0].NativeMessage==0x203||downs[0].NativeMessage==0x206||downs[0].NativeMessage==0x209)return V(false,"unexpected DBLCLK; count 1 required");
            if(downs[0].Button!=button||ups[0].Button!=button||downs[0].Area!="target"||ups[0].Area!="target")return V(false,"wrong button or target (decoy excluded)");
            if(c.Spec=="right_menu"&&!o.ContextOpened)return V(false,"own context menu did not open");
            if(c.Spec=="inactive_click"&&(!o.SecondaryPreparedInactive||!e.Any(x=>x.Kind=="activate"&&!x.ActiveBefore&&x.OwnHandle==o.SecondaryFormHandle&&x.Index<downs[0].Index)))return new BasicVerdict("unknown","no inactive own-window first-activation evidence");
            return V(true,"native "+button+" down/up in target, visible state recorded");
        }
        static BasicVerdict Keyboard(BasicCase c,List<BasicEvent> e,BasicObserved o) {
            var held=new HashSet<int>();var pressed=new List<int>();
            foreach(var x in e) {
                if(x.Kind=="char")continue;
                int vk=CanonicalKey(x.KeyCode);
                if(x.Kind=="key_down") {if(!held.Add(vk))return V(false,"unexpected repeated keydown");pressed.Add(vk);}
                else if(x.Kind=="key_up") {if(!held.Remove(vk))return V(false,"unpaired keyup");}
                else return V(false,"non-keyboard target event");
            }
            if(held.Count!=0)return V(false,"missing keyup; no release proof");
            int[] expected;
            switch(c.Spec) {
                case "plain_key":expected=new[]{65};break;
                case "enter_tab":expected=new[]{13,9};break;
                case "select_all":expected=new[]{17,65};break;
                case "shift_select":expected=new[]{16,37};break;
                case "alt_command":expected=new[]{18,74};break;
                case "multi_modifier":expected=new[]{17,16,74};break;
                case "short_hold":case "long_hold":expected=new[]{16};break;
                case "modifier_release":expected=new[]{17,65,66};break;
                default:return new BasicVerdict("unknown","unknown keyboard spec");
            }
            if(!pressed.SequenceEqual(expected))return V(false,"unexpected key sequence");
            if(c.Spec=="short_hold"||c.Spec=="long_hold") {
                var d=e.First(x=>x.Kind=="key_down");var u=e.First(x=>x.Kind=="key_up");
                uint ms=unchecked(u.NativeTimeMs-d.NativeTimeMs);int duration=c.Spec=="short_hold"?120:900;
                return V(ms>=duration-40&&ms<=duration+600&&o.Text==c.InitialText,"native hold "+ms+"ms; required "+duration+"ms (-40/+600 scheduling tolerance)");
            }
            // Chords require actual simultaneous held modifiers at the principal keydown.
            int main=c.Spec=="shift_select"?37:c.Spec=="alt_command"||c.Spec=="multi_modifier"?74:65;
            int required=c.Spec=="select_all"||c.Spec=="modifier_release"?2:c.Spec=="shift_select"?1:c.Spec=="alt_command"?4:c.Spec=="multi_modifier"?3:0;
            if(required!=0) {
                var principal=e.First(x=>x.Kind=="key_down"&&CanonicalKey(x.KeyCode)==main);
                if(principal.Modifiers!=required)return V(false,"wrong modifiers at principal keydown");
                var downs=e.Where(x=>x.Kind=="key_down"&&CanonicalKey(x.KeyCode)!=main&&CanonicalKey(x.KeyCode)!=66);
                if(downs.Any(d=>!e.Any(u=>u.Kind=="key_up"&&CanonicalKey(u.KeyCode)==CanonicalKey(d.KeyCode)&&u.Index>principal.Index)))return V(false,"modifier released before principal key");
            }
            bool matched=false;
            switch(c.Spec) {
                case "plain_key": matched=o.Text=="a"&&o.SelectionLength==0&&e.Any(x=>x.Kind=="char"&&x.KeyCode==97);break;
                case "enter_tab":matched=o.Text=="\r\n\t"&&e.Any(x=>x.Kind=="char"&&x.KeyCode==13)&&e.Any(x=>x.Kind=="char"&&x.KeyCode==9);break;
                case "select_all":matched=o.Text==c.InitialText&&o.SelectionStart==0&&o.SelectionLength==c.InitialText.Length;break;
                case "shift_select":matched=o.Text==c.InitialText&&o.SelectionStart==10&&o.SelectionLength==1;break;
                case "alt_command":case "multi_modifier":matched=o.Text==c.InitialText&&o.CommandCount==1;break;
                case "modifier_release":
                    var b=e.First(x=>x.Kind=="key_down"&&x.KeyCode==66);
                    matched=o.Text=="b"&&o.SelectionLength==0&&b.Modifiers==0&&e.Any(x=>x.Kind=="key_up"&&CanonicalKey(x.KeyCode)==17&&x.Index<b.Index)
                        &&e.Any(x=>x.Kind=="char"&&x.KeyCode==98);break;
            }
            return V(matched,"native key sequence/release and exact native EDIT text/selection/command state");
        }
    }
}
