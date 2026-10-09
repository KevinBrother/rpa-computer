using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace DesktopFeedback {
    internal static class Native {
        [DllImport("user32.dll", SetLastError = true)] static extern bool SetProcessDpiAwarenessContext(IntPtr value);
        [DllImport("user32.dll")] static extern IntPtr GetThreadDpiAwarenessContext();
        [DllImport("user32.dll")] static extern bool AreDpiAwarenessContextsEqual(IntPtr a, IntPtr b);
        [DllImport("user32.dll")] internal static extern uint GetDpiForWindow(IntPtr window);
        [DllImport("user32.dll", SetLastError = true)] static extern bool SetWindowDisplayAffinity(IntPtr window, uint affinity);
        [DllImport("user32.dll", SetLastError = true)] static extern bool GetWindowDisplayAffinity(IntPtr window, out uint affinity);
        [DllImport("dwmapi.dll")] static extern int DwmIsCompositionEnabled([MarshalAs(UnmanagedType.Bool)] out bool enabled);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool GetVersionEx(ref VersionInfo info);
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        struct VersionInfo {
            internal uint Size, Major, Minor, Build, Platform;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] internal string ServicePack;
        }
        internal static void Initialize() {
            if (!Environment.UserInteractive || Process.GetCurrentProcess().SessionId == 0) throw new WireFailure("interactive_desktop_required");
            try {
                IntPtr pmv2 = new IntPtr(-4);
                if (!AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), pmv2)) {
                    if (!SetProcessDpiAwarenessContext(pmv2)) { Diagnostic("pmv2_initialization_failed"); throw new WireFailure("pmv2_initialization_failed"); }
                }
                if (!AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), pmv2)) throw new WireFailure("pmv2_initialization_failed");
            } catch (EntryPointNotFoundException) { throw new WireFailure("pmv2_unsupported"); }
        }
        internal static string RequestExclusion(IntPtr[] windows, out string failure) {
            failure = null;
            try {
                // supportedOS manifest makes GetVersionEx report the actual Windows 10+ build.
                VersionInfo version = new VersionInfo(); version.Size = (uint)Marshal.SizeOf(typeof(VersionInfo));
                if (!GetVersionEx(ref version)) { failure = "capture_os_version_failed"; Diagnostic(failure); return "unsupported"; }
                if (version.Major < 10 || (version.Major == 10 && version.Build < 19041)) { failure = "capture_exclusion_unsupported_os"; return "unsupported"; }
                bool enabled;
                if (DwmIsCompositionEnabled(out enabled) != 0 || !enabled) { failure = "capture_dwm_unavailable"; return "unsupported"; }
                foreach (IntPtr window in windows) {
                    uint affinity;
                    if (!SetWindowDisplayAffinity(window, 0x11)) { failure = "capture_exclusion_request_failed"; Diagnostic(failure); Rollback(windows); return "unsupported"; }
                    if (!GetWindowDisplayAffinity(window, out affinity) || affinity != 0x11) { failure = "capture_exclusion_readback_failed"; Diagnostic(failure); Rollback(windows); return "unsupported"; }
                }
                return "requested"; // NOT proof that screenshots::GDI actually excludes these HWNDs.
            } catch (EntryPointNotFoundException) { failure = "capture_exclusion_api_missing"; Rollback(windows); return "unsupported"; }
        }
        static void Rollback(IntPtr[] windows) {
            // No fallback to WDA_MONITOR: avoid partial or blacked-out overlays.
            try { foreach (IntPtr window in windows) if (!SetWindowDisplayAffinity(window, 0)) Diagnostic("capture_exclusion_rollback_failed"); }
            catch (EntryPointNotFoundException) { Console.Error.WriteLine("capture_exclusion_rollback_api_missing"); }
        }
        static void Diagnostic(string code) { Console.Error.WriteLine(code + " win32=" + Marshal.GetLastWin32Error()); }
    }
}
