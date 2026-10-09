// OPTIONAL CC+GLM-only bounded diagnostic, requires coordinator GO. NO input injection.
// Actual GestureForm/GestureSentence behavior; observer never sets selection or emulates EDIT.
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows.Forms;
namespace AcceptanceFixture {
    static class GestureStageANativeProbe {
        [DllImport("user32.dll",CharSet=CharSet.Unicode,EntryPoint="GetClassNameW")]
        private static extern int GetClassName(IntPtr hwnd,StringBuilder buffer,int capacity);
        [DllImport("user32.dll",EntryPoint="GetWindowLongW")]
        private static extern int GetWindowLong(IntPtr hwnd,int index);
        [DllImport("user32.dll",EntryPoint="SendMessageW")]
        private static extern IntPtr GetSelection(IntPtr hwnd,int message,out int start,out int end);
        private static void Snapshot(GestureSentence sentence,string phase,int eventIndex) {
            int start,end;
            GetSelection(sentence.Handle,0x00B0,out start,out end); // EM_GETSEL only; no input/selection mutation.
            string selected=sentence.SelectedText;
            GestureStageARunner.Emit(new {type="native_selection",phase=phase,event_index=eventIndex,
                selection_start=start,selection_end=end,selected_text=selected,
                utf16=BitConverter.ToString(Encoding.Unicode.GetBytes(selected)),text=sentence.Text,
                held=sentence.ButtonHeld,focused=sentence.Focused,
                caution=phase=="queued-after-native"?"queued snapshot may include later messages; NOT per-message postcondition":"pre-native snapshot"});
        }
        internal static void Run() {
            using(var form=new GestureForm("multiclick",1UL,null)) {
                var sentence=GestureStageARunner.Field<GestureSentence>(form,"sentence");
                var original=sentence.Sink;
                int eventIndex=0;
                sentence.Sink=e=> {
                    int index=++eventIndex;
                    GestureStageARunner.Emit(new {type="native_input",event_index=index,kind=e.Kind,button=e.Button,
                        native_message=e.NativeMessage,native_count=e.NativeCount,double_click_msg=e.DoubleClickMsg,
                        native_time=e.NativeTimeMs,x=e.X,y=e.Y,source=e.Source});
                    Snapshot(sentence,"before-native",index);
                    if(original!=null) original(e); // Preserve actual production event recording.
                    sentence.BeginInvoke((Action)(()=> {
                        if(!sentence.IsDisposed) Snapshot(sentence,"queued-after-native",index);
                    }));
                };
                form.Shown+=(s,e)=> {
                    var name=new StringBuilder(128);GetClassName(sentence.Handle,name,name.Capacity);
                    var instruction=GestureStageARunner.Field<Label>(form,"instruction");
                    using(var graphics=sentence.CreateGraphics()) GestureStageARunner.Emit(new {type="native_environment",
                        os=Environment.OSVersion.ToString(),clr=Environment.Version.ToString(),class_name=name.ToString(),
                        window_style=GetWindowLong(sentence.Handle,-16),read_only=sentence.ReadOnly,multiline=sentence.Multiline,
                        hide_selection=sentence.HideSelection,dpi_x=graphics.DpiX,dpi_y=graphics.DpiY,
                        font=sentence.Font.ToString(),instruction_bounds=instruction.Bounds.ToString(),
                        warning="Manual diagnostic only. CC+GLM clicks; no automated selection, no acceptance PASS."});
                    if(!string.Equals(name.ToString(),"EDIT",StringComparison.OrdinalIgnoreCase) &&
                        !name.ToString().StartsWith("WindowsForms10.EDIT.",StringComparison.OrdinalIgnoreCase)) {
                        GestureStageARunner.Emit(new {type="blocked",reason="unexpected native class; no EDIT conclusion permitted"});
                        form.Close();
                    }
                };
                Application.Run(form);
            }
        }
    }
}
