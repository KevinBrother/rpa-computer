// Supplemental CC+GLM-only Windows layout coverage. Not part of the product build.
// Uses the real GestureForm, including Check/Finish footer states, without input injection.
using System;
using System.Drawing;
using System.Linq;
using System.Reflection;
using System.Web.Script.Serialization;
using System.Windows.Forms;
namespace AcceptanceFixture {
    static class GestureStageBLayoutContracts {
        private static int failures;
        private static void Emit(object value) { Console.WriteLine(new JavaScriptSerializer().Serialize(value)); }
        private static void Check(string name,bool ok,string detail) {
            if(!ok) failures++;
            Emit(new {type="contract",name=name,passed=ok,detail=detail,gui_proof=false});
        }
        private static T Field<T>(GestureForm form,string name) {
            return (T)typeof(GestureForm).GetField(name,BindingFlags.Instance|BindingFlags.NonPublic).GetValue(form);
        }
        private static void Invoke(GestureForm form,string name) {
            typeof(GestureForm).GetMethod(name,BindingFlags.Instance|BindingFlags.NonPublic).Invoke(form,null);
        }
        private static void Inspect(GestureForm form,string suite,string id,string phase) {
            string prefix=id+"-"+phase;
            var controls=form.Controls.Cast<Control>().Where(c=>c.Visible).ToArray();
            var instruction=Field<Label>(form,"instruction");
            var sentence=Field<GestureSentence>(form,"sentence");
            var canvas=Field<GestureCanvas>(form,"canvas");
            var current=GestureCatalog.CasesFor(suite).Single(c=>c.Id==id);
            Check(prefix+"-canonical-goal-preserved",instruction.Text.StartsWith(current.Instruction,StringComparison.Ordinal),instruction.Text);
            Check(prefix+"-instruction-font-unchanged",instruction.Font.Name=="Segoe UI" && instruction.Font.SizeInPoints==20 && instruction.Font.Bold,instruction.Font.ToString());
            Check(prefix+"-native-input-geometry-unchanged",canvas.Size==new Size(860,300) && sentence.Font.SizeInPoints==24 && sentence.ReadOnly && !sentence.Multiline,
                "canvas="+canvas.Size+" sentence="+sentence.Font);
            var working=Screen.PrimaryScreen.WorkingArea;
            Check(prefix+"-window-inside-working-area",working.Contains(form.Bounds),"working="+working+" window="+form.Bounds);
            using(var graphics=form.CreateGraphics()) {
                if(suite=="multiclick" && graphics.DpiY==96)
                    Check(prefix+"-minimum-viewport-96dpi",form.ClientSize==new Size(900,760),"actual client="+form.ClientSize);
                Emit(new {type="layout",case_id=id,phase=phase,dpi_y=graphics.DpiY,client=form.ClientSize.ToString(),
                    controls=controls.Select(c=>new {kind=c.GetType().Name,bounds=c.Bounds.ToString(),text=c.Text}).ToArray()});
            }
            foreach(var control in controls) {
                Check(prefix+"-within-client-"+control.GetType().Name,form.ClientRectangle.Contains(control.Bounds),control.Bounds.ToString());
                var label=control as Label;
                if(label!=null) {
                    var preferred=label.GetPreferredSize(new Size(label.Width,0));
                    Check(prefix+"-label-unclipped-"+label.Text,preferred.Height<=label.Height && preferred.Width<=label.Width,
                        "preferred="+preferred+" actual="+label.Size);
                }
            }
            for(int i=0;i<controls.Length;i++) for(int j=i+1;j<controls.Length;j++)
                Check(prefix+"-no-visible-overlap-"+i+"-"+j,!controls[i].Bounds.IntersectsWith(controls[j].Bounds),
                    controls[i].GetType().Name+":"+controls[i].Bounds+" vs "+controls[j].GetType().Name+":"+controls[j].Bounds);
            if(current.Field("flow")=="select_line") {
                Check(prefix+"-original-caveat-retained",instruction.Text.Contains("On Windows native EDIT, report the visible selection; whole-line selection is not guaranteed."),instruction.Text);
                Check(prefix+"-whole-line-criterion-visible",instruction.Text.Contains("Only the whole sentence satisfies this goal."),instruction.Text);
            }
        }
        [STAThread]
        static int Main(string[] args) {
            try {
                if(Environment.OSVersion.Platform!=PlatformID.Win32NT || args.Length!=0)
                    throw new InvalidOperationException("Windows-only; no arguments; no Mac/Linux tests permitted");
                Application.EnableVisualStyles();Application.SetCompatibleTextRenderingDefault(false);
                foreach(string suite in GestureCatalog.Suites) {
                    Exception error=null;
                    using(var form=new GestureForm(suite,1UL,null)) {
                        form.Shown+=(s,e)=> {
                            try {
                                var cases=GestureCatalog.CasesFor(suite);
                                for(int n=0;n<cases.Length;n++) {
                                    if(n>0) Invoke(form,"StartTrial");
                                    Inspect(form,suite,cases[n].Id,"ready");
                                    Invoke(form,"Check"); // Empty events: test footer layout, NOT gesture success.
                                    Inspect(form,suite,cases[n].Id,"checked-no-input");
                                }
                                Invoke(form,"Next");
                                Inspect(form,suite,cases[cases.Length-1].Id,"finished");
                            } catch(Exception ex) {error=ex;}
                            finally {form.Close();}
                        };
                        Application.Run(form);
                    }
                    if(error!=null) throw new InvalidOperationException("production layout inspection failed",error);
                }
                Emit(new {type="summary",failures=failures,gui_proof=false});
                return failures==0?0:1;
            } catch(Exception ex) {Emit(new {type="precondition_error",error=ex.ToString()});return 2;}
        }
    }
}
