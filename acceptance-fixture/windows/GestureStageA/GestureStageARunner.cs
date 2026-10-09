// Standalone StageA entrypoint; deliberately excluded from the root fixture build.
// CC+GLM execute/review on Windows. Author may compile, never execute this assembly.
using System;
using System.Web.Script.Serialization;
using System.Windows.Forms;
namespace AcceptanceFixture {
    static class GestureStageARunner {
        private static int failures;
        internal static void Emit(object value) {
            Console.WriteLine(new JavaScriptSerializer().Serialize(value));
            Console.Out.Flush();
        }
        internal static void Check(string name, bool ok, string detail) {
            if (!ok) failures++;
            Emit(new { type="contract", name=name, passed=ok, detail=detail, gui_proof=false });
        }
        internal static T Field<T>(object obj, string name) {
            var field=obj.GetType().GetField(name, System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic);
            if (field==null) throw new InvalidOperationException("production field absent: "+name);
            return (T)field.GetValue(obj);
        }
        internal static void StartNextTrial(GestureForm form) {
            var method=typeof(GestureForm).GetMethod("StartTrial", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic);
            if (method==null) throw new InvalidOperationException("production StartTrial absent");
            method.Invoke(form, null);
        }
        [STAThread]
        static int Main(string[] args) {
            try {
                if (Environment.OSVersion.Platform!=PlatformID.Win32NT)
                    throw new InvalidOperationException("Windows-only; no Mac/Linux execution permitted");
                if (args.Length==2 && args[0]=="--replay") GestureStageAContracts.Run(args[1]);
                else if (args.Length==1 && (args[0]=="--layout" || args[0]=="--native-probe")) {
                    Application.EnableVisualStyles();
                    Application.SetCompatibleTextRenderingDefault(false);
                    if (args[0]=="--layout") GestureStageALayout.Run();
                    else GestureStageANativeProbe.Run();
                } else throw new ArgumentException("usage: --replay <frozen-oracle.jsonl> | --layout | --native-probe");
                Emit(new { type="summary", failures=failures, status=args[0]=="--native-probe"?"diagnostic_only":failures==0?"contracts_passed":"contracts_red",
                    scope="StageA only; native probe is diagnostic, not acceptance or capability proof" });
                return failures==0?0:1;
            } catch (Exception ex) {
                Emit(new { type="precondition_error", error=ex.ToString(), status="blocked_not_red" });
                return 2;
            }
        }
    }
}
