// Standalone .NET Framework / C# 5 fixture. Do not add to windows/*.cs builds.
using System;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows.Forms;

namespace RpaTextPasteAcceptance
{
    internal static class SelfTest
    {
        // This path must stay ahead of all WinForms initialization in Main.
        internal static int Run()
        {
            try
            {
                Require(TextEncoding.NormalizePayload("a\nb\rc\r\nd\t") == "a\r\nb\r\nc\r\nd\t", "payload CRLF normalization");
                Require(TextEncoding.NormalizePayload("\r\n\n\r") == "\r\n\r\n\r\n", "adjacent line endings");
                Require(TextEncoding.Matches("a\nb", "a\r\nb"), "normalized expectation");
                Require(!TextEncoding.Matches("a\nb", "a\nb"), "actual LF must not be normalized");
                Require(!TextEncoding.Matches("abc", "abc "), "no trimming");
                Require(!TextEncoding.Matches("A1!", "Ａ１！"), "no width folding");
                Require(!TextEncoding.Matches("\u00e9", "e\u0301"), "no Unicode normalization");
                Require(!TextEncoding.Matches("ABC", "abc"), "ordinal case-sensitive comparison");
                Require(TextEncoding.Json("\"\\\b\f\n\r\t\0") == "\"\\\"\\\\\\b\\f\\n\\r\\t\\u0000\"", "JSON control escapes");
                Require(TextEncoding.Json("中文") == "\"中文\"", "readable Chinese JSON");
                Require(TextEncoding.Json("\ud83d\ude00\ud800") == "\"\\ud83d\\ude00\\ud800\"", "lossless surrogate escapes");
                Require(TextEncoding.Codepoints("中\ud83d\ude00\t\r\n\ud800") == "U+4E2D U+1F600 U+0009 U+000D U+000A U+D800", "scalar and unpaired surrogate diagnostics");
                Require(TextEncoding.Json("") == "\"\"" && TextEncoding.Codepoints("") == "", "empty values");
                Require(Cases.All.Length == 4, "four cases");
                foreach (PasteCase item in Cases.All)
                    Require(TextEncoding.Matches(item.Payload, item.Expected), "case expectation: " + item.Id);
                Require(Cases.All[3].Expected.IndexOf('\t') >= 0 && Cases.All[3].Expected.IndexOf("\r\n", StringComparison.Ordinal) >= 0, "literal tab and CRLF case");
                string row = TextEncoding.Evidence(Cases.All[3], "wrong\n\ud800", "unavailable");
                Require(row.IndexOf("\"match\":false", StringComparison.Ordinal) >= 0 && row.IndexOf('\n') < 0 && row.IndexOf('\r') < 0, "single-line mismatch evidence");
                Require(row.IndexOf("\\ud800", StringComparison.Ordinal) >= 0, "evidence preserves invalid UTF16");
                Console.WriteLine("PASS: all non-GUI text-paste self-tests");
                return 0;
            }
            catch (Exception error)
            {
                Console.Error.WriteLine("FAIL: " + error.Message);
                return 1;
            }
        }

        private static void Require(bool value, string description)
        {
            if (!value) throw new InvalidOperationException(description);
        }
    }

    internal sealed class PasteCase
    {
        internal readonly string Id;
        internal readonly string Name;
        internal readonly string Payload;
        internal readonly string Expected;

        internal PasteCase(string id, string name, string payload)
        {
            Id = id;
            Name = name;
            Payload = payload;
            Expected = TextEncoding.NormalizePayload(payload);
        }
    }

    internal static class Cases
    {
        internal static readonly PasteCase[] All = new PasteCase[]
        {
            new PasteCase("chinese", "Chinese", "中文粘贴验收：你好，世界！"),
            new PasteCase("mixed-ascii", "ASCII punctuation + Chinese + numbers", "中文ABC xyz 0123456789 !@#$%^&*()_+-=[]{};:'\",.<>/?\\|`~"),
            new PasteCase("emoji", "Emoji (UTF-16 surrogate pairs)", "表情 😀 🚀 🎉 结束"),
            new PasteCase("multiline-tab", "Multiline + literal TAB", "第一行：中文123\n第二行：左\t右 ABC!\n第三行：结束。")
        };
    }

