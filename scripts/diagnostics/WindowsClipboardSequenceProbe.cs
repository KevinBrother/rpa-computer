// Standalone C#/.NET Framework metadata probe; not product code.
// Compile with existing Windows csc.exe. Do not run until operator cleanup is complete.
// Only clipboard operations: sequence query and ONE OpenClipboard(NULL)/CloseClipboard.
using System;
using System.Runtime.InteropServices;
using System.Security.Principal;

internal static class WindowsClipboardSequenceProbe
{
    [StructLayout(LayoutKind.Sequential)]
    private struct ObjectBasicInfo {
        public uint Attributes, GrantedAccess, HandleCount, PointerCount;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 10)] public uint[] Reserved;
    }
    [DllImport("user32.dll", SetLastError = true)] private static extern uint GetClipboardSequenceNumber();
    [DllImport("kernel32.dll")] private static extern void SetLastError(uint error);
    [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr GetProcessWindowStation();
    [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr GetThreadDesktop(uint tid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool GetUserObjectInformationW(IntPtr h, int index, IntPtr data, uint size, out uint needed);
    [DllImport("ntdll.dll")] private static extern int NtQueryObject(IntPtr h, int kind, ref ObjectBasicInfo data, uint size, out uint returned);
    [DllImport("kernel32.dll")] private static extern uint GetCurrentProcessId();
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
    [DllImport("kernel32.dll")] private static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll")] private static extern IntPtr GetCurrentThread();
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool ProcessIdToSessionId(uint pid, out uint session);
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr h);
    [DllImport("advapi32.dll", SetLastError = true)] private static extern bool OpenProcessToken(IntPtr h, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)] private static extern bool OpenThreadToken(IntPtr h, uint access, bool asSelf, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)] private static extern bool GetTokenInformation(IntPtr token, int kind, IntPtr data, uint size, out uint needed);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool OpenClipboard(IntPtr owner);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool CloseClipboard();

    private static void Sequence(string label) {
        SetLastError(0);
        uint sequence = GetClipboardSequenceNumber();
        uint error = unchecked((uint)Marshal.GetLastWin32Error()); // Observation only: this API has NO documented last-error contract.
        Console.WriteLine("{0}.sequence={1}\n{0}.last_error_observation={2}", label, sequence, error);
    }
    private static void UserObject(string label, IntPtr handle) {
        Console.WriteLine("{0}.handle=0x{1:X}", label, handle.ToInt64());
        if (handle == IntPtr.Zero) return;
        IntPtr buffer = Marshal.AllocHGlobal(2048);
        try {
            uint needed;
            if (GetUserObjectInformationW(handle, 2, buffer, 2048, out needed)) // UOI_NAME
                Console.WriteLine("{0}.name={1}", label, Marshal.PtrToStringUni(buffer));
            else Console.WriteLine("{0}.name_error={1}", label, Marshal.GetLastWin32Error());
            if (GetUserObjectInformationW(handle, 1, buffer, 2048, out needed)) // UOI_FLAGS
                Console.WriteLine("{0}.flags=0x{1:X8}", label, Marshal.ReadInt32(buffer, 8));
            else Console.WriteLine("{0}.flags_error={1}", label, Marshal.GetLastWin32Error());
        } finally { Marshal.FreeHGlobal(buffer); }
        ObjectBasicInfo info = new ObjectBasicInfo { Reserved = new uint[10] };
        uint size = (uint)Marshal.SizeOf(typeof(ObjectBasicInfo));
        uint returned;
        try {
            int status = NtQueryObject(handle, 0, ref info, size, out returned);
            Console.WriteLine("{0}.query_ntstatus=0x{1:X8}\n{0}.query_returned_bytes={2}", label, status, returned);
            if (status >= 0 && returned >= size) {
                Console.WriteLine("{0}.granted_access=0x{1:X8}", label, info.GrantedAccess);
                if (label == "station") Console.WriteLine("station.has_WINSTA_ACCESSCLIPBOARD={0}", (info.GrantedAccess & 4) != 0);
            } else Console.WriteLine("{0}.granted_access=UNKNOWN", label);
        } catch (EntryPointNotFoundException) { Console.WriteLine("{0}.granted_access=UNKNOWN (API unavailable)", label); }
    }
    private static void Token(string label, IntPtr token) {
        IntPtr buffer = Marshal.AllocHGlobal(4096);
        try {
            uint needed;
            if (GetTokenInformation(token, 1, buffer, 4096, out needed)) // TOKEN_USER first field is SID pointer
                Console.WriteLine("{0}.user_sid={1}", label, new SecurityIdentifier(Marshal.ReadIntPtr(buffer)).Value);
            else Console.WriteLine("{0}.user_error={1}", label, Marshal.GetLastWin32Error());
            int[] classes = { 12, 20, 8 }; // TokenSessionId, TokenElevation, TokenType
            string[] names = { "session", "elevated", "type" };
            for (int i = 0; i < classes.Length; i++) {
                if (GetTokenInformation(token, classes[i], buffer, 4096, out needed))
                    Console.WriteLine("{0}.{1}={2}", label, names[i], Marshal.ReadInt32(buffer));
                else Console.WriteLine("{0}.{1}_error={2}", label, names[i], Marshal.GetLastWin32Error());
            }
            if (GetTokenInformation(token, 10, buffer, 4096, out needed)) // TOKEN_STATISTICS AuthenticationId at offset 8
                Console.WriteLine("{0}.auth_luid={1:X8}:{2:X8}", label, Marshal.ReadInt32(buffer, 12), Marshal.ReadInt32(buffer, 8));
            else Console.WriteLine("{0}.statistics_error={1}", label, Marshal.GetLastWin32Error());
            if (GetTokenInformation(token, 25, buffer, 4096, out needed))
                Console.WriteLine("{0}.integrity_sid={1}", label, new SecurityIdentifier(Marshal.ReadIntPtr(buffer)).Value);
            else Console.WriteLine("{0}.integrity_error={1}", label, Marshal.GetLastWin32Error());
        } finally { Marshal.FreeHGlobal(buffer); CloseHandle(token); }
    }
    public static int Main() {
        Console.WriteLine("probe=clipboard-sequence-readonly-v2\nutc={0:o}\nscope=SELF ONLY; not frozen host\nsequence_last_error_contract=NONE", DateTime.UtcNow);
        Console.WriteLine("pid={0}\ntid={1}\npointer_bits={2}\nimage={3}", GetCurrentProcessId(), GetCurrentThreadId(), IntPtr.Size * 8, typeof(WindowsClipboardSequenceProbe).Assembly.Location);
        Sequence("before");
        uint session;
        if (ProcessIdToSessionId(GetCurrentProcessId(), out session)) Console.WriteLine("process.session={0}", session);
        else Console.WriteLine("process.session_error={0}", Marshal.GetLastWin32Error());
        IntPtr station = GetProcessWindowStation();
        if (station == IntPtr.Zero) Console.WriteLine("station.get_error={0}", Marshal.GetLastWin32Error());
        UserObject("station", station);
        IntPtr desktop = GetThreadDesktop(GetCurrentThreadId());
        if (desktop == IntPtr.Zero) Console.WriteLine("desktop.get_error={0}", Marshal.GetLastWin32Error());
        UserObject("desktop", desktop); // Borrowed station/desktop handles: never close or switch.
        IntPtr token;
        if (OpenProcessToken(GetCurrentProcess(), 8, out token)) Token("process_token", token);
        else Console.WriteLine("process_token.open_error={0}", Marshal.GetLastWin32Error());
        if (OpenThreadToken(GetCurrentThread(), 8, true, out token)) Token("thread_token", token);
        else Console.WriteLine("thread_token.open_error={0} (1008 means no thread token)", Marshal.GetLastWin32Error());
        Sequence("pre_open");
        bool opened = OpenClipboard(IntPtr.Zero);
        int openError = opened ? 0 : Marshal.GetLastWin32Error();
        bool closed = false;
        int closeError = 0;
        uint lockedSequence = 0;
        uint lockedError = 0;
        // No console output while the lock is held. No retry, sleep, reads, or writes.
        if (opened) {
            try {
                SetLastError(0);
                lockedSequence = GetClipboardSequenceNumber();
                lockedError = unchecked((uint)Marshal.GetLastWin32Error());
            } finally {
                closed = CloseClipboard();
                closeError = closed ? 0 : Marshal.GetLastWin32Error();
            }
        }
        Console.WriteLine("clipboard.open_ok={0}\nclipboard.open_error={1}", opened, openError);
        if (opened) Console.WriteLine("locked.sequence={0}\nlocked.last_error_observation={1}\nclipboard.close_ok={2}\nclipboard.close_error={3}", lockedSequence, lockedError, closed, closeError);
        Sequence("after");
        // 0 means report emitted and acquired lock released, NOT paste proved.
        return opened && !closed ? 2 : 0;
    }
}
