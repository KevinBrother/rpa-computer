// Windows-only B2 contract. Pure models/catalog/export: no control or native calls.
using System;
using System.Collections.Generic;
using System.Text;
namespace AcceptanceFixture {
    sealed class BasicCase {
        public string Suite,Id,Spec,Instruction,InitialText="",PlatformExpected;
        public bool Blocked,Reject;
        public BasicCase(string suite,int n,string spec,string instruction) {
            Suite=suite;Id=suite+"-"+n.ToString("00");Spec=spec;Instruction=instruction;
            PlatformExpected="windows: owned HWND raw WM messages + visible state; tool dispatch alone is not success";
        }
    }
    sealed class BasicEvent {
        public int Index,NativeMessage,KeyCode,Modifiers,X,Y;
        public long RawWParam,RawLParam,OwnHandle,FocusHandle;
        public uint NativeTimeMs;
        public double TMs;
        public string Kind="",Button="none",Area="",Source="";
        public bool Focused,ActiveBefore;
    }
    sealed class BasicObserved {
        public bool Released,ContextOpened,SecondaryPreparedInactive;
        public long TargetHandle,SecondaryFormHandle;
        public string Text="";
        public int SelectionStart,SelectionLength,CommandCount;
    }
    sealed class BasicVerdict {
        public string State,Reason;
        public BasicVerdict(string state,string reason) {State=state;Reason=reason;}
    }
    static class BasicCatalog {
        public static readonly string[] Suites={"pointer","keyboard"};
        public static bool IsBasic(string suite) {return suite=="pointer"||suite=="keyboard";}
        public static BasicCase[] CasesFor(string suite) {
            if(!IsBasic(suite)) throw new ArgumentException("unknown basic suite");
            string[] specs=suite=="pointer"?new[]{"center_move","edge_move","scaled_move","left_click","right_menu","middle_click","inactive_click","small_target","padding_reject","bounds_reject"}
                :new[]{"plain_key","enter_tab","select_all","shift_select","alt_command","multi_modifier","short_hold","long_hold","modifier_release","invalid_key"};
            string[] instructions=suite=="pointer"?new[]{
                "MOVE to the center of the green TARGET. Do not click.",
                "MOVE into the amber INNER EDGE strip of TARGET (inside its right edge). Do not click.",
                "MOVE to TARGET center using a genuinely smaller MCP observation. App cannot prove scaling; external evidence required.",
                "LEFT click TARGET once (count 1).",
                "RIGHT click TARGET once. Own context menu must open. Then click Check; do not choose menu items.",
                "MIDDLE click TARGET once.",
                "LEFT click TARGET in the owned SECOND WINDOW once, without activating it first. Never Alt-Tab.",
                "LEFT click the small GREEN square, not its adjacent RED decoy. Caption is outside the hit area.",
                "BLOCKED / needs_capability: screenshot padding is not exposed by current root API. Do not send any target input. Check records BLOCKED.",
                "Send one MOVE with position [-1,-1]. Expect explicit invalid_action + not_started; do not replace with a legal move. App alone cannot PASS."
            }:new[]{
                "Click EDITOR to focus; key_hold a for 80 ms. Expected native down/up and text a. Do not use text_input.",
                "Click EDITOR; key_hold enter 80 ms, then key_hold tab 80 ms. Both remain inside EDITOR (CRLF + TAB).",
                "Click EDITOR; key_chord modifiers [ctrl], key a. Select all ALPHA BRAVO.",
                "Click EDITOR at end of ALPHA BRAVO; key_chord [shift], left. Select final O.",
                "Click EDITOR; key_chord [alt], j. Only this app's safe ALT+J command lights up; no system shortcut.",
                "Click EDITOR; key_chord [ctrl,shift], j. Own safe MULTI command lights up.",
                "Click EDITOR; key_hold shift for 120 ms. Native down/up timing required; no text is expected.",
                "Click EDITOR; key_hold shift for 900 ms. Native down/up timing required; no text is expected.",
                "Click EDITOR; key_chord [ctrl], a; then key_hold b 80 ms. Expected b, real Ctrl release before b.",
                "Click EDITOR; one key_chord [ctrl], __invalid__. Expect explicit rejection before ANY native key event. Never substitute a valid key."
            };
            var result=new BasicCase[10];
            for(int i=0;i<10;i++) result[i]=new BasicCase(suite,i+1,specs[i],instructions[i]);
            if(suite=="pointer") {
                result[8].Blocked=true;result[8].Reject=true;result[9].Reject=true;
                result[8].PlatformExpected="BLOCKED needs_capability. Future gate requires actual MCP observation_id + declared padding rectangle + image-space point strictly inside that padding + explicit not_started rejection + zero owned target events. Current API has no padding map; this build never upgrades this case.";
                result[2].PlatformExpected="Native center hit AND based_on actual MCP width_px/height_px smaller than a prior observation of the same surface_id/geometry_version. An open max_width request alone is NOT scaling evidence.";
            }
            else {foreach(int n in new[]{2,3,8}) result[n].InitialText="ALPHA BRAVO";result[9].Reject=true;}
            return result;
        }
        public static string ManifestJson() {
            var b=new StringBuilder("{\"schema\":\"windows-basic-v1\",\"platform\":\"windows\",\"legacy_manifest_unchanged\":true,\"suites\":{");
            for(int s=0;s<Suites.Length;s++) {
                if(s>0)b.Append(',');b.Append(GestureExporter.JsonString(Suites[s])).Append(":{\"total\":10,\"cases\":[");
                var cases=CasesFor(Suites[s]);
                for(int i=0;i<cases.Length;i++) {
                    if(i>0)b.Append(',');var c=cases[i];
                    b.Append("{\"id\":").Append(GestureExporter.JsonString(c.Id)).Append(",\"spec\":").Append(GestureExporter.JsonString(c.Spec))
                     .Append(",\"instruction\":").Append(GestureExporter.JsonString(c.Instruction)).Append(",\"platform_expected\":").Append(GestureExporter.JsonString(c.PlatformExpected))
                     .Append(",\"initial_text\":").Append(GestureExporter.JsonString(c.InitialText)).Append(",\"status\":").Append(GestureExporter.JsonString(c.Blocked?"blocked/needs_capability":"implemented/pending_CC"))
                     .Append(",\"valid_input_candidate\":").Append(c.Reject?"false":"true").Append(",\"trials_per_run\":1}");
                }
                b.Append("]}");
            }
            return b.Append("},\"ten_valid_trials_per_action_gate\":\"NOT_COMPLETED\",\"macos\":\"pending\"}\n").ToString();
        }
    }
}
