// Owned B and modal D. Only explicit UI business buttons change window state.
using System;
using System.Drawing;
using System.Windows.Forms;
namespace AcceptanceFixture {
    sealed class FocusPeer : FocusWindow {
        public readonly FocusEditor Editor;
        readonly Label heading;
        readonly FocusButton minimize;
        public FocusPeer(FocusContext c):base(c,"B") {
            ClientSize=new Size(400,310);FormBorderStyle=FormBorderStyle.FixedToolWindow;ShowInTaskbar=false;
            StartPosition=FormStartPosition.Manual;BackColor=Color.FromArgb(224,238,255);
            heading=new Label{Dock=DockStyle.Top,Height=65,Font=new Font("Segoe UI",17,FontStyle.Bold),Padding=new Padding(8)};
            Editor=new FocusEditor(c,"B.edit"){Dock=DockStyle.Fill};
            minimize=new FocusButton(c,"B.minimize","MINIMIZE B"){Dock=DockStyle.Bottom};
            minimize.Click+=(s,e)=>{WindowState=FormWindowState.Minimized;};
            Controls.Add(Editor);Controls.Add(heading);Controls.Add(minimize);
        }
        protected override bool ShowWithoutActivation {get{return true;}}
        public void Prepare(FocusCase test,string nonce) {
            Text="FOCUS OWN B — "+test.Id+" — "+nonce;
            heading.Text="B EDITOR · "+test.Id+"\nNONCE "+nonce;
            Editor.Text="";Editor.Enabled=test.Spec!="unconfirmed_stop";minimize.Enabled=test.Spec=="minimize_restore";
        }
        public void RestoreFromOwnButton() {WindowState=FormWindowState.Normal;Show();Activate();}
    }
    sealed class FocusDialog : FocusWindow {
        public readonly FocusEditor Editor;
        public readonly Label Result;
        public readonly FocusButton CheckButton;
        public FocusDialog(FocusContext context,string caseID,string nonce,Action check):base(context,"D") {
            Text="FOCUS OWN MODAL D — "+caseID;ClientSize=new Size(550,420);FormBorderStyle=FormBorderStyle.FixedDialog;
            StartPosition=FormStartPosition.CenterParent;MaximizeBox=false;MinimizeBox=false;ShowInTaskbar=false;
            var layout=new TableLayoutPanel{Dock=DockStyle.Fill,RowCount=5,ColumnCount=1,Padding=new Padding(14)};
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute,76));layout.RowStyles.Add(new RowStyle(SizeType.Percent,100));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute,92));layout.RowStyles.Add(new RowStyle(SizeType.Absolute,55));layout.RowStyles.Add(new RowStyle(SizeType.Absolute,55));
            layout.Controls.Add(new Label{Dock=DockStyle.Fill,Text="D EDITOR · "+caseID+"\nNONCE "+nonce,Font=new Font("Segoe UI",19,FontStyle.Bold)},0,0);
            Editor=new FocusEditor(context,"D.edit"){Dock=DockStyle.Fill};layout.Controls.Add(Editor,0,1);
            Result=new Label{Dock=DockStyle.Fill,Text="Own modal: A is disabled naturally.",Font=new Font("Segoe UI",14)};layout.Controls.Add(Result,0,2);
            CheckButton=new FocusButton(context,"admin.check","Check once (inside D)"){Dock=DockStyle.Fill};CheckButton.Click+=(s,e)=>check();layout.Controls.Add(CheckButton,0,3);
            var close=new FocusButton(context,"D.close","CLOSE D"){Dock=DockStyle.Fill};close.Click+=(s,e)=>Close();layout.Controls.Add(close,0,4);
            Controls.Add(layout);
        }
    }
}
