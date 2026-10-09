using System;
using System.Text;
using System.Diagnostics;
using System.Windows.Forms;

namespace DesktopFeedback {
    internal sealed class RendererContext : ApplicationContext {
        readonly OutputPump output = new OutputPump();
        readonly InputPump input = new InputPump();
        readonly FeedbackState state = new FeedbackState();
        readonly FeedbackWindows windows;
        readonly Timer timer = new Timer();
        readonly Stopwatch clock = Stopwatch.StartNew();
        double lastHeartbeat, pointerDeadline, lastGeometryCheck;
        string lastGeometryError;
        bool ready, finished;
        internal RendererContext(Style style) {
            windows = new FeedbackWindows(style); windows.RequestStop = RequestStop;
            timer.Interval = 30; timer.Tick += Tick; timer.Start();
        }
        void RequestStop() {
            if (!ready || finished) return;
            Session current = state.RequestStop(); if (current == null) return;
            // Queue Stop BEFORE rendering. A layout/topology failure must drain
            // that cancellation through OutputPump.Finish, not escape the click event.
            try { output.Send(WireOutput.Stop(current)); windows.Render(state, false); }
            catch (WireFailure error) { Fail(error.Code); }
            catch (Exception) { Fail("renderer_ui_failed"); }
        }
        void Tick(object sender, EventArgs args) {
            try {
                if (!ready) {
                    // First message-loop tick: all real HWNDs initialized, shown,
                    // with PMv2 checked before creation. Pure tests never reach here.
                    string failure;
                    string capability = Native.RequestExclusion(windows.Handles, out failure);
                    output.Send(WireOutput.Ready(capability)); ready = true;
                    if (failure != null) output.Send(WireOutput.Error(failure));
                    lastHeartbeat = clock.Elapsed.TotalSeconds; input.Start();
                }
                string error; bool eof;
                Snapshot snapshot = input.Poll(out error, out eof);
                if (error != null) { Fail(error); return; }
                if (eof) { Finish(0); return; }
                double now = clock.Elapsed.TotalSeconds;
                bool changed = snapshot != null && state.Apply(snapshot);
                if (changed) pointerDeadline = now + (snapshot.Pointer != null && snapshot.Pointer.Kind == "click" ? 0.65 : 0.35);
                if (changed || now - lastGeometryCheck >= 1) {
                    string code = windows.Render(state, now < pointerDeadline);
                    if (code != null && code != lastGeometryError) output.Send(WireOutput.Error(code));
                    lastGeometryError = code; lastGeometryCheck = now;
                }
                if (now >= pointerDeadline) windows.ExpirePointer();
                if (now - lastHeartbeat >= 1) { output.Send(WireOutput.Heartbeat); lastHeartbeat = now; }
            } catch (WireFailure e) { Fail(e.Code); }
            catch (Exception) { Fail("renderer_ui_failed"); }
        }
        void Fail(string code) { output.Send(WireOutput.Error(code)); Finish(65); }
        void Finish(int code) {
            if (finished) return; finished = true; timer.Stop(); windows.Dispose(); output.Finish(code);
            // Exit is owned by output pump/watchdog; do not block the UI on flush.
        }
        protected override void Dispose(bool disposing) { if (disposing) timer.Dispose(); base.Dispose(disposing); }
    }
    internal static class Program {
        [STAThread]
        static int Main(string[] args) {
            // Branch before any WinForms/DPI/desktop API initialization.
            if (args.Length == 1 && args[0] == "--self-test") {
                try { SelfTests.Run(); return 0; }
                catch (WireFailure e) { Console.Error.WriteLine(e.Code); return 1; }
                catch (Exception) { Console.Error.WriteLine("self_test_failed"); return 1; }
            }
            try {
                Style style = Style.Parse(args); Native.Initialize();
                Application.EnableVisualStyles(); Application.SetCompatibleTextRenderingDefault(false);
                Application.SetUnhandledExceptionMode(UnhandledExceptionMode.ThrowException);
                using (RendererContext context = new RendererContext(style)) Application.Run(context);
                return 0;
            } catch (WireFailure e) { WriteStartupError(e.Code); return 65; }
            catch (Exception) { WriteStartupError("gui_initialization_failed"); return 65; }
        }
        static void WriteStartupError(string code) {
            byte[] frame = new UTF8Encoding(false, true).GetBytes(WireOutput.Error(code) + "\n");
            try { Console.OpenStandardOutput().Write(frame, 0, frame.Length); }
            catch (Exception) { Console.Error.WriteLine("stdout_write_failed"); }
        }
    }
}
