// Owned process controls only. Raw MSG observation, never global hooks or input synthesis.
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Windows.Forms;
namespace AcceptanceFixture {
    sealed class FocusContext : IMessageFilter {
        [DllImport("user32.dll")] static extern int GetMessageTime();
        [DllImport("user32.dll")] static extern IntPtr GetFocus();
        [DllImport("user32.dll")] static extern IntPtr GetActiveWindow();
        readonly Dictionary<IntPtr,string> roles=new Dictionary<IntPtr,string>();
        readonly HashSet<long> keys=new HashSet<long>();
        readonly HashSet<string> buttons=new HashSet<string>();
        public Action<FocusEvent> Sink;
        public Action<string,long> Binding;
        public bool Ready;
        public bool Released {get{return keys.Count==0&&buttons.Count==0;}}
        public long FocusHandle {get{IntPtr h=GetFocus();return roles.ContainsKey(h)?h.ToInt64():0;}}
        public long ActiveHandle {get{IntPtr h=GetActiveWindow();return roles.ContainsKey(h)?h.ToInt64():0;}}
        public void Track(Control control,string role) {
            IntPtr registered=IntPtr.Zero;
            Action register=()=>{registered=control.Handle;roles[registered]=role;if(Binding!=null)Binding(role,registered.ToInt64());};
            control.HandleCreated+=(s,e)=>register();
            control.HandleDestroyed+=(s,e)=>roles.Remove(registered);
            if(control.IsHandleCreated)register();
        }
        public void PublishBindings() {if(Binding!=null)foreach(var p in roles)Binding(p.Value,p.Key.ToInt64());}
        public void ResetHeld() {keys.Clear();buttons.Clear();}
        public void Receive(Message message,string role,string source) {
            string kind=FocusMessages.Kind(message.Msg);if(!Ready||kind.Length==0||!roles.ContainsKey(message.HWnd))return;
            long wp=message.WParam.ToInt64();
            if(kind=="key_down")keys.Add(wp);if(kind=="key_up")keys.Remove(wp);
            if(kind=="down")buttons.Add("mouse");if(kind=="up")buttons.Remove("mouse");
            if(Sink!=null)Sink(new FocusEvent {Message=message.Msg,Kind=kind,Role=role,Source=source,Handle=message.HWnd.ToInt64(),
                WParam=wp,LParam=message.LParam.ToInt64(),NativeMs=unchecked((uint)GetMessageTime()),FocusHandle=FocusHandle,ActiveHandle=ActiveHandle});
        }
        public bool PreFilterMessage(ref Message m) {
            string role;
            if(roles.TryGetValue(m.HWnd,out role)&&role.EndsWith(".edit",StringComparison.Ordinal)
                &&(m.Msg==0x100||m.Msg==0x101||m.Msg==0x102||m.Msg==0x104||m.Msg==0x105||m.Msg==0x106))Receive(m,role,"queue/own-EDIT");
            return false;
        }
    }
    class FocusWindow : Form {
        protected FocusContext Context;
        public string WindowRole;
        public FocusWindow(FocusContext context,string role) {Context=context;WindowRole=role;context.Track(this,role);}
        protected override void WndProc(ref Message m) {
            if(Context!=null&&(m.Msg<0x100||m.Msg>=0x200))Context.Receive(m,WindowRole,"WndProc/own");
            base.WndProc(ref m);
        }
    }
    sealed class FocusEditor : TextBox {
        readonly FocusContext context;readonly string role;
        public FocusEditor(FocusContext c,string r) {
            context=c;role=r;c.Track(this,r);Multiline=true;AcceptsReturn=true;AcceptsTab=true;HideSelection=false;
            Font=new System.Drawing.Font("Consolas",22);ShortcutsEnabled=true;
        }
        protected override void WndProc(ref Message m) {
            if(context!=null&&(m.Msg<0x100||m.Msg>=0x200))context.Receive(m,role,"WndProc/own");
            base.WndProc(ref m);
        }
    }
    sealed class FocusButton : Button {
        readonly FocusContext context;readonly string role;
        public FocusButton(FocusContext c,string r,string text) {context=c;role=r;Text=text;c.Track(this,r);Font=new System.Drawing.Font("Segoe UI",16);Height=50;}
        protected override void WndProc(ref Message m) {
            if(context!=null&&(m.Msg<0x100||m.Msg>=0x200))context.Receive(m,role,"WndProc/own");
            base.WndProc(ref m);
        }
    }
}
