// Real owned-control WndProc receivers; no hooks, global capture or SendInput.
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;
using System.Windows.Forms;
namespace AcceptanceFixture {
    static class GestureMessages {
        [DllImport("user32.dll")] public static extern int GetMessageTime();
        public const int Move=0x0200, LDown=0x0201, LUp=0x0202, LDbl=0x0203,
            RDown=0x0204, RUp=0x0205, RDbl=0x0206, Wheel=0x020A, HWheel=0x020E;
        public static bool IsPointer(int m) {return m>=Move&&m<=RDbl;}
        public static bool IsDown(int m) {return m==LDown||m==RDown||m==LDbl||m==RDbl;}
        public static bool IsUp(int m) {return m==LUp||m==RUp;}
        public static int SignedLow(long n) {return unchecked((short)(n&0xffff));}
        public static int SignedHigh(long n) {return unchecked((short)((n>>16)&0xffff));}
        public static GestureEvent Pointer(Message m, string area, bool held, int clicks) {
            bool dbl=m.Msg==LDbl||m.Msg==RDbl;
            return new GestureEvent {Kind=IsDown(m.Msg)?"down":IsUp(m.Msg)?"up":"move",
                Button=m.Msg==RDown||m.Msg==RUp||m.Msg==RDbl?"right":"left",
                // Count is message classification if managed Clicks wasn't exposed; never manufacture 3.
                NativeCount=clicks>=0?clicks:dbl?2:1, MouseEventClicks=clicks, DoubleClickMsg=dbl,
                NativeMessage=m.Msg, NativeTimeMs=GetMessageTime(), WParam=m.WParam.ToInt64(),LParam=m.LParam.ToInt64(),
                X=SignedLow(m.LParam.ToInt64()),Y=SignedHigh(m.LParam.ToInt64()),Area=area,Held=held};
        }
    }
    sealed class GestureCanvas : Control {
        public GestureZone[] Zones=new GestureZone[0];
        public Action<GestureEvent> Sink;
        public bool DragMode, ButtonHeld;
        private int managedClicks=-1;
        private readonly List<PointF> trail=new List<PointF>();
        public GestureCanvas() {SetStyle(ControlStyles.UserPaint|ControlStyles.AllPaintingInWmPaint|ControlStyles.OptimizedDoubleBuffer|ControlStyles.StandardClick|ControlStyles.StandardDoubleClick,true);BackColor=Color.White;}
        protected override void OnMouseDown(MouseEventArgs e) {managedClicks=e.Clicks;base.OnMouseDown(e);}
        protected override void OnMouseUp(MouseEventArgs e) {managedClicks=e.Clicks;base.OnMouseUp(e);}
        public void ResetTrial() {ButtonHeld=false;Capture=false;trail.Clear();Invalidate();}
        protected override void WndProc(ref Message m) {
            int msg=m.Msg;
            if(!GestureMessages.IsPointer(msg)) {base.WndProc(ref m);return;}
            bool wasHeld=ButtonHeld;
            if(GestureMessages.IsDown(msg)&&DragMode&&msg!=GestureMessages.RDown&&msg!=GestureMessages.RDbl) {ButtonHeld=true;Capture=true;}
            if(GestureMessages.IsUp(msg)) ButtonHeld=false;
            bool record=GestureMessages.IsDown(msg)||GestureMessages.IsUp(msg)||(DragMode&&wasHeld);
            long lp=m.LParam.ToInt64();float x=GestureMessages.SignedLow(lp),y=GestureMessages.SignedHigh(lp);
            string area=DragMode?"canvas":"outside";
            if(!DragMode) foreach(var z in Zones) if(x>=z.X&&x<=z.X+z.W&&y>=z.Y&&y<=z.Y+z.H) {area=z.Name=="TARGET"?"target":z.Name;break;}
            managedClicks=-1; Message raw=m; base.WndProc(ref m);
            if(record && Sink!=null) {
                var e=GestureMessages.Pointer(raw,area,msg==GestureMessages.Move&&wasHeld,managedClicks);
                if(msg==GestureMessages.Move) e.Button=(raw.WParam.ToInt64()&2)!=0?"right":"left";
                // Held is raw MK_LBUTTON / MK_RBUTTON on motion, not an invented sample.
                if(msg==GestureMessages.Move) e.Held=(raw.WParam.ToInt64()&3)!=0;
                Sink(e);trail.Add(new PointF(x,y));Invalidate();
            }
            if(GestureMessages.IsUp(msg)) Capture=false;
        }
        protected override void OnPaint(PaintEventArgs e) {
            base.OnPaint(e);e.Graphics.SetClip(ClientRectangle);e.Graphics.SmoothingMode=SmoothingMode.AntiAlias;
            using(var font=new Font("Segoe UI",22,FontStyle.Bold)) using(var ink=new SolidBrush(Color.FromArgb(18,60,45)))
            using(var fill=new SolidBrush(Color.FromArgb(211,244,223))) using(var pen=new Pen(Color.SeaGreen,3)) {
                foreach(var z in Zones) {
                    var r=new RectangleF((float)z.X,(float)z.Y,(float)z.W,(float)z.H);
                    e.Graphics.FillRectangle(fill,r);e.Graphics.DrawRectangle(pen,r.X,r.Y,r.Width,r.Height);
                    using(var format=new StringFormat {Alignment=StringAlignment.Center,LineAlignment=StringAlignment.Center}) e.Graphics.DrawString(z.Name,font,ink,r,format);
                }
            }
            using(var pen=new Pen(Color.RoyalBlue,3)) if(trail.Count>1&&DragMode) e.Graphics.DrawLines(pen,trail.ToArray());
            using(var brush=new SolidBrush(Color.RoyalBlue)) foreach(var p in trail) e.Graphics.FillEllipse(brush,p.X-4,p.Y-4,8,8);
        }
    }
    sealed class GestureSentence : TextBox {
        public Action<GestureEvent> Sink;
        private bool held;
        public bool ButtonHeld {get{return held;}}
        public GestureSentence() {ReadOnly=true;Multiline=false;Font=new Font("Segoe UI",24);}
        public void ResetTrial() {held=false;SelectionStart=0;SelectionLength=0;}
        protected override void WndProc(ref Message m) {
            bool pointer=GestureMessages.IsPointer(m.Msg);
            bool record=pointer&&(GestureMessages.IsDown(m.Msg)||GestureMessages.IsUp(m.Msg)||(m.Msg==GestureMessages.Move&&held));
            if(GestureMessages.IsDown(m.Msg)) held=true;
            if(GestureMessages.IsUp(m.Msg)) held=false;
            if(record&&Sink!=null) {
                // Record BEFORE native EDIT selection processing (which may reenter its own WndProc).
                // Actual managed MouseEventArgs.Clicks is supplemental, not the source of press count.
                var point=new Point(GestureMessages.SignedLow(m.LParam.ToInt64()),GestureMessages.SignedHigh(m.LParam.ToInt64()));
                var raw=GestureMessages.Pointer(m,ClientRectangle.Contains(point)?"text":"outside",m.Msg==GestureMessages.Move&&(m.WParam.ToInt64()&1)!=0,-1);
                raw.Source="WndProc/own-EDIT";Sink(raw);
            }
            base.WndProc(ref m);
        }
    }
    sealed class GestureScrollPanel : Control {
        public string PanelName="panelA";
        public int VOffset,HOffset;
        public int VMax {get{return Math.Max(0,1440-ClientSize.Height);}}
        public int HMax {get{return Math.Max(0,1200-ClientSize.Width);}}
        public Action<GestureEvent> Sink;
        public Action OffsetChanged;
        public GestureScrollPanel() {SetStyle(ControlStyles.UserPaint|ControlStyles.AllPaintingInWmPaint|ControlStyles.OptimizedDoubleBuffer,true);BackColor=Color.White;TabStop=true;}
        protected override void OnMouseEnter(EventArgs e) {base.OnMouseEnter(e);Focus();}
        protected override void WndProc(ref Message m) {
            if(m.Msg==GestureMessages.Wheel||m.Msg==GestureMessages.HWheel) {ReceiveWheel(m);m.Result=IntPtr.Zero;return;}
            base.WndProc(ref m);
        }
        public void ReceiveWheel(Message m) {
            int delta=GestureMessages.SignedHigh(m.WParam.ToInt64());bool horizontal=m.Msg==GestureMessages.HWheel;
            // WM_MOUSEHWHEEL positive is right; vertical positive is up. 120 units = 90px here.
            int pixels=(int)Math.Round(delta*90.0/120.0);
            if(horizontal) HOffset=Math.Max(0,Math.Min(HMax,HOffset+pixels));
            else VOffset=Math.Max(0,Math.Min(VMax,VOffset-pixels));
            var screen=new Point(GestureMessages.SignedLow(m.LParam.ToInt64()),GestureMessages.SignedHigh(m.LParam.ToInt64()));
            Point p=PointToClient(screen);
            if(Sink!=null) Sink(new GestureEvent {Kind="wheel",Button="other",Area=PanelName,X=p.X,Y=p.Y,
                Dx=horizontal?delta:0,Dy=horizontal?0:delta,NativeMessage=m.Msg,NativeTimeMs=GestureMessages.GetMessageTime(),WParam=m.WParam.ToInt64(),LParam=m.LParam.ToInt64()});
            Invalidate();if(OffsetChanged!=null) OffsetChanged();
        }
        public void SetOffset(int h,int v) {HOffset=Math.Max(0,Math.Min(HMax,h));VOffset=Math.Max(0,Math.Min(VMax,v));Invalidate();}
        protected override void OnPaint(PaintEventArgs e) {
            base.OnPaint(e);e.Graphics.SetClip(ClientRectangle);
            using(var font=new Font("Consolas",16,FontStyle.Bold)) using(var brush=new SolidBrush(Color.DarkSlateGray)) using(var pen=new Pen(Color.LightGray)) {
                for(int row=0;row<40;row++) for(int col=0;col<4;col++) {
                    int x=col*300-HOffset,y=row*36-VOffset;
                    e.Graphics.DrawString(PanelName+" R"+row+" C"+col,font,brush,x+10,y+5);
                    e.Graphics.DrawLine(pen,x,y+35,x+300,y+35);
                }
                e.Graphics.DrawRectangle(pen,0,0,Width-1,Height-1);
            }
        }
    }
}