    internal static class TextEncoding
    {
        internal static string NormalizePayload(string payload)
        {
            return payload.Replace("\r\n", "\n").Replace("\r", "\n").Replace("\n", "\r\n");
        }

        internal static bool Matches(string payload, string actual)
        {
            // Normalize ONLY the fixture payload, never the observed TextBox value.
            return String.Equals(NormalizePayload(payload), actual, StringComparison.Ordinal);
        }

        internal static string Json(string value)
        {
            StringBuilder result = new StringBuilder("\"");
            foreach (char ch in value)
            {
                switch (ch)
                {
                    case '"': result.Append("\\\""); break;
                    case '\\': result.Append("\\\\"); break;
                    case '\b': result.Append("\\b"); break;
                    case '\f': result.Append("\\f"); break;
                    case '\n': result.Append("\\n"); break;
                    case '\r': result.Append("\\r"); break;
                    case '\t': result.Append("\\t"); break;
                    default:
                        // Escape surrogate code units individually, including malformed input.
                        if (ch < 0x20 || Char.IsSurrogate(ch) || ch == '\u2028' || ch == '\u2029')
                            result.Append("\\u").Append(((int)ch).ToString("x4", CultureInfo.InvariantCulture));
                        else result.Append(ch);
                        break;
                }
            }
            return result.Append('"').ToString();
        }

        internal static string Codepoints(string value)
        {
            StringBuilder result = new StringBuilder();
            for (int i = 0; i < value.Length; i++)
            {
                int codepoint = value[i];
                if (Char.IsHighSurrogate(value[i]) && i + 1 < value.Length && Char.IsLowSurrogate(value[i + 1]))
                {
                    codepoint = Char.ConvertToUtf32(value[i], value[i + 1]);
                    i++;
                }
                if (result.Length > 0) result.Append(' ');
                result.Append("U+").Append(codepoint.ToString("X4", CultureInfo.InvariantCulture));
            }
            return result.ToString();
        }

        internal static string Evidence(PasteCase item, string actual, string ime)
        {
            return "{\"timestamp\":" + Json(DateTimeOffset.UtcNow.ToString("o", CultureInfo.InvariantCulture))
                + ",\"case\":" + Json(item.Id)
                + ",\"expected\":" + Json(item.Expected)
                + ",\"actual\":" + Json(actual)
                + ",\"expectedCodepoints\":" + Json(Codepoints(item.Expected))
                + ",\"actualCodepoints\":" + Json(Codepoints(actual))
                + ",\"expectedUtf16Length\":" + item.Expected.Length.ToString(CultureInfo.InvariantCulture)
                + ",\"actualUtf16Length\":" + actual.Length.ToString(CultureInfo.InvariantCulture)
                + ",\"match\":" + (Matches(item.Payload, actual) ? "true" : "false")
                + ",\"ime\":" + Json(ime) + "}";
        }
    }

    internal sealed class TextBoxIme
    {
        private const uint NativeFullShape = 0x0001 | 0x0008;
        private IntPtr savedWindow;
        private IntPtr savedContext;
        private bool savedOpen;
        private uint savedConversion;
        private uint savedSentence;
        internal string State = "IME not requested (no verified state).";

        internal string Enable(TextBox target)
        {
            // No layout switching, global hotkeys, or handles from other applications.
            target.Focus();
            IntPtr window = target.Handle;
            IntPtr context = IntPtr.Zero;
            try
            {
                context = ImmGetContext(window);
                if (context == IntPtr.Zero) return State = "IME unavailable: TextBox has no IMM context.";
                uint conversion, sentence;
                if (!ImmGetConversionStatus(context, out conversion, out sentence))
                    return State = "IME unavailable: cannot query conversion state; nothing changed.";
                if (savedContext == IntPtr.Zero)
                {
                    savedWindow = window;
                    savedContext = context;
                    savedOpen = ImmGetOpenStatus(context);
                    savedConversion = conversion;
                    savedSentence = sentence;
                }
                else if (savedContext != context || savedWindow != window)
                    return State = "IME unavailable: TextBox context changed; refusing to modify another context.";

                bool opened = ImmSetOpenStatus(context, true);
                bool converted = ImmSetConversionStatus(context, NativeFullShape, sentence);
                return State = Query(context) + " | setters open=" + opened + ", conversion=" + converted;
            }
            catch (DllNotFoundException) { return State = "IME unavailable: imm32.dll not available."; }
            catch (EntryPointNotFoundException) { return State = "IME unavailable: IMM entry point missing."; }
            catch (BadImageFormatException) { return State = "IME unavailable: incompatible IMM library."; }
            finally { if (context != IntPtr.Zero) ImmReleaseContext(window, context); }
        }

