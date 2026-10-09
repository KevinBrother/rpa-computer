// Geometry-only owned HWND observation. No injection, hooks, DPI/settings changes.
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows.Forms;
namespace AcceptanceFixture {
    sealed class GeometrySnapshot {
        public int Dpi,ModeWidth,ModeHeight,OriginX,OriginY;
        public string Device="";
        public bool PhysicalCoordinates,DpiReliable;
    }
    static class GeometryNative {
        [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
        [DllImport("user32.dll")] static extern IntPtr GetWindowDpiAwarenessContext(IntPtr hwnd);
        [DllImport("user32.dll")] static extern bool AreDpiAwarenessContextsEqual(IntPtr a,IntPtr b);
        [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern int GetMessageTime();
        [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd,ref POINT p);
        [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool EnumDisplaySettings(string device,int mode,ref DEVMODE data);
        [StructLayout(LayoutKind.Sequential)] struct POINT {public int X,Y;}
        [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct DEVMODE {
            [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string Device;
            public short Spec,Driver,Size,Extra;public uint Fields;
            public int X,Y;public uint Orientation,FixedOutput;
            public short Color,Duplex,YResolution,TT,Collate;
            [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string Form;
            public short LogPixels;public uint Bits,Width,Height,Flags,Frequency,ICMMethod,ICMIntent,Media,Dither,Reserved1,Reserved2,PanningWidth,PanningHeight;
        }
        public static void EnableOwnThreadDpi() {
            // Thread awareness affects only this new geometry UI, never desktop DPI/settings.
            try {SetThreadDpiAwarenessContext(new IntPtr(-4));}catch(EntryPointNotFoundException){}catch(DllNotFoundException){}
        }
        public static GeometrySnapshot Snapshot(Control owner) {
            var s=new GeometrySnapshot();
            try {s.PhysicalCoordinates=AreDpiAwarenessContextsEqual(GetWindowDpiAwarenessContext(owner.Handle),new IntPtr(-4));
                s.Dpi=(int)GetDpiForWindow(owner.Handle);s.DpiReliable=s.Dpi>0&&s.PhysicalCoordinates;
            }catch(EntryPointNotFoundException){}catch(DllNotFoundException){}
            s.Device=Screen.FromHandle(owner.Handle).DeviceName;
            var m=new DEVMODE();m.Size=(short)Marshal.SizeOf(typeof(DEVMODE));
            if(EnumDisplaySettings(s.Device,-1,ref m)) {s.ModeWidth=(int)m.Width;s.ModeHeight=(int)m.Height;s.OriginX=m.X;s.OriginY=m.Y;}
            return s;
        }
        public static Point ScreenPoint(IntPtr hwnd,int x,int y) {var p=new POINT{X=x,Y=y};if(!ClientToScreen(hwnd,ref p))throw new InvalidOperationException("own ClientToScreen failed");return new Point(p.X,p.Y);}
        public static int Signed(long n) {return (short)(n&65535);}
        public static string InputKind(int m) {
            if(m==0x200)return "move";
            if(m==0x201||m==0x203||m==0x204||m==0x206||m==0x207||m==0x209)return "down";
            if(m==0x202||m==0x205||m==0x208)return "up";
            if(m==0x100||m==0x104)return "key_down";
            if(m==0x101||m==0x105)return "key_up";
            if(m==0x102||m==0x106)return "char";
            if(m==0x20a||m==0x20e)return "wheel";
            return "";
        }
    }
    sealed class GeometryInput : IMessageFilter,IDisposable {
        readonly Dictionary<IntPtr,Control> owned=new Dictionary<IntPtr,Control>();
        public Action<Control,Message> Sink;
        public void Track(Control c) {
            IntPtr h=IntPtr.Zero;c.HandleCreated+=(s,e)=>{h=c.Handle;owned[h]=c;};
            c.HandleDestroyed+=(s,e)=>owned.Remove(h);
            if(c.IsHandleCreated){h=c.Handle;owned[h]=c;}
            foreach(Control child in c.Controls)Track(child);
        }
        public bool PreFilterMessage(ref Message m) {Control c;if(owned.TryGetValue(m.HWnd,out c)&&GeometryNative.InputKind(m.Msg)!=""&&Sink!=null)Sink(c,m);return false;}
        public void Dispose(){Application.RemoveMessageFilter(this);owned.Clear();}
    }
    sealed class GeometryTarget : Panel {
        public GeometryTarget(){Name="target";DoubleBuffered=true;BackColor=Color.FromArgb(18,116,82);ForeColor=Color.White;Size=new Size(180,100);}
        protected override void OnPaint(PaintEventArgs e){base.OnPaint(e);using(var f=new Font("Segoe UI",18,FontStyle.Bold))TextRenderer.DrawText(e.Graphics,"TARGET\nClick once",f,ClientRectangle,ForeColor,TextFormatFlags.HorizontalCenter|TextFormatFlags.VerticalCenter);}
    }
}
