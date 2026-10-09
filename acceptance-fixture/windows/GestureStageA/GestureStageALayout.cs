// Native Windows measurement of ACTUAL production labels, not copied layout constants.
using System;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;
namespace AcceptanceFixture {
    static class GestureStageALayout {
        internal static void Run() {
            Exception error=null;
            using(var form=new GestureForm("multiclick",1UL,null)) {
                // Production Shown handler runs StartTrial first; never overwrite label text/bounds/font.
                form.Shown+=(sender,args)=> {
                    try {
                        for(int n=0;n<GestureCatalog.MulticlickCases.Length;n++) {
                            if(n>0) GestureStageARunner.StartNextTrial(form);
                            form.PerformLayout();
                            var instruction=GestureStageARunner.Field<Label>(form,"instruction");
                            var c=GestureCatalog.MulticlickCases[n];
                            using(var graphics=instruction.CreateGraphics()) {
                                var available=new Size(instruction.ClientSize.Width-instruction.Padding.Horizontal,
                                    instruction.ClientSize.Height-instruction.Padding.Vertical);
                                // Native Label preferred size accounts for actual text rendering mode/flags.
                                var preferred=instruction.GetPreferredSize(new Size(instruction.Width,0));
                                // Unbounded-height wrap measurement is supplemental, not a screenshot oracle.
                                var measured=TextRenderer.MeasureText(graphics,instruction.Text,instruction.Font,
                                    new Size(available.Width,int.MaxValue),TextFormatFlags.WordBreak | TextFormatFlags.TextBoxControl);
                                GestureStageARunner.Emit(new {type="layout_measurement",case_id=c.Id,
                                    text=instruction.Text,width=instruction.Width,height=instruction.Height,
                                    preferred_width=preferred.Width,preferred_height=preferred.Height,
                                    measured_width=measured.Width,measured_height=measured.Height,
                                    dpi_x=graphics.DpiX,dpi_y=graphics.DpiY,font=instruction.Font.Name,
                                    font_points=instruction.Font.SizeInPoints,compatible=instruction.UseCompatibleTextRendering,
                                    auto_ellipsis=instruction.AutoEllipsis,gui_proof=false});
                                GestureStageARunner.Check(c.Id+"-full-instruction-fits-real-label",
                                    preferred.Height<=instruction.Height && preferred.Width<=instruction.Width,
                                    "actual preferred="+preferred+" bounds="+instruction.Bounds+"; CC screenshot gate required");
                                if(!instruction.UseCompatibleTextRendering)
                                    GestureStageARunner.Check(c.Id+"-wrapped-height-fits",measured.Height<=available.Height,
                                        "native measured height="+measured.Height+" available="+available.Height);
                            }
                            GestureStageARunner.Check(c.Id+"-instruction-keeps-user-goal",instruction.Text.StartsWith(c.Instruction,StringComparison.Ordinal),
                                "Native caveat must not silently replace canonical user goal");
                            GestureStageARunner.Check(c.Id+"-instruction-within-form",form.ClientRectangle.Contains(instruction.Bounds),
                                "instruction="+instruction.Bounds+" client="+form.ClientRectangle);
                            var overlaps=form.Controls.Cast<Control>().Where(other=>other!=instruction && other.Visible &&
                                other.Bounds.IntersectsWith(instruction.Bounds)).Select(other=>other.GetType().Name+":"+other.Bounds).ToArray();
                            GestureStageARunner.Check(c.Id+"-instruction-no-visible-overlap",overlaps.Length==0,string.Join(";",overlaps));
                        }
                    } catch(Exception ex) {error=ex;}
                    finally {form.Close();}
                };
                Application.Run(form);
            }
            if(error!=null) throw new InvalidOperationException("actual layout inspection failed",error);
        }
    }
}