        internal string Refresh(TextBox target)
        {
            IntPtr context = IntPtr.Zero;
            IntPtr window = target.Handle;
            try
            {
                context = ImmGetContext(window);
                return State = context == IntPtr.Zero ? "IME unavailable: no TextBox IMM context." : Query(context);
            }
            catch (DllNotFoundException) { return State = "IME unavailable: imm32.dll not available."; }
            catch (EntryPointNotFoundException) { return State = "IME unavailable: IMM entry point missing."; }
            catch (BadImageFormatException) { return State = "IME unavailable: incompatible IMM library."; }
            finally { if (context != IntPtr.Zero) ImmReleaseContext(window, context); }
        }

        private static string Query(IntPtr context)
        {
            uint conversion, sentence;
            if (!ImmGetConversionStatus(context, out conversion, out sentence))
                return "IME unavailable: actual conversion state could not be queried.";
            bool open = ImmGetOpenStatus(context);
            bool full = open && (conversion & NativeFullShape) == NativeFullShape;
            return (full ? "IME VERIFIED: native + full width" : "IME NOT VERIFIED: requested native + full width not active")
                + " | open=" + open + " conversion=0x" + conversion.ToString("X8", CultureInfo.InvariantCulture)
                + " sentence=0x" + sentence.ToString("X8", CultureInfo.InvariantCulture);
        }

        internal void Restore(TextBox target)
        {
            if (savedContext == IntPtr.Zero || !target.IsHandleCreated || target.Handle != savedWindow) return;
            IntPtr context = IntPtr.Zero;
            try
            {
                context = ImmGetContext(savedWindow);
                // Never restore through a stale or replacement context handle.
                if (context == savedContext)
                {
                    ImmSetConversionStatus(context, savedConversion, savedSentence);
                    ImmSetOpenStatus(context, savedOpen);
                }
            }
            catch (DllNotFoundException) { }
            catch (EntryPointNotFoundException) { }
            catch (BadImageFormatException) { }
            finally
            {
                if (context != IntPtr.Zero) ImmReleaseContext(savedWindow, context);
                savedContext = IntPtr.Zero;
            }
        }

