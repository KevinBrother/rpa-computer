// Focus trial UI/evidence. No Check action repairs focus or generates target input.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Windows.Forms;
namespace AcceptanceFixture {
    sealed class FocusForm : FocusWindow {
        readonly EvidenceLogger logger;
        readonly Stopwatch clock=new Stopwatch();
        readonly List<FocusEvent> events=new List<FocusEvent>();
        readonly Label title=new Label(),nonceLabel=new Label(),instruction=new Label(),status=new Label();
        readonly FocusEditor editor;
        readonly FocusButton check,next,openModal,openMenu,restore;
        readonly ContextMenu menu;
        readonly Timer heartbeat=new Timer();
        FocusPeer peer;FocusDialog dialog;
        int trial,checkIndex;
        bool menuActive,closing;
        string nonce="",lastDialogText="";
        FocusCase Current {get{return FocusCatalog.Cases[Math.Max(0,trial-1)];}}
        public FocusForm(string evidencePath):this(new FocusContext(),evidencePath) {}
        FocusForm(FocusContext context,string evidencePath):base(context,"A") {
            logger=EvidenceLogger.Create(evidencePath);Text="Computer Use Acceptance — WINDOWS FOCUS OWN A";
            ClientSize=new Size(1000,720);MinimumSize=new Size(970,710);StartPosition=FormStartPosition.CenterScreen;
            Font=new Font("Segoe UI",16);BackColor=Color.FromArgb(242,246,250);AutoScaleMode=AutoScaleMode.Dpi;
            Context.Sink=Record;Context.Binding=(role,h)=>{if(trial>0&&Context.Ready)Log("focus_binding","role",role,"own_handle",h);};
            var layout=new TableLayoutPanel{Dock=DockStyle.Fill,RowCount=7,ColumnCount=1,Padding=new Padding(18)};
            foreach(int h in new[]{44,38,116})layout.RowStyles.Add(new RowStyle(SizeType.Absolute,h));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent,100));foreach(int h in new[]{50,85,62})layout.RowStyles.Add(new RowStyle(SizeType.Absolute,h));
            title.Font=new Font("Segoe UI",23,FontStyle.Bold);nonceLabel.Font=new Font("Consolas",18,FontStyle.Bold);
            foreach(var l in new[]{title,nonceLabel,instruction,status})l.Dock=DockStyle.Fill;
            status.Font=new Font("Segoe UI",16,FontStyle.Bold);
            layout.Controls.Add(title,0,0);layout.Controls.Add(nonceLabel,0,1);layout.Controls.Add(instruction,0,2);
            var stage=new Panel{Dock=DockStyle.Fill};
            var a=new Panel{Dock=DockStyle.Left,Width=430,Padding=new Padding(6),BackColor=Color.Honeydew};
            editor=new FocusEditor(Context,"A.edit"){Dock=DockStyle.Fill};a.Controls.Add(editor);
            a.Controls.Add(new Label{Text="A EDITOR — owned target",Dock=DockStyle.Top,Height=35,Font=new Font("Segoe UI",18,FontStyle.Bold)});
            stage.Controls.Add(a);stage.Controls.Add(new Label{Text="Owned B appears to the right.\nNo other application is a target.",Left=465,Top=15,Width=450,Height=80});layout.Controls.Add(stage,0,3);
            var business=new FlowLayoutPanel{Dock=DockStyle.Fill};
            openModal=new FocusButton(Context,"A.open_modal","OPEN MODAL"){Width=210};openModal.Click+=(s,e)=>OpenModal();
            openMenu=new FocusButton(Context,"A.open_menu","OPEN OWN MENU"){Width=230};
            restore=new FocusButton(Context,"A.restore","RESTORE B"){Width=200};restore.Click+=(s,e)=>{if(peer!=null&&!peer.IsDisposed)peer.RestoreFromOwnButton();};
            business.Controls.AddRange(new Control[]{openModal,openMenu,restore});layout.Controls.Add(business,0,4);layout.Controls.Add(status,0,5);
            var admin=new FlowLayoutPanel{Dock=DockStyle.Fill};check=new FocusButton(Context,"admin.check","Check once"){Width=190};next=new FocusButton(Context,"admin.next","Next"){Width=170};
            check.Click+=(s,e)=>Check();next.Click+=(s,e)=>Next();admin.Controls.AddRange(new Control[]{check,next,new Label{Text="ADMIN — not target input",AutoSize=true,Padding=new Padding(8,10,0,0)}});
            layout.Controls.Add(admin,0,6);Controls.Add(layout);
            // Real native ContextMenu/TrackPopupMenu owner loop; semantic Popup is not used as native proof.
            menu=new ContextMenu(new[]{new MenuItem("Dismiss OWN menu",(s,e)=>{})});
            openMenu.Click+=(s,e)=>menu.Show(this,new Point(450,320));
            Application.AddMessageFilter(Context);
            heartbeat.Interval=400;heartbeat.Tick+=(s,e)=>{if(trial==10&&!closing)Log("focus_heartbeat","event_count",events.Count,"text_a",editor.Text,"text_b",PeerText(),"text_d",DialogText(),"focus_handle",Context.FocusHandle);};
            Shown+=(s,e)=>StartTrial();
            FormClosed+=(s,e)=>{closing=true;Context.Ready=false;heartbeat.Stop();heartbeat.Dispose();Application.RemoveMessageFilter(Context);if(peer!=null)peer.Close();if(dialog!=null)dialog.Close();menu.Dispose();Log("focus_window_closed","checks",checkIndex);};
        }
        void Log(string type,params object[] pairs) {
            if(logger==null)return;var data=new List<object>{"suite","focus","case_id",trial==0?"":Current.Id,"nonce",nonce,"trial",trial,"platform","windows",
                "utc_ms",DateTimeOffset.UtcNow.ToUnixTimeMillisecondsCompat(),"t_ms",clock.Elapsed.TotalMilliseconds};data.AddRange(pairs);logger.Log(type,data.ToArray());
        }
        void StartTrial() {
            Context.Ready=false;if(peer!=null)peer.Close();trial++;checkIndex=0;events.Clear();Context.ResetHeld();lastDialogText="";menuActive=false;
            nonce=Guid.NewGuid().ToString("N").Substring(0,10).ToUpperInvariant();var c=Current;
            editor.Text=FocusCatalog.InitialA(c);editor.Select(editor.TextLength,0);editor.Enabled=trial!=10;
            title.Text=c.Id.ToUpperInvariant()+" · "+trial+"/10 · WINDOWS OWN A/B";nonceLabel.Text="NONCE "+nonce;instruction.Text=c.Instruction;
            status.Text=trial==10?"FOCUS UNCONFIRMED — NO TEXT/KEY INPUT; observe, Check, safe Close":"READY — first attempt only; check once";
            status.ForeColor=trial==10?Color.DarkGoldenrod:Color.Black;check.Enabled=true;next.Enabled=false;
            openModal.Enabled=trial==5||trial==6;openMenu.Enabled=trial==7;restore.Enabled=trial==8;next.Text=trial==10?"Finish via safe Close":"Next";
            peer=new FocusPeer(Context);peer.Prepare(c,nonce);
            var bounds=Screen.FromControl(this).WorkingArea;peer.Location=new Point(Math.Min(Right-peer.Width-30,bounds.Right-peer.Width),Top+230);peer.Show(this);
            clock.Restart();Log("focus_trial","spec",c.Spec,"instruction",c.Instruction,"payload",c.Payload,"target_role",c.Target,
                "focus_policy",trial==10?"unconfirmed_no_input":"owned_only","platform_expected","own WM + readback + tool association; all user/protected apps remain UNKNOWN",
                "initial_a",FocusCatalog.InitialA(c));Context.Ready=true;Context.PublishBindings();
            if(trial==10)heartbeat.Start();
        }
        void Record(FocusEvent e) {
            if(closing||trial==0)return;e.Index=events.Count+1;e.TMs=clock.Elapsed.TotalMilliseconds;events.Add(e);
            if(e.Role=="A"&&e.Kind=="menu_enter")menuActive=true;if(e.Role=="A"&&e.Kind=="menu_exit")menuActive=false;
            Log("focus_native","event_index",e.Index,"native_message",e.Message,"kind",e.Kind,"role",e.Role,"source",e.Source,
                "own_handle",e.Handle,"raw_wparam",e.WParam,"raw_lparam",e.LParam,"native_timestamp_ms",e.NativeMs,"focus_handle",e.FocusHandle,
                "active_handle",e.ActiveHandle,"phase",checkIndex==0?"before_check":"after_check");
        }
        string PeerText() {return peer!=null&&!peer.IsDisposed?peer.Editor.Text:"[owned B unavailable]";}
        string DialogText() {return dialog!=null&&!dialog.IsDisposed?dialog.Editor.Text:lastDialogText;}
        void OpenModal() {
            if(trial!=5&&trial!=6||checkIndex!=0)return;
            using(var d=new FocusDialog(Context,Current.Id,nonce,Check)) {
                dialog=d;d.CheckButton.Enabled=trial==5;
                d.FormClosing+=(s,e)=>{lastDialogText=d.Editor.Text;};
                d.ShowDialog(this);dialog=null;
            }
        }
        void Check() {
            if(trial==0||checkIndex!=0)return;
            var o=new FocusObserved {TextA=editor.Text,TextB=PeerText(),TextD=DialogText(),SelectionAStart=editor.SelectionStart,SelectionALength=editor.SelectionLength,
                Released=Context.Released,ParentEnabled=Enabled,ModalOpen=dialog!=null&&!dialog.IsDisposed&&dialog.Visible,
                BMinimized=peer==null||peer.IsDisposed||peer.WindowState==FormWindowState.Minimized,MenuActive=menuActive};
            var v=FocusJudge.Check(Current,events,o);checkIndex=1;status.Text=v.State.ToUpperInvariant()+" — "+v.Reason;
            status.ForeColor=v.State=="matched"?Color.SeaGreen:v.State=="mismatch"?Color.Firebrick:Color.DarkGoldenrod;
            if(dialog!=null){dialog.Result.Text=status.Text;dialog.CheckButton.Enabled=false;}
            Log("focus_check","check_index",1,"check_kind","first","state",v.State,"visible_result",status.Text,"event_count",events.Count,
                "released",o.Released,"text_a",o.TextA,"text_b",o.TextB,"text_d",o.TextD,"selection_a_start",o.SelectionAStart,"selection_a_length",o.SelectionALength,
                "modal_open",o.ModalOpen,"parent_enabled",o.ParentEnabled,"b_minimized",o.BMinimized,"menu_active",o.MenuActive,"global_protected_apps","unknown");
            check.Enabled=false;next.Enabled=o.Released&&trial<10;
            if(trial==10){Log("focus_trial_end","final_check_index",1);Log("focus_suite_complete","visited",10,"safe_stop","pending_tool_trace");}
        }
        void Next() {if(checkIndex!=1||trial>=10)return;Log("focus_trial_end","final_check_index",1);StartTrial();}
    }
}
