// Own process HWND receivers only. No hooks, injection, global hotkeys or clipboard.
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows.Forms;
namespace AcceptanceFixture {
    static class BasicNative {
        [DllImport("user32.dll")] public static extern int GetMessageTime();
        [DllImport("user32.dll")] public static extern IntPtr GetFocus();
        [DllImport("user32.dll")] public static extern IntPtr GetActiveWindow();
        public static BasicEvent Raw(Message m,string source,string area) {
            IntPtr focus=GetFocus();
            return new BasicEvent {NativeMessage=m.Msg,RawWParam=m.WParam.ToInt64(),RawLParam=m.LParam.ToInt64(),
                NativeTimeMs=unchecked((uint)GetMessageTime()),OwnHandle=m.HWnd.ToInt64(),FocusHandle=focus.ToInt64(),
                Focused=focus==m.HWnd,Source=source,Area=area};
        }
    }
    sealed class BasicPointer : Control {
        public Action<BasicEvent> Sink;
        public string Spec="";
        public bool ContextOpened;
        readonly HashSet<string> held=new HashSet<string>();
        readonly ContextMenuStrip menu=new ContextMenuStrip();
        public bool Released {get{return held.Count==0;}}
        public BasicPointer() {
            SetStyle(ControlStyles.UserPaint|ControlStyles.AllPaintingInWmPaint|ControlStyles.OptimizedDoubleBuffer|ControlStyles.StandardClick|ControlStyles.StandardDoubleClick,true);
            BackColor=Color.White;TabStop=false;
            menu.Font=new Font("Segoe UI",16);menu.AutoClose=false;
            menu.Items.Add("OWN TARGET MENU — no system action");
            menu.Opened+=(s,e)=>{ContextOpened=true;};
        }
        public Rectangle Target {
            get {
                if(Spec=="small_target")return new Rectangle(Math.Max(30,Width/2-28),Math.Max(65,Height/2-12),24,24);
                return new Rectangle(Math.Max(20,Width/2-100),Math.Max(25,Height/2-60),200,120);
            }
        }
        public Rectangle Decoy {get{return new Rectangle(Target.Right+8,Target.Y,24,24);}}
        public string Area(Point p) {
            var r=Target;
            if(r.Contains(p)) {
                if(Spec=="center_move"||Spec=="scaled_move")
                    return Math.Abs(p.X-(r.Left+r.Width/2))<=16&&Math.Abs(p.Y-(r.Top+r.Height/2))<=16?"target":"target_interior";
                return Spec=="edge_move"&&p.X>=r.Right-12?"edge":"target";
            }
            if(Spec=="small_target"&&Decoy.Contains(p))return "decoy";
            return "outside";
        }
        public void Reset(string spec) {Spec=spec;held.Clear();ContextOpened=false;menu.Close();Invalidate();}
        public void CloseMenu() {menu.Close();}
        protected override void Dispose(bool disposing) {if(disposing)menu.Dispose();base.Dispose(disposing);}
        protected override void WndProc(ref Message m) {
            string kind=BasicJudge.MouseKind(m.Msg);
            if(kind.Length==0) {base.WndProc(ref m);return;}
            var e=BasicNative.Raw(m,"WndProc/own-pointer","");
            e.X=unchecked((short)(e.RawLParam&65535));e.Y=unchecked((short)((e.RawLParam>>16)&65535));
            e.Area=Area(new Point(e.X,e.Y));e.Kind=kind;e.Button=BasicJudge.MouseButton(m.Msg);
            if(kind=="down")held.Add(e.Button);if(kind=="up")held.Remove(e.Button);
            if(Sink!=null)Sink(e);
            base.WndProc(ref m);
            if(m.Msg==0x205&&Spec=="right_menu"&&e.Area=="target")menu.Show(this,new Point(e.X,e.Y));
        }
        protected override void OnPaint(PaintEventArgs e) {
            base.OnPaint(e);var r=Target;
            using(var fill=new SolidBrush(Color.FromArgb(200,241,219)))e.Graphics.FillRectangle(fill,r);
            using(var pen=new Pen(Color.SeaGreen,2))e.Graphics.DrawRectangle(pen,r);
            using(var font=new Font("Segoe UI",18,FontStyle.Bold)) {
                if(Spec=="small_target") {
                    e.Graphics.FillRectangle(Brushes.Firebrick,Decoy);
                    e.Graphics.DrawString("GREEN target 24 x 24 / RED decoy",font,Brushes.Black,20,15);
                } else {
                    e.Graphics.DrawString("TARGET",font,Brushes.DarkGreen,r.X+35,r.Y+10);
                    if(Spec=="center_move"||Spec=="scaled_move") {
                        int cx=r.Left+r.Width/2,cy=r.Top+r.Height/2;
                        e.Graphics.DrawEllipse(Pens.DarkGreen,cx-16,cy-16,32,32);
                        e.Graphics.DrawLine(Pens.DarkGreen,cx-20,cy,cx+20,cy);
                        e.Graphics.DrawLine(Pens.DarkGreen,cx,cy-20,cx,cy+20);
                    }
                    if(Spec=="edge_move") {
                        e.Graphics.FillRectangle(Brushes.Goldenrod,r.Right-12,r.Top,12,r.Height);
                        e.Graphics.DrawString("INNER EDGE (12 px)",font,Brushes.Black,20,15);
                    }
                }
            }
        }
    }
    sealed class BasicSecondary : Form {
        public readonly BasicPointer Target=new BasicPointer();
        public Action<BasicEvent> Sink;
        public BasicSecondary() {
            Text="B2 OWN SECOND WINDOW";ClientSize=new Size(350,260);ShowInTaskbar=false;
            FormBorderStyle=FormBorderStyle.FixedToolWindow;StartPosition=FormStartPosition.Manual;
            Target.Dock=DockStyle.Fill;Controls.Add(Target);
        }
        protected override bool ShowWithoutActivation {get{return true;}}
        protected override void WndProc(ref Message m) {
            if(m.Msg==0x21&&Sink!=null) {
                var e=BasicNative.Raw(m,"WndProc/own-Form","secondary");e.Kind="activate";
                e.ActiveBefore=BasicNative.GetActiveWindow()==Handle;Sink(e);
            }
            base.WndProc(ref m);
        }
    }
    sealed class BasicEditor : TextBox {
        public BasicEditor() {
            Multiline=true;AcceptsReturn=true;AcceptsTab=true;ShortcutsEnabled=true;HideSelection=false;
            Font=new Font("Consolas",22);WordWrap=false;
        }
        protected override bool IsInputKey(Keys keyData) {
            Keys k=keyData&Keys.KeyCode;
            if(k==Keys.Tab||k==Keys.Enter||k==Keys.Left||k==Keys.Right)return true;
            return base.IsInputKey(keyData);
        }
    }
    // IMessageFilter sees actual queued WM_KEY/SYSKEY/CHAR before WinForms menu/Tab preprocessing.
    // Exact handles limit observation to two explicitly owned EDIT controls, never another process.
    sealed class BasicKeyReceiver : IMessageFilter {
        readonly BasicEditor editor,decoy;
        readonly HashSet<int> held=new HashSet<int>();
        public Action<BasicEvent> Sink;
        public Action Command;
        public string Spec="";
        public bool Enabled;
        public bool Released {get{return held.Count==0;}}
        public BasicKeyReceiver(BasicEditor target,BasicEditor other) {editor=target;decoy=other;}
        public void Reset(string spec) {Spec=spec;held.Clear();}
        int Modifiers {
            get {return (held.Contains(16)?1:0)|(held.Contains(17)?2:0)|(held.Contains(18)?4:0);}
        }
        public bool PreFilterMessage(ref Message m) {
            if(!Enabled||m.Msg<0x100||m.Msg>0x106||m.Msg==0x103)return false;
            if(!editor.IsHandleCreated||!decoy.IsHandleCreated||(m.HWnd!=editor.Handle&&m.HWnd!=decoy.Handle))return false;
            bool down=m.Msg==0x100||m.Msg==0x104,up=m.Msg==0x101||m.Msg==0x105;
            int vk=BasicJudge.CanonicalKey(m.WParam.ToInt32());bool repeat=down&&held.Contains(vk);
            if(down)held.Add(vk);
            var e=BasicNative.Raw(m,"IMessageFilter/own-EDIT",m.HWnd==editor.Handle?"editor":"decoy");
            e.Kind=down?"key_down":up?"key_up":"char";e.KeyCode=m.WParam.ToInt32();e.Modifiers=Modifiers;
            if(up)held.Remove(vk);
            if(Sink!=null)Sink(e);
            bool alt=Spec=="alt_command",multi=Spec=="multi_modifier";
            if(m.HWnd==editor.Handle&&(alt||multi)) {
                if(down&&vk==74&&e.Modifiers==(alt?4:3)&&!repeat&&Command!=null)Command();
                // Consume only this fixture's safe shortcut messages after recording the real MSG.
                // No SendKeys/SendMessage and no global shortcut registration.
                if(vk==74||(alt&&vk==18)||(!down&&!up))return true;
            }
            return false;
        }
    }
}