        [DllImport("imm32.dll")] private static extern IntPtr ImmGetContext(IntPtr window);
        [DllImport("imm32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool ImmReleaseContext(IntPtr window, IntPtr context);
        [DllImport("imm32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool ImmGetOpenStatus(IntPtr context);
        [DllImport("imm32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool ImmSetOpenStatus(IntPtr context, [MarshalAs(UnmanagedType.Bool)] bool open);
        [DllImport("imm32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool ImmGetConversionStatus(IntPtr context, out uint conversion, out uint sentence);
        [DllImport("imm32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool ImmSetConversionStatus(IntPtr context, uint conversion, uint sentence);
    }

    internal sealed class PasteForm : Form
    {
        private readonly string evidenceFile;
        private readonly TextBox input = new TextBox();
        private readonly Label heading = new Label();
        private readonly Label sample = new Label();
        private readonly Label escaped = new Label();
        private readonly Label result = new Label();
        private readonly Label imeLabel = new Label();
        private readonly Button check = new Button();
        private readonly Button clear = new Button();
        private readonly Button next = new Button();
        private readonly Button enableIme = new Button();
        private readonly TextBoxIme ime = new TextBoxIme();
        private readonly Timer shutdown = new Timer();
        private int caseIndex;
        private int passed;
        private bool checkedCase;

        internal PasteForm(string path)
        {
            evidenceFile = path;
            Text = "RPA Text Paste Acceptance 20261013";
            StartPosition = FormStartPosition.CenterScreen;
            ClientSize = new Size(1020, 820);
            MinimumSize = new Size(980, 800);
            AutoScaleMode = AutoScaleMode.Dpi;
            Font = new Font("Microsoft YaHei UI", 11F);
            BackColor = Color.White;
            ImeMode = ImeMode.NoControl;

            TableLayoutPanel layout = new TableLayoutPanel();
            layout.Dock = DockStyle.Fill;
            layout.Padding = new Padding(18);
            layout.ColumnCount = 1;
            layout.RowCount = 10;
            float[] heights = new float[] { 38, 40, 115, 76, 28, 140, 48, 70, 76, 45 };
            for (int i = 0; i < heights.Length; i++)
                layout.RowStyles.Add(new RowStyle(i == 5 ? SizeType.Percent : SizeType.Absolute, i == 5 ? 100 : heights[i]));
            Controls.Add(layout);

            heading.Font = new Font(Font.FontFamily, 16F, FontStyle.Bold);
            AddLabel(layout, heading, 0);
            AddLabel(layout, new Label { Text = "Read the sample below. Paste ONCE using explicit method=paste, then Check.\r\nExact UTF-16 comparison; spaces, punctuation, emoji and line endings matter." }, 1);
            sample.Font = new Font("Microsoft YaHei UI", 20F);
            sample.BackColor = Color.FromArgb(239, 246, 255);
            sample.Padding = new Padding(8);
            AddLabel(layout, sample, 2);
            escaped.Font = new Font("Consolas", 12F);
            AddLabel(layout, escaped, 3);
            AddLabel(layout, new Label { Text = "Paste target (TAB and ENTER insert literal characters):" }, 4);
            input.Multiline = true;
            input.AcceptsTab = true;
            input.AcceptsReturn = true;
            input.WordWrap = false;
            input.ScrollBars = ScrollBars.Both;
            input.Font = new Font("Microsoft YaHei UI", 18F);
            input.Dock = DockStyle.Fill;
            input.ImeMode = ImeMode.NoControl;
            input.AccessibleName = "Text paste target";
            layout.Controls.Add(input, 0, 5);

            FlowLayoutPanel actions = new FlowLayoutPanel { Dock = DockStyle.Fill, WrapContents = false };
            ConfigureButton(check, "Check", 100, delegate { CheckCase(); });
            ConfigureButton(clear, "Clear", 100, delegate { input.Clear(); input.Focus(); });
            ConfigureButton(next, "Next", 100, delegate { caseIndex++; ShowCase(); });
            ConfigureButton(enableIme, "Enable IME Full Width", 240, delegate { imeLabel.Text = ime.Enable(input); });
            actions.Controls.AddRange(new Control[] { check, clear, next, enableIme });
            layout.Controls.Add(actions, 0, 6);
            result.Font = new Font(Font.FontFamily, 19F, FontStyle.Bold);
            AddLabel(layout, result, 7);
            imeLabel.Text = ime.State;
            imeLabel.ForeColor = Color.FromArgb(125, 70, 0);
            AddLabel(layout, imeLabel, 8);
            AddLabel(layout, new Label { Text = "Escape closes this fixture only. Auto-close: 15 minutes from launch (not reset).\r\nEvidence is appended only by Check; there is no native-input baseline." }, 9);

            // Focus returns to the input before Check queries IME; no implicit AcceptButton.
            Shown += delegate { input.Focus(); };
            shutdown.Interval = 15 * 60 * 1000;
            shutdown.Tick += delegate { Close(); };
            shutdown.Start();
            ShowCase();
        }

        private static void AddLabel(TableLayoutPanel layout, Label label, int row)
        {
            label.Dock = DockStyle.Fill;
            label.UseMnemonic = false; // Literal '&' is part of the punctuation case.
            label.AutoEllipsis = false;
            layout.Controls.Add(label, 0, row);
        }

        private static void ConfigureButton(Button button, string text, int width, EventHandler action)
        {
            button.Text = text;
            button.Width = width;
            button.Height = 38;
            button.Click += action;
        }

        private void ShowCase()
        {
            PasteCase item = Cases.All[caseIndex];
            checkedCase = false;
            heading.Text = "Case " + (caseIndex + 1) + " / 4 - " + item.Name;
            sample.Text = item.Expected;
            escaped.Text = "Exact payload (JSON string; decode escapes, omit outer quotes):\r\n" + TextEncoding.Json(item.Expected);
            input.ReadOnly = false;
            input.Clear();
            check.Enabled = clear.Enabled = enableIme.Enabled = true;
            next.Enabled = false;
            result.Text = "WAITING - paste once, then Check";
            result.BackColor = Color.FromArgb(238, 242, 246);
            result.ForeColor = Color.FromArgb(30, 45, 60);
            input.Focus();
        }

        private void CheckCase()
        {
            if (checkedCase) return;
            checkedCase = true;
            input.Focus();
            string actual = input.Text; // Capture verbatim, before any UI state changes.
            PasteCase item = Cases.All[caseIndex];
            bool match = TextEncoding.Matches(item.Payload, actual);
            imeLabel.Text = ime.Refresh(input);
            bool recorded = false;
            try
            {
                File.AppendAllText(evidenceFile, TextEncoding.Evidence(item, actual, ime.State) + "\n", new UTF8Encoding(false, true));
                recorded = true;
                if (match) passed++;
                result.Text = match ? "PASS - exact UTF-16 match; evidence saved" : "FAIL - text differs; evidence saved";
            }
            catch (Exception error)
            {
                if (!(error is IOException) && !(error is UnauthorizedAccessException) && !(error is System.Security.SecurityException)) throw;
                result.Text = "FAIL - evidence write failed";
                imeLabel.Text = "Text match=" + match + "; " + error.Message;
            }
            bool accepted = match && recorded;
            result.BackColor = accepted ? Color.FromArgb(16, 110, 57) : Color.FromArgb(170, 25, 35);
            result.ForeColor = Color.White;
            input.ReadOnly = true;
            check.Enabled = clear.Enabled = enableIme.Enabled = false;
            next.Enabled = caseIndex < Cases.All.Length - 1;
            if (!next.Enabled) result.Text += "\r\nFinished: " + passed + "/4 matched and saved. Escape to close.";
        }

        protected override bool ProcessCmdKey(ref Message message, Keys keyData)
        {
            if (keyData == Keys.Escape) { Close(); return true; }
            return base.ProcessCmdKey(ref message, keyData);
        }

        protected override void OnFormClosing(FormClosingEventArgs e)
        {
            shutdown.Stop();
            // WinForms may detach an IME context while a TextBox is read-only.
            // Remove that restriction before trying to restore the original context.
            input.ReadOnly = false;
            ime.Restore(input); // Before child handles are destroyed; best effort only.
            base.OnFormClosing(e);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing) shutdown.Dispose();
            base.Dispose(disposing);
        }
    }

    internal static class Program
    {
        [STAThread]
        private static int Main(string[] args)
        {
            string evidenceFile = null;
            bool selfTest = false;
            try
            {
                for (int i = 0; i < args.Length; i++)
                {
                    if (args[i] == "--self-test") selfTest = true;
                    else if (args[i] == "--evidence-file" || args[i] == "--suite")
                    {
                        string option = args[i];
                        if (++i == args.Length || args[i].StartsWith("--", StringComparison.Ordinal))
                            throw new ArgumentException("Missing value for " + option);
                        if (option == "--evidence-file") evidenceFile = args[i];
                        // --suite is consumed and deliberately ignored for launcher compatibility.
                    }
                    else throw new ArgumentException("Unknown argument: " + args[i]);
                }
                if (selfTest) return SelfTest.Run();
                if (String.IsNullOrWhiteSpace(evidenceFile))
                    throw new ArgumentException("Required: --evidence-file <JSONL path>; optional: --suite <ignored> or --self-test.");
                evidenceFile = Path.GetFullPath(evidenceFile);
                // Fail before opening any GUI if the launcher's evidence destination is unusable.
                using (FileStream probe = new FileStream(evidenceFile, FileMode.Append, FileAccess.Write, FileShare.Read)) { }
                Application.EnableVisualStyles();
                Application.SetCompatibleTextRenderingDefault(false);
                using (PasteForm form = new PasteForm(evidenceFile)) Application.Run(form);
                return 0;
            }
            catch (Exception error)
            {
                Console.Error.WriteLine("TextPasteFixture: " + error.Message);
                return 2;
            }
        }
    }
}
