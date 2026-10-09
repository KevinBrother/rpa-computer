// B2 presentation + trial orchestration, separate from pure judgments and native receivers.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;
namespace AcceptanceFixture {
    sealed class BasicForm : Form {
        readonly BasicCase[] cases;
        readonly EvidenceLogger logger;
        readonly Stopwatch clock=new Stopwatch();
        readonly List<BasicEvent> events=new List<BasicEvent>();
        readonly Label heading=new Label(),nonceLabel=new Label(),instruction=new Label(),status=new Label(),live=new Label();
        readonly Button check=new Button(),next=new Button();
        readonly BasicPointer pointer=new BasicPointer();
        readonly BasicEditor editor=new BasicEditor(),decoy=new BasicEditor();
        readonly BasicKeyReceiver keys;
        readonly Panel stage=new Panel();
        BasicSecondary secondary;
        int trial,checks,commands;
        string nonce="";
        bool finished,ready,secondaryInactive;
        public BasicForm(string suite,ulong? seed,string evidencePath) {
            cases=BasicCatalog.CasesFor(suite);logger=EvidenceLogger.Create(evidencePath);
            Text="Computer Use Acceptance — WINDOWS B2 "+suite.ToUpperInvariant();
            ClientSize=new Size(960,710);MinimumSize=new Size(940,700);StartPosition=FormStartPosition.CenterScreen;
            BackColor=Color.FromArgb(241,245,249);Font=new Font("Segoe UI",16);AutoScaleMode=AutoScaleMode.Dpi;
            var layout=new TableLayoutPanel {Dock=DockStyle.Fill,ColumnCount=1,RowCount=7,Padding=new Padding(18)};
            foreach(int h in new[]{44,36,112})layout.RowStyles.Add(new RowStyle(SizeType.Absolute,h));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent,100));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute,46));layout.RowStyles.Add(new RowStyle(SizeType.Absolute,72));layout.RowStyles.Add(new RowStyle(SizeType.Absolute,60));
            heading.Font=new Font("Segoe UI",23,FontStyle.Bold);nonceLabel.Font=new Font("Consolas",18,FontStyle.Bold);
            foreach(var l in new[]{heading,nonceLabel,instruction,live,status}) {l.Dock=DockStyle.Fill;l.AutoEllipsis=false;}
            status.Font=new Font("Segoe UI",16,FontStyle.Bold);live.Font=new Font("Consolas",12);
            layout.Controls.Add(heading,0,0);layout.Controls.Add(nonceLabel,0,1);layout.Controls.Add(instruction,0,2);
            stage.Dock=DockStyle.Fill;stage.BackColor=Color.White;layout.Controls.Add(stage,0,3);
            layout.Controls.Add(live,0,4);layout.Controls.Add(status,0,5);
            var admin=new FlowLayoutPanel {Dock=DockStyle.Fill,FlowDirection=FlowDirection.LeftToRight};
            check.Text="Check once";next.Text="Next";
            foreach(var b in new[]{check,next}) {b.Width=200;b.Height=50;b.Font=new Font("Segoe UI",18,FontStyle.Bold);admin.Controls.Add(b);}
            var label=new Label {Text="ADMIN — not target input",AutoSize=true,Padding=new Padding(8,10,0,0)};admin.Controls.Add(label);
            layout.Controls.Add(admin,0,6);Controls.Add(layout);
            pointer.Dock=DockStyle.Fill;stage.Controls.Add(pointer);pointer.Sink=Record;
            var editLayout=new TableLayoutPanel {Dock=DockStyle.Fill,RowCount=4,ColumnCount=1,Visible=suite=="keyboard"};
            editLayout.RowStyles.Add(new RowStyle(SizeType.Absolute,32));editLayout.RowStyles.Add(new RowStyle(SizeType.Percent,70));
            editLayout.RowStyles.Add(new RowStyle(SizeType.Absolute,32));editLayout.RowStyles.Add(new RowStyle(SizeType.Percent,30));
            editLayout.Controls.Add(new Label {Text="EDITOR — TARGET (click here to focus)",Dock=DockStyle.Fill},0,0);
            editor.Dock=DockStyle.Fill;editLayout.Controls.Add(editor,0,1);
            editLayout.Controls.Add(new Label {Text="DECOY — do NOT type here",Dock=DockStyle.Fill,ForeColor=Color.Firebrick},0,2);
            decoy.Dock=DockStyle.Fill;decoy.BackColor=Color.MistyRose;editLayout.Controls.Add(decoy,0,3);
            stage.Controls.Add(editLayout);if(suite=="keyboard"){pointer.Visible=false;editLayout.BringToFront();}
            keys=new BasicKeyReceiver(editor,decoy);keys.Sink=Record;
            keys.Command=()=>{commands++;live.Text="OWN SAFE COMMAND lit · count "+commands;};
            Application.AddMessageFilter(keys);
            editor.GotFocus+=(s,e)=>Log("basic_focus","area","editor","own_handle",editor.Handle.ToInt64());
            decoy.GotFocus+=(s,e)=>Log("basic_focus","area","decoy","own_handle",decoy.Handle.ToInt64());
            check.Click+=(s,e)=>Check();next.Click+=(s,e)=>Next();
            Shown+=(s,e)=>{Log("basic_suite_start","seed",seed.HasValue?seed.Value.ToString():"none","planned_cases",10);StartTrial();};
            FormClosed+=(s,e)=>{ready=false;keys.Enabled=false;Application.RemoveMessageFilter(keys);if(secondary!=null)secondary.Close();Log("basic_window_closed","finished",finished);};
        }
        BasicCase Current {get{return cases[Math.Max(0,trial-1)];}}
        void Log(string type,params object[] pairs) {
            if(logger==null)return;
            var all=new List<object>{"suite",Current.Suite,"case_id",trial==0?"":Current.Id,"nonce",nonce,"trial",trial,"platform","windows",
                "utc_ms",DateTimeOffset.UtcNow.ToUnixTimeMillisecondsCompat(),"t_ms",clock.Elapsed.TotalMilliseconds};
            all.AddRange(pairs);logger.Log(type,all.ToArray());
        }
        void StartTrial() {
            ready=false;keys.Enabled=false;
            if(secondary!=null){secondary.Close();secondary=null;}
            trial++;checks=0;commands=0;events.Clear();nonce=Guid.NewGuid().ToString("N").Substring(0,10).ToUpperInvariant();
            var c=Current;pointer.Reset(c.Spec);keys.Reset(c.Spec);secondaryInactive=false;
            editor.Text=c.InitialText;editor.Select(editor.TextLength,0);decoy.Text="";
            heading.Text=c.Id.ToUpperInvariant()+"  ·  "+trial+" / 10  ·  WINDOWS";
            nonceLabel.Text="NONCE  "+nonce;instruction.Text=c.Instruction;
            status.Text=c.Blocked?"BLOCKED — needs_capability; do not send target input":"READY — perform requirement, then Check once";
            status.ForeColor=c.Blocked?Color.DarkGoldenrod:Color.Black;
            live.Text="Raw WM events: 0 · no synthetic input · admin controls excluded";
            check.Enabled=true;next.Enabled=false;next.Text=trial==10?"Finish":"Next";
            if(c.Spec=="inactive_click") {
                secondary=new BasicSecondary();secondary.Text="B2 OWN SECOND · "+c.Id+" · "+nonce;
                secondary.Target.Reset(c.Spec);secondary.Sink=Record;secondary.Target.Sink=Record;
                var screen=Screen.FromControl(this).WorkingArea;
                secondary.Location=new Point(Math.Min(Right-secondary.Width-25,screen.Right-secondary.Width),Top+235);
                secondary.Show(this);
                secondaryInactive=BasicNative.GetActiveWindow()!=secondary.Handle;
            }
            clock.Restart();ready=true;keys.Enabled=c.Suite=="keyboard";
            Log("basic_trial","spec",c.Spec,"instruction",c.Instruction,"initial_text",c.InitialText,"platform_expected",c.PlatformExpected,
                "observation_requirement",c.Spec=="scaled_move"?"actual smaller MCP observation":"current","capability",c.Blocked?"needs_capability":"available",
                "secondary_prepared_inactive",secondaryInactive,"target_handle",(secondary==null?pointer:secondary.Target).Handle.ToInt64(),
                "secondary_form_handle",secondary==null?0L:secondary.Handle.ToInt64(),"expected_hold_ms",c.Spec=="short_hold"?120:c.Spec=="long_hold"?900:0);
            LogGeometry();
        }
        void LogGeometry() {
            BasicPointer target=secondary==null?pointer:secondary.Target;
            Rectangle r=target.RectangleToScreen(target.Target);
            Log("basic_geometry","area","target","screen_x",r.X,"screen_y",r.Y,"width",r.Width,"height",r.Height,
                "note","own app geometry only, NOT MCP observation dimensions; no assumed pixel scaling");
        }
        void Record(BasicEvent e) {
            if(!ready||finished)return;
            if(checks>0) {Log("basic_post_check_input","native_message",e.NativeMessage,"area",e.Area,"source",e.Source);return;}
            e.Index=events.Count+1;e.TMs=clock.Elapsed.TotalMilliseconds;events.Add(e);
            Log("basic_input","event_index",e.Index,"kind",e.Kind,"button",e.Button,"area",e.Area,"source",e.Source,
                "native_message",e.NativeMessage,"native_timestamp_ms",e.NativeTimeMs,"raw_wparam",e.RawWParam,"raw_lparam",e.RawLParam,
                "x",e.X,"y",e.Y,"key_code",e.KeyCode,"modifiers",e.Modifiers,"own_handle",e.OwnHandle,"focus_handle",e.FocusHandle,
                "focused",e.Focused,"active_before",e.ActiveBefore);
            live.Text="WM 0x"+e.NativeMessage.ToString("X4")+" · #"+e.Index+" · "+e.Kind+" · "+e.Area+" · focus="+e.Focused
                +(commands>0?" · OWN SAFE COMMAND "+(Current.Spec=="alt_command"?"ALT+J":"CTRL+SHIFT+J")+" lit: "+commands:"");
        }
        void Check() {
            if(!ready||finished||checks!=0)return;
            Log("basic_admin","operation","check","area","admin");
            var o=new BasicObserved {Released=keys.Released&&pointer.Released&&(secondary==null||secondary.Target.Released),
                TargetHandle=(secondary==null?pointer:secondary.Target).Handle.ToInt64(),SecondaryFormHandle=secondary==null?0L:secondary.Handle.ToInt64(),
                Text=editor.Text,SelectionStart=editor.SelectionStart,SelectionLength=editor.SelectionLength,
                CommandCount=commands,ContextOpened=pointer.ContextOpened,SecondaryPreparedInactive=secondaryInactive};
            var v=BasicJudge.Check(Current,events,o);checks=1;
            status.Text=v.State.ToUpperInvariant()+" — "+v.Reason;
            status.ForeColor=v.State=="matched"?Color.SeaGreen:v.State=="mismatch"?Color.Firebrick:Color.DarkGoldenrod;
            Log("basic_check","check_index",checks,"check_kind","first","state",v.State,"reason",v.Reason,"visible_result",status.Text,
                "event_count",events.Count,"released",o.Released,"text",o.Text,"selection_start",o.SelectionStart,"selection_length",o.SelectionLength,
                "command_count",o.CommandCount,"context_opened",o.ContextOpened,"secondary_prepared_inactive",o.SecondaryPreparedInactive);
            pointer.CloseMenu();check.Enabled=false;next.Enabled=o.Released;
        }
        void Next() {
            if(!ready||finished||checks!=1)return;
            Log("basic_admin","operation",trial==10?"finish":"next","area","admin");
            Log("basic_trial_end","final_check_index",checks);
            if(trial==10) {
                finished=true;ready=false;keys.Enabled=false;next.Enabled=false;
                status.Text="SUITE COMPLETE — 10 cases visited; NOT 10 valid inputs or passes";
                Log("basic_suite_complete","visited",10);return;
            }
            StartTrial();
        }
    }
}
