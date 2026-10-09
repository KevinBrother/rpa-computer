// Measured owned-form layout. Fonts, input geometry and raw input handling stay unchanged.
using System;
using System.Drawing;
using System.Windows.Forms;
namespace AcceptanceFixture {
    sealed partial class GestureForm {
        private const int RowGap=8, EdgeMargin=12;
        private static void FitLabel(Label label,string reserveText=null) {
            var preferred=label.GetPreferredSize(new Size(label.Width,0));
            int height=preferred.Height;
            if(!label.UseCompatibleTextRendering) using(var graphics=label.CreateGraphics()) {
                var wrapped=TextRenderer.MeasureText(graphics,label.Text,label.Font,
                    new Size(label.ClientSize.Width-label.Padding.Horizontal,int.MaxValue),
                    TextFormatFlags.WordBreak|TextFormatFlags.TextBoxControl);
                height=Math.Max(height,wrapped.Height+label.Padding.Vertical);
                if(reserveText!=null) {
                    var reserved=TextRenderer.MeasureText(graphics,reserveText,label.Font,
                        new Size(label.ClientSize.Width-label.Padding.Horizontal,int.MaxValue),
                        TextFormatFlags.WordBreak|TextFormatFlags.TextBoxControl);
                    height=Math.Max(height,reserved.Height+label.Padding.Vertical);
                }
            }
            // Small slack for rasterization, not smaller fonts or hidden/ellipsized text.
            label.Height=height+2;
        }
        private static int PlaceRow(Control control,int top) {
            control.Top=top;return control.Bottom+RowGap;
        }
        private void LayoutTrial() {
            // Only called at trial/check/finish boundaries, NEVER from Record/WndProc.
            // Do not move an input target while a native multi-click/drag is in flight.
            SuspendLayout();
            try {
                foreach(var label in new[]{title,nonceLabel,instruction,status,offsetA,offsetB}) FitLabel(label);
                // Bound the counter text without changing it or relaying out during input.
                FitLabel(live,"Native presses: 2147483647 · events: 2147483647 · down outside");
                live.Height=Math.Max(34,live.Height);
                status.Height=Math.Max(92,status.Height);
                int y=EdgeMargin;
                y=PlaceRow(title,y);y=PlaceRow(nonceLabel,y);y=PlaceRow(instruction,y);y=PlaceRow(live,y);
                if(suite=="scroll") {
                    offsetA.Top=offsetB.Top=y;
                    panelA.Top=panelB.Top=y+Math.Max(offsetA.Height,offsetB.Height)+RowGap;
                    y=Math.Max(panelA.Bottom,panelB.Bottom)+RowGap;
                } else if(sentence.Visible) {
                    // A text trial must not reserve the hidden 300px canvas above its EDIT.
                    y=PlaceRow(sentence,y);
                } else y=PlaceRow(canvas,y);
                status.Top=y;
                var buttons=new[]{check,next,close};
                int buttonHeight=40;
                foreach(var button in buttons) {
                    var preferred=button.GetPreferredSize(Size.Empty);
                    button.Width=Math.Max(110,preferred.Width);
                    buttonHeight=Math.Max(buttonHeight,preferred.Height);
                }
                int requiredClientHeight=status.Bottom+RowGap+buttonHeight+EdgeMargin;
                int clientHeight=Math.Max(760,requiredClientHeight);
                var working=Screen.PrimaryScreen.WorkingArea;
                var requiredWindow=SizeFromClientSize(new Size(900,clientHeight));
                if(requiredWindow.Width>working.Width || requiredWindow.Height>working.Height)
                    throw new InvalidOperationException("primary screen too small for full readable fixture: required window "
                        +requiredWindow+", working area "+working.Size+"; no text or input area has been reduced");
                ClientSize=new Size(900,clientHeight);
                int buttonTop=ClientSize.Height-EdgeMargin-buttonHeight;
                // Keep feedback adjacent to the footer, with any spare space above it.
                status.Top=buttonTop-RowGap-status.Height;
                int right=ClientSize.Width-20;
                for(int i=buttons.Length-1;i>=0;i--) {
                    var button=buttons[i];right-=button.Width;
                    button.Bounds=new Rectangle(right,buttonTop,button.Width,buttonHeight);right-=RowGap;
                }
                if(right<12) throw new InvalidOperationException("button row does not fit the readable fixture width");
                Location=new Point(Math.Max(working.Left,Math.Min(Left,working.Right-Width)),
                    Math.Max(working.Top,Math.Min(Top,working.Bottom-Height)));
            } finally {ResumeLayout(false);}
        }
    }
}
