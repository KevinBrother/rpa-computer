using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Linq;
using System.Runtime.InteropServices;
using System.Windows.Forms;
namespace AcceptanceFixture {
    sealed partial class GestureForm : Form {
        [DllImport("user32.dll")] private static extern uint GetDoubleClickTime();
        private readonly string suite;
        private readonly GestureCase[] cases;
        private readonly EvidenceLogger logger;
        private readonly SplitMix64 rng;
        private readonly Stopwatch clock=new Stopwatch();
        private readonly List<GestureEvent> events=new List<GestureEvent>();
        private int trial,checkIndex;
        private bool finished;
        private string nonce="";
        private GestureZone[] zones=new GestureZone[0];
        private int av,ah,bv,bh;
        private Label title,nonceLabel,instruction,live,status,offsetA,offsetB;
        private GestureCanvas canvas;
        private GestureSentence sentence;
        private GestureScrollPanel panelA,panelB;
        private Button check,next,close;
        public GestureForm(string suiteName,ulong? seed,string evidencePath) {
            suite=suiteName;cases=GestureCatalog.CasesFor(suite);logger=EvidenceLogger.Create(evidencePath);
            ulong seedUsed=seed??(ulong)DateTime.UtcNow.Ticks;rng=new SplitMix64(seedUsed);
            Text="Computer Use Acceptance";ClientSize=new Size(900,760);FormBorderStyle=FormBorderStyle.FixedSingle;
            MaximizeBox=false;StartPosition=FormStartPosition.Manual;AutoScaleMode=AutoScaleMode.None;
            var screen=Screen.PrimaryScreen.WorkingArea;
            if(screen.Width<Width||screen.Height<Height) throw new InvalidOperationException("primary screen too small for readable fixture");
            Location=new Point(screen.X+(screen.Width-Width)/2,screen.Y+(screen.Height-Height)/2);
            title=LabelAt(20,15,860,35,21);nonceLabel=LabelAt(20,55,860,44,30);
            instruction=LabelAt(20,110,860,75,20);live=LabelAt(20,190,860,34,17);
            canvas=new GestureCanvas {Bounds=new Rectangle(20,235,860,300),DragMode=suite=="drag",Sink=Record};Controls.Add(canvas);
            sentence=new GestureSentence {Bounds=new Rectangle(20,545,860,50),Sink=Record};Controls.Add(sentence);
            panelA=PanelAt("panelA",20);panelB=PanelAt("panelB",455);
            offsetA=LabelAt(20,235,425,34,18);offsetB=LabelAt(455,235,425,34,18);
            status=LabelAt(20,600,860,92,21);
            check=ButtonAt("Check",520,Check);next=ButtonAt("Next",640,Next);close=ButtonAt("Close",760,Close);
            if(logger!=null) logger.Log("session","suite",suite,"platform","windows","seed",seedUsed);
            Shown+=(s,e)=>StartTrial();
            FormClosed+=(s,e)=>{if(trial>0)Log("session_close","completed",finished);};
        }
        private Label LabelAt(int x,int y,int w,int h,int size) {
            var l=new Label {Bounds=new Rectangle(x,y,w,h),Font=new Font("Segoe UI",size,FontStyle.Bold),AutoSize=false};Controls.Add(l);return l;
        }
        private Button ButtonAt(string text,int x,Action action) {
            var b=new Button {Text=text,Bounds=new Rectangle(x,710,110,40),Font=new Font("Segoe UI",18,FontStyle.Bold)};
            b.Click+=(s,e)=>action();Controls.Add(b);return b;
        }
        private GestureScrollPanel PanelAt(string name,int x) {
            var p=new GestureScrollPanel {PanelName=name,Bounds=new Rectangle(x,275,425,260),Sink=Record,OffsetChanged=UpdateOffsets};Controls.Add(p);return p;
        }
        // Some Windows configurations deliver wheel to the focused form. Route the
        // genuine message by its screen coordinates, never fabricate another input.
        protected override void WndProc(ref Message m) {
            if((m.Msg==GestureMessages.Wheel||m.Msg==GestureMessages.HWheel)&&panelA!=null&&suite=="scroll") {
                Point p=PointToClient(new Point(GestureMessages.SignedLow(m.LParam.ToInt64()),GestureMessages.SignedHigh(m.LParam.ToInt64())));
                var panel=panelA.Bounds.Contains(p)?panelA:panelB.Bounds.Contains(p)?panelB:null;
                if(panel!=null) {panel.ReceiveWheel(m);m.Result=IntPtr.Zero;return;}
            }
            base.WndProc(ref m);
        }
        private void Log(string type,params object[] extra) {
            var f=new List<object> {"suite",suite,"case_id",cases[trial-1].Id,"nonce",nonce,"trial",trial,"case_index",trial,"case_total",cases.Length};
            f.AddRange(extra);if(logger!=null)logger.Log(type,f.ToArray());
        }
        private void UpdateOffsets() {
            if(offsetA==null) return;
            offsetA.Text="Panel A row "+panelA.VOffset/36+" ↓"+panelA.VOffset+" →"+panelA.HOffset;
            offsetB.Text="Panel B row "+panelB.VOffset/36+" ↓"+panelB.VOffset+" →"+panelB.HOffset;
        }
        private void StartTrial() {
            if(finished||trial>=cases.Length) return;
            trial++;checkIndex=0;events.Clear();canvas.ResetTrial();sentence.ResetTrial();
            nonce="";const string chars="ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
            for(int i=0;i<6;i++)nonce+=chars[rng.Below(chars.Length)];
            var c=cases[trial-1];zones=GestureLayout.Zones(c);
            int jx=rng.Below(21)-10,jy=rng.Below(21)-10;
            zones=zones.Select(z=>new GestureZone(z.Name,z.X+jx,z.Y+jy,z.W,z.H)).ToArray();canvas.Zones=zones;
            bool text=c.Field("flow")=="text_select"||c.Field("flow")=="select_word"||c.Field("flow")=="select_line";
            canvas.Visible=suite!="scroll"&&!text;sentence.Visible=text;
            sentence.Text=c.Field("flow")=="text_select"?GestureCatalog.DragSelectSentence:GestureCatalog.SelectWordSentence;
            sentence.Select(0,0);
            panelA.Visible=panelB.Visible=offsetA.Visible=offsetB.Visible=suite=="scroll";
            panelA.SetOffset(c.Field("dir")=="left"?300:0,c.Field("dir")=="up"?400:0);panelB.SetOffset(0,0);
            av=panelA.VOffset;ah=panelA.HOffset;bv=panelB.VOffset;bh=panelB.HOffset;UpdateOffsets();
            title.Text=suite.ToUpperInvariant()+" · "+c.Id+" · Trial "+trial+"/"+cases.Length;nonceLabel.Text="NONCE: "+nonce;
            instruction.Text=c.Instruction;
            if(c.Field("flow")=="select_line") instruction.Text+=" On Windows native EDIT, report the visible selection; whole-line selection is not guaranteed. Only the whole sentence satisfies this goal.";
            status.Text="READY — perform the requirement, then Check once";status.ForeColor=Color.Black;live.Text="Waiting for native input";
            next.Text=trial==cases.Length?"Finish":"Next";LayoutTrial();canvas.Invalidate();clock.Restart();
            Log("trial","spec",c.Spec,"instruction",c.Instruction,"platform","windows","started_ms",DateTimeOffset.UtcNow.ToUnixTimeMillisecondsCompat(),
                "platform_expected",c.Field("flow")=="select_line"?"windows: "+GestureJudge.WholeLineRequirement:GesturePlatformNotes.Note(suite,c.Id,"windows"),"native_click_interval_ms",(long)GetDoubleClickTime(),
                "native_click_slop_x",SystemInformation.DoubleClickSize.Width/2.0,"native_click_slop_y",SystemInformation.DoubleClickSize.Height/2.0,
                "a_v_start",av,"a_h_start",ah,"b_v_start",bv,"b_h_start",bh);
        }
        private void Record(GestureEvent e) {
            if(finished||trial==0) return;
            e.TMs=clock.Elapsed.TotalMilliseconds;events.Add(e);
            Log("input_event","event_index",events.Count,"kind",e.Kind,"button",e.Button,"native_count",e.NativeCount,
                "mouse_event_clicks",e.MouseEventClicks,"native_count_source",e.MouseEventClicks>=0?"MouseEventArgs.Clicks":"WM message classification (managed Clicks unavailable)",
                "double_click_msg",e.DoubleClickMsg,"native_message",e.NativeMessage,"native_timestamp_ms",e.NativeTimeMs,"raw_wparam",e.WParam,"raw_lparam",e.LParam,
                "area",e.Area,"x",e.X,"y",e.Y,"raw_dx",e.Dx,"raw_dy",e.Dy,"contract_dx",e.Dx,"contract_dy",-e.Dy,
                "held",e.Held,"t_ms",e.TMs,"source",e.Source,"a_v",panelA.VOffset,"a_h",panelA.HOffset,"b_v",panelB.VOffset,"b_h",panelB.HOffset);
            live.Text="Native presses: "+events.Count(ev=>ev.Kind=="down")+" · events: "+events.Count+" · "+e.Kind+" "+e.Area;
        }
        private GestureObserved Observed() {
            return new GestureObserved {Selection=sentence.SelectedText,AVStart=av,AVEnd=panelA.VOffset,AHStart=ah,AHEnd=panelA.HOffset,
                BVStart=bv,BVEnd=panelB.VOffset,BHStart=bh,BHEnd=panelB.HOffset,AVMax=panelA.VMax,AHMax=panelA.HMax,BVMax=panelB.VMax,BHMax=panelB.HMax};
        }
        private void Check() {
            if(finished||trial==0) return;var c=cases[trial-1];var o=Observed();GestureVerdict v;
            if(suite=="multiclick") v=GestureJudge.Multiclick(c,events,o,GetDoubleClickTime(),SystemInformation.DoubleClickSize.Width/2.0,SystemInformation.DoubleClickSize.Height/2.0);
            else if(suite=="drag") v=GestureJudge.Drag(c,events,o,zones);else v=GestureJudge.Scroll(c,events,o);
            status.Text=(v.Matched?"MATCHED — ":"MISMATCH — ")+v.Reason;status.ForeColor=v.Matched?Color.SeaGreen:Color.Firebrick;
            LayoutTrial();checkIndex++;
            Log("gesture_check","check_index",checkIndex,"check_kind",checkIndex==1?"first":"final","matched",v.Matched,"reason",v.Reason,
                "observed",v.Observed,"event_count",events.Count,"selection",o.Selection,"released",!canvas.ButtonHeld&&!sentence.ButtonHeld,
                "a_v_start",o.AVStart,"a_v_end",o.AVEnd,"a_h_start",o.AHStart,"a_h_end",o.AHEnd,
                "b_v_start",o.BVStart,"b_v_end",o.BVEnd,"b_h_start",o.BHStart,"b_h_end",o.BHEnd,
                "a_v_max",o.AVMax,"a_h_max",o.AHMax,"b_v_max",o.BVMax,"b_h_max",o.BHMax);
        }
        private void Next() {
            if(finished||trial==0)return;Log("trial_end","checks",checkIndex);
            if(trial==cases.Length){finished=true;status.Text="SUITE COMPLETE — "+cases.Length+" cases visited (not a pass count)";Log("suite_complete","visited",trial);next.Enabled=false;LayoutTrial();}
            else StartTrial();
        }
    }
    static class GestureDateCompat {
        // .NET Framework does not expose DateTimeOffset.ToUnixTimeMilliseconds.
        public static long ToUnixTimeMillisecondsCompat(this DateTimeOffset value) {
            return (long)(value.UtcDateTime-new DateTime(1970,1,1,0,0,0,DateTimeKind.Utc)).TotalMilliseconds;
        }
    }
}
