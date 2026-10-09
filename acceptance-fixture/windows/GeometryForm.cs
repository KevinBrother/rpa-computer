// Owned Windows fixture. Local status NEVER certifies the MCP/GLM/PNG outcome.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Windows.Forms;
namespace AcceptanceFixture {
    sealed class GeometryForm : Form {
        readonly EvidenceLogger logger;
        readonly Stopwatch clock=Stopwatch.StartNew();
        readonly GeometryInput input=new GeometryInput();
        readonly Label heading=new Label(),identity=new Label(),instructions=new Label(),environment=new Label(),status=new Label();
        readonly Panel stage=new Panel();
        readonly GeometryTarget target=new GeometryTarget();
        readonly Button check=new Button(),next=new Button(),remove=new Button();
        readonly Timer heartbeat=new Timer();
        GeometrySnapshot before,last;
        int index=-1,generation,checkIndex;long eventIndex,downIndex;Point downPoint;
        bool hit,removed,environmentChanged;
        double removedAt,changedAt,lastAutomaticCheck;string nonce="";
        GeometryCase Current {get{return GeometryCatalog.Cases[index];}}
        public GeometryForm(string evidencePath) {
            logger=EvidenceLogger.Create(evidencePath);Text="Owned Windows Geometry — diagnostic only";Name="window";
            Font=new Font("Segoe UI",14);BackColor=Color.FromArgb(244,247,250);ClientSize=new Size(1000,740);MinimumSize=new Size(780,640);StartPosition=FormStartPosition.CenterScreen;
            var grid=new TableLayoutPanel{Dock=DockStyle.Fill,ColumnCount=1,RowCount=7,Padding=new Padding(16),Name="layout"};
            foreach(int h in new[]{42,40,140,66})grid.RowStyles.Add(new RowStyle(SizeType.Absolute,h));
            grid.RowStyles.Add(new RowStyle(SizeType.Percent,100));grid.RowStyles.Add(new RowStyle(SizeType.Absolute,72));grid.RowStyles.Add(new RowStyle(SizeType.Absolute,64));
            heading.Font=new Font("Segoe UI",22,FontStyle.Bold);identity.Font=new Font("Consolas",15,FontStyle.Bold);
            foreach(Label label in new[]{heading,identity,instructions,environment,status}){label.Dock=DockStyle.Fill;label.AutoSize=false;}
            stage.Dock=DockStyle.Fill;stage.Name="stage";stage.BackColor=Color.White;stage.BorderStyle=BorderStyle.FixedSingle;stage.Controls.Add(target);
            var actions=new FlowLayoutPanel{Dock=DockStyle.Fill,Name="actions"};
            Setup(check,"check","Check local evidence");Setup(next,"next","Next case");Setup(remove,"remove","REMOVE TARGET");
            actions.Controls.Add(check);actions.Controls.Add(next);actions.Controls.Add(remove);
            grid.Controls.Add(heading,0,0);grid.Controls.Add(identity,0,1);grid.Controls.Add(instructions,0,2);grid.Controls.Add(environment,0,3);grid.Controls.Add(stage,0,4);grid.Controls.Add(status,0,5);grid.Controls.Add(actions,0,6);Controls.Add(grid);
            check.Click+=(s,e)=>Check("visible_check");next.Click+=(s,e)=>BeginCase();remove.Click+=(s,e)=>RemoveTarget();
            stage.Resize+=(s,e)=>{target.Location=new Point(Math.Max(0,(stage.Width-target.Width)/2),Math.Max(0,(stage.Height-target.Height)/2));ChangedLayout("layout");};
            Move+=(s,e)=>ChangedLayout("window_move");input.Track(this);input.Sink=OnInput;Application.AddMessageFilter(input);
            Shown+=(s,e)=>{BeginCase();heartbeat.Start();};heartbeat.Interval=500;
            heartbeat.Tick+=(s,e)=>{Emit("heartbeat","target_present",!removed,"hit",hit);if(clock.Elapsed.TotalMilliseconds-lastAutomaticCheck>=2000&&((removed&&clock.Elapsed.TotalMilliseconds-removedAt>=2000)||(environmentChanged&&clock.Elapsed.TotalMilliseconds-changedAt>=2000))){lastAutomaticCheck=clock.Elapsed.TotalMilliseconds;Check(removed?"automatic_after_removal":"automatic_after_environment_change");}};
            FormClosed+=(s,e)=>{Emit("fixture_closed");heartbeat.Stop();heartbeat.Dispose();input.Dispose();};
        }
        static void Setup(Button b,string name,string text){b.Name=name;b.Text=text;b.AutoSize=true;b.Height=48;b.Margin=new Padding(4);}
        void BeginCase() {
            if(index>=0)Emit("trial_end","check_count",checkIndex);
            if(index==9){status.Text="Suite finished. GUI verification remains pending CC.";next.Enabled=false;return;}
            index++;nonce=Guid.NewGuid().ToString("N").Substring(0,12);generation++;checkIndex=0;hit=false;removed=false;lastAutomaticCheck=0;environmentChanged=false;downIndex=0;
            target.Visible=true;before=last=GeometryNative.Snapshot(this);
            heading.Text=Current.Id+"  •  "+Current.Spec;identity.Text="nonce "+nonce+"   trial "+(index+1)+"   Windows owned HWND";
            instructions.Text=Current.Instruction;environment.Text=EnvironmentText(last);
            check.Enabled=true;remove.Enabled=index==8;status.Text=Current.Mode=="pure_mapping"?"NOT GUI — supervisor skips this case. Pure self-test is separate.":"Awaiting evidence — no GUI pass is inferred.";
            Emit("trial_start","spec",Current.Spec,"mode",Current.Mode,"target_present",true);
        }
        string EnvironmentText(GeometrySnapshot s){return "HWND DPI "+s.Dpi+" | physical coordinates "+s.PhysicalCoordinates+" | display "+s.Device+"\nmode "+s.ModeWidth+"×"+s.ModeHeight+" origin ("+s.OriginX+","+s.OriginY+") | generation "+generation;}
        void ChangedLayout(string reason){if(index<0)return;generation++;Emit("layout_changed","reason",reason);}
        void RemoveTarget(){if(index!=8||removed)return;removed=true;target.Visible=false;generation++;removedAt=clock.Elapsed.TotalMilliseconds;remove.Enabled=false;check.Enabled=false;status.Text="TARGET REMOVED. Observe disappearance, STOP, Close. Do NOT click Check or Next.";Emit("target_removed","target_present",false);}
        void Check(string trigger){if(index<0)return;checkIndex++;var s=GeometryNative.Snapshot(this);string state=GeometryJudge.Local(Current,hit,s.Dpi,s.DpiReliable,environmentChanged);status.Text="LOCAL "+state+" | "+(hit?"owned DOWN/UP received":"no certified target click")+" | supervisor tool/PNG review required";Emit("check","check_index",checkIndex,"trigger",trigger,"local_state",state,"hit",hit,"target_present",!removed);}
        void OnInput(Control c,Message m) {
            if(index<0)return;eventIndex++;int x=GeometryNative.Signed(m.LParam.ToInt64()),y=GeometryNative.Signed(m.LParam.ToInt64()>>16);
            string kind=GeometryNative.InputKind(m.Msg);
            Point p;string positionBasis;
            if(kind=="wheel"){p=new Point(x,y);var client=c.PointToClient(p);x=client.X;y=client.Y;positionBasis="lparam-screen";}
            else if(kind=="move"||kind=="down"||kind=="up"){p=GeometryNative.ScreenPoint(c.Handle,x,y);positionBasis="lparam-client-to-screen";}
            else{p=Point.Empty;x=y=0;positionBasis="not-positional";}
            Emit("input_event","event_index",eventIndex,"role",c.Name,"own_handle",m.HWnd.ToInt64(),"source","queue/own-HWND","kind",kind,
                "native_message",m.Msg,"raw_wparam",m.WParam.ToInt64(),"raw_lparam",m.LParam.ToInt64(),"native_timestamp_ms",unchecked((uint)GeometryNative.GetMessageTime()),"position_basis",positionBasis,"client_x",x,"client_y",y,"screen_x",p.X,"screen_y",p.Y);
            if(c!=target||removed)return;
            if(m.Msg==0x201&&(m.WParam.ToInt64()&1)!=0){downIndex=eventIndex;downPoint=p;}
            if(m.Msg==0x202){if((m.WParam.ToInt64()&1)==0&&GeometryJudge.ClickPair(0x201,m.Msg,downIndex,eventIndex,downPoint.X,downPoint.Y,p.X,p.Y)&&target.ClientRectangle.Contains(x,y))hit=true;downIndex=0;}
        }
        protected override void WndProc(ref Message m) {
            int msg=m.Msg;long wp=m.WParam.ToInt64(),lp=m.LParam.ToInt64();base.WndProc(ref m);
            if(index<0||(msg!=0x7e&&msg!=0x2e0))return;
            var now=GeometryNative.Snapshot(this);generation++;
            bool resolution=before.Device==now.Device&&before.ModeWidth>0&&now.ModeWidth>0&&(before.ModeWidth!=now.ModeWidth||before.ModeHeight!=now.ModeHeight);
            bool dpi=before.DpiReliable&&now.DpiReliable&&before.Device==now.Device&&before.Dpi!=now.Dpi;
            if(!environmentChanged&&(index==6&&msg==0x7e&&resolution||index==7&&msg==0x2e0&&dpi)){environmentChanged=true;changedAt=clock.Elapsed.TotalMilliseconds;}
            Emit("environment_notification","notification_id",Guid.NewGuid().ToString("N"),"native_message",msg,"raw_wparam",wp,"raw_lparam",lp,"native_timestamp_ms",unchecked((uint)GeometryNative.GetMessageTime()),
                "before_dpi_reliable",last.DpiReliable,"before_device",last.Device,"before_width",last.ModeWidth,"before_height",last.ModeHeight,"before_dpi",last.Dpi,
                "resolution_changed",resolution,"dpi_changed",dpi,"environment_changed",environmentChanged);
            last=now;environment.Text=EnvironmentText(now);
        }
        void Emit(string type,params object[] extra) {
            if(index<0||logger==null)return;var s=GeometryNative.Snapshot(this);Rectangle r=target.RectangleToScreen(target.ClientRectangle);
            var fields=new List<object>{"platform","windows","suite","geometry","case_id",Current.Id,"nonce",nonce,"trial",index+1,"schema","windows-geometry-v1","t_ms",clock.Elapsed.TotalMilliseconds,
                "layout_generation",generation,"window_handle",Handle.ToInt64(),"pid",Process.GetCurrentProcess().Id,"dpi",s.Dpi,"dpi_reliable",s.DpiReliable,"physical_coordinates",s.PhysicalCoordinates,"coordinate_basis",s.PhysicalCoordinates?"native_physical_pixels":"virtualized_or_unknown","dpi_basis","GetDpiForWindow/own-HWND","awareness_basis","GetWindowDpiAwarenessContext/PMv2",
                "display_device",s.Device,"display_width",s.ModeWidth,"display_height",s.ModeHeight,"display_x",s.OriginX,"display_y",s.OriginY,
                "target_handle",target.Handle.ToInt64(),"target_x",r.X,"target_y",r.Y,"target_width",r.Width,"target_height",r.Height};
            fields.AddRange(extra);logger.Log(type,fields.ToArray());
        }
    }
}
