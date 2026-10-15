// Standalone .NET Framework / C# 5 fixture. Do not add to windows/*.cs builds.
using System;
using System.Drawing;
using System.Globalization;
using System.Collections.Generic;
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
                int diagnosticsFailures = RunDiagnosticsContracts();
                if (diagnosticsFailures != 0)
                    throw new InvalidOperationException("Observability/readability contracts failed: " + diagnosticsFailures);
                Console.WriteLine("PASS: all non-GUI text-paste self-tests");
                return 0;
            }
            catch (Exception error)
            {
                Console.Error.WriteLine("FAIL: " + error.Message);
                return 1;
            }
        }

        // Independent groups report names only: never print fixture payloads/oracle values.
        private static int RunDiagnosticsContracts()
        {
            int failures = 0;
            failures += Contract("canonical ambiguous characters", delegate
            {
                string encoded = ReadablePayload.Json("\uFF1A'\"` ");
                Require(encoded == "\"\\uFF1A\\u0027\\u0022\\u0060\\u0020\"", "explicit width/quote/backtick/space escapes");
            });
            failures += Contract("canonical payload preservation", delegate
            {
                foreach (PasteCase item in Cases.All)
                {
                    string before = item.Expected;
                    Require(DecodeJson(ReadablePayload.Json(before)) == before, "canonical roundtrip");
                    Require(item.Expected == before && TextEncoding.Matches(item.Payload, before), "case remains unchanged");
                }
                string raw = "A\nB\rC\r\n\t\0\ud83d\ude00\ud800\\\u2028";
                Require(DecodeJson(ReadablePayload.Json(raw)) == raw, "no hidden normalization or surrogate loss");
            });
            failures += Contract("canonical token-safe visual wrapping", delegate
            {
                string raw = "\uFF1A'\"` \ud83d\ude00\\\r\n\t";
                string[] lines = ReadablePayload.Lines(raw, 18);
                Require(lines.Length > 1, "long canonical display must wrap");
                Require(String.Concat(lines) == ReadablePayload.Json(raw), "visual wraps add no payload characters");
                foreach (string line in lines)
                {
                    Require(line.Length > 0 && line.Length <= 18, "bounded visual line");
                    // Parse each fragment independently: an escape/surrogate pair may not straddle rows.
                    string units = DecodeJsonFragment(line, true);
                    for (int i = 0; i < units.Length; i++)
                    {
                        if (Char.IsHighSurrogate(units[i]))
                            Require(++i < units.Length && Char.IsLowSurrogate(units[i]), "surrogate token stays on one row");
                        else Require(!Char.IsLowSurrogate(units[i]), "no detached low surrogate");
                    }
                }
                Require(DecodeJson(String.Concat(lines)) == raw, "wrapped display roundtrip");
            });
            failures += Contract("trace bounded metadata bookkeeping", delegate
            {
                MetadataTrace trace = new MetadataTrace(2);
                int calls = 0;
                Func<ImeSnapshot> capture = delegate { calls++; return new ImeSnapshot { Availability = ImeAvailability.Available, Conversion = 9 }; };
                trace.Observe(0, TraceBoundary.Paste, TracePhase.Before, capture);
                trace.Observe(0, TraceBoundary.Paste, TracePhase.After, capture);
                trace.Observe(0, TraceBoundary.FocusLost, TracePhase.Before, capture);
                Require(trace.Count == 2 && trace.Dropped == 1, "bounded entries and explicit overflow");
                Require(calls == 2, "full buffer avoids further query overhead");
                TraceEntry[] entries = trace.Snapshot();
                Require(entries.Length == 2 && entries[0].Sequence == 1 && entries[1].Sequence == 2, "ordered metadata sequence");
                Require(entries[0].CaseIndex == 0 && entries[0].Boundary == TraceBoundary.Paste && entries[0].Phase == TracePhase.Before, "boundary attribution");
                Require(entries[0].State.Conversion == 9 && entries[0].Timestamp > 0 && entries[1].Timestamp >= entries[0].Timestamp, "snapshot metadata and monotonic time");
                entries[0] = new TraceEntry();
                Require(trace.Snapshot()[0].Sequence == 1, "snapshot cannot modify buffered entries");
            });
            failures += Contract("trace query failure cannot interrupt dispatch", delegate
            {
                MetadataTrace trace = new MetadataTrace(4);
                int dispatches = 0;
                trace.Observe(0, TraceBoundary.Paste, TracePhase.Before, delegate { throw new InvalidOperationException("not for evidence"); });
                dispatches++; // Stands for exactly one base dispatch, not a native message or IMM call.
                trace.Observe(0, TraceBoundary.Paste, TracePhase.After, delegate { return new ImeSnapshot { Availability = ImeAvailability.NoContext }; });
                Require(dispatches == 1 && trace.Count == 2, "query exception does not bypass dispatch");
                TraceEntry[] entries = trace.Snapshot();
                Require(entries[0].State.Availability == ImeAvailability.QueryException, "query exception recorded without message text");
                Require(entries[1].State.Availability == ImeAvailability.NoContext, "unavailable context preserved");
            });
            failures += Contract("trace flush acknowledgment preserves sequence", delegate
            {
                MetadataTrace trace = new MetadataTrace(1);
                Func<ImeSnapshot> capture = delegate { return new ImeSnapshot { Availability = ImeAvailability.QueryFailed }; };
                trace.Observe(1, TraceBoundary.Key, TracePhase.Before, capture);
                trace.Observe(1, TraceBoundary.Key, TracePhase.After, capture);
                Require(trace.Snapshot().Length == 1 && trace.Count == 1 && trace.Dropped == 1, "snapshot alone never clears pending evidence");
                trace.MarkFlushed(); // Only called after successful append in GREEN integration.
                Require(trace.Count == 0 && trace.Dropped == 0, "acknowledged batch released");
                trace.Observe(2, TraceBoundary.FocusGained, TracePhase.After, capture);
                Require(trace.Snapshot()[0].Sequence == 3, "sequence includes dropped events and does not reset per batch");
            });
            return failures;
        }

        private static int Contract(string name, Action body)
        {
            try { body(); Console.WriteLine("PASS: " + name); return 0; }
            catch (Exception) { Console.Error.WriteLine("FAIL: " + name); return 1; }
        }

        // Test-only decoder, independent of the proposed canonical encoder.
        private static string DecodeJson(string value)
        {
            Require(value.Length >= 2 && value[0] == '"' && value[value.Length - 1] == '"', "JSON string delimiters");
            return DecodeJsonFragment(value.Substring(1, value.Length - 2), false);
        }

        private static string DecodeJsonFragment(string value, bool allowOuterQuotes)
        {
            StringBuilder result = new StringBuilder();
            for (int i = 0; i < value.Length; i++)
            {
                char ch = value[i];
                if (ch == '"') { Require(allowOuterQuotes, "unexpected unescaped quote"); continue; }
                if (ch != '\\') { Require(ch >= 0x20, "literal control in JSON"); result.Append(ch); continue; }
                Require(++i < value.Length, "split escape");
                switch (value[i])
                {
                    case '"': result.Append('"'); break;
                    case '\\': result.Append('\\'); break;
                    case '/': result.Append('/'); break;
                    case 'b': result.Append('\b'); break;
                    case 'f': result.Append('\f'); break;
                    case 'n': result.Append('\n'); break;
                    case 'r': result.Append('\r'); break;
                    case 't': result.Append('\t'); break;
                    case 'u':
                        Require(i + 4 < value.Length, "split Unicode escape");
                        result.Append((char)Int32.Parse(value.Substring(i + 1, 4), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture));
                        i += 4;
                        break;
                    default: throw new InvalidOperationException("Unknown JSON escape");
                }
            }
            return result.ToString();
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

    internal static class ReadablePayload
    {
        private static IEnumerable<string> Tokens(string value)
        {
            yield return "\"";
            for (int i = 0; i < value.Length; i++)
            {
                char ch = value[i];
                if (ch == '\uFF1A' || ch == '\'' || ch == '"' || ch == '`' || ch == ' ')
                    yield return "\\u" + ((int)ch).ToString("X4", CultureInfo.InvariantCulture);
                else
                {
                    int length = Char.IsHighSurrogate(ch) && i + 1 < value.Length && Char.IsLowSurrogate(value[i + 1]) ? 2 : 1;
                    string encoded = TextEncoding.Json(value.Substring(i, length));
                    yield return encoded.Substring(1, encoded.Length - 2);
                    i += length - 1;
                }
            }
            yield return "\"";
        }
        internal static string Json(string value) { return String.Concat(Tokens(value)); }
        internal static string[] Lines(string value, int maxColumns)
        {
            if (maxColumns < 12) throw new ArgumentOutOfRangeException("maxColumns");
            return Wrap(value, delegate(string line) { return line.Length <= maxColumns; });
        }
        internal static string[] Wrap(string value, Func<string, bool> fits)
        {
            List<string> lines = new List<string>();
            string line = "";
            foreach (string token in Tokens(value))
            {
                if (!fits(token)) throw new InvalidOperationException("Display too narrow for an intact token");
                if (line.Length > 0 && !fits(line + token)) { lines.Add(line); line = ""; }
                line += token;
            }
            if (line.Length > 0) lines.Add(line);
            return lines.ToArray();
        }
    }

    internal sealed class CanonicalView : Control
    {
        internal string Payload = "";
        internal CanonicalView() { DoubleBuffered = true; }
        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            const TextFormatFlags flags = TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding | TextFormatFlags.SingleLine;
            using (Font hint = new Font(Font.FontFamily, 10F))
                TextRenderer.DrawText(e.Graphics, "Canonical JSON: decode ONCE; omit outer quotes. Visual wraps add NOTHING.", hint, new Point(0, 0), ForeColor, flags);
            try
            {
                string[] rows = ReadablePayload.Wrap(Payload, delegate(string line) {
                    return TextRenderer.MeasureText(e.Graphics, line, Font, new Size(Int32.MaxValue, Int32.MaxValue), flags).Width <= ClientSize.Width;
                });
                int height = TextRenderer.MeasureText(e.Graphics, "Mg", Font, new Size(Int32.MaxValue, Int32.MaxValue), flags).Height;
                if (24 + rows.Length * height > ClientSize.Height) throw new InvalidOperationException("Display too short");
                for (int i = 0; i < rows.Length; i++)
                    TextRenderer.DrawText(e.Graphics, rows[i], Font, new Point(0, 24 + i * height), ForeColor, flags);
            }
            catch (Exception) { TextRenderer.DrawText(e.Graphics, "DISPLAY INCOMPLETE - do not enter text", Font, new Point(0, 28), Color.DarkRed, flags); }
        }
    }

    internal enum TraceBoundary { Paste, FocusGained, FocusLost, Key }
    internal enum TracePhase { Before, After }
    internal enum ImeAvailability { Available, NoContext, QueryFailed, QueryException }

    // Typed metadata only: no text, composition, clipboard, or arbitrary message fields.
    internal struct ImeSnapshot
    {
        internal IntPtr Window, Focus, Context;
        internal uint ThreadId, Conversion, Sentence;
        internal int Message, VirtualKey, ModifierMask;
        internal bool ModifiersAvailable, ReleaseFailed, CommandKey;
        internal bool Open;
        internal ImeAvailability Availability;
    }

    internal struct TraceEntry
    {
        internal long Sequence, Timestamp;
        internal int CaseIndex;
        internal TraceBoundary Boundary;
        internal TracePhase Phase;
        internal ImeSnapshot State;
    }

    internal sealed class MetadataTrace
    {
        private readonly TraceEntry[] entries;
        private int count;
        private long sequence, dropped;
        private bool capturing;
        internal MetadataTrace(int capacity)
        {
            if (capacity <= 0) throw new ArgumentOutOfRangeException("capacity");
            entries = new TraceEntry[capacity];
        }
        internal int Count { get { return count; } }
        internal long Dropped { get { return dropped; } }
        internal void Observe(int caseIndex, TraceBoundary boundary, TracePhase phase, Func<ImeSnapshot> capture)
        {
            long current = ++sequence;
            if (capturing || count == entries.Length) { dropped++; return; }
            long timestamp = System.Diagnostics.Stopwatch.GetTimestamp();
            ImeSnapshot state;
            capturing = true;
            try { state = capture(); }
            catch (Exception) { state = new ImeSnapshot { Availability = ImeAvailability.QueryException }; }
            finally { capturing = false; }
            entries[count++] = new TraceEntry { Sequence = current, Timestamp = timestamp, CaseIndex = caseIndex, Boundary = boundary, Phase = phase, State = state };
        }
        internal TraceEntry[] Snapshot()
        {
            TraceEntry[] copy = new TraceEntry[count];
            Array.Copy(entries, copy, count);
            return copy;
        }
        internal void MarkFlushed() { Array.Clear(entries, 0, count); count = 0; dropped = 0; }
    }

    internal sealed class ObservedTextBox : TextBox
    {
        internal readonly MetadataTrace Trace = new MetadataTrace(512);
        internal int CaseIndex;
        private static bool RelevantKey(int key) { return key == 0x56 || (key >= 0x10 && key <= 0x12) || key == 0x5B || key == 0x5C || (key >= 0xA0 && key <= 0xA5); }
        private void Observe(IntPtr window, int message, int key, bool command, TraceBoundary boundary, TracePhase phase)
        {
            Trace.Observe(CaseIndex, boundary, phase, delegate {
                ImeSnapshot sample = IsHandleCreated && Handle == window ? TextBoxIme.Sample(window) : new ImeSnapshot { Availability = ImeAvailability.NoContext };
                sample.Message = message; sample.VirtualKey = key; sample.CommandKey = command;
                return sample;
            });
        }
        protected override void WndProc(ref Message message)
        {
            int msg = message.Msg;
            int key = (msg == 0x100 || msg == 0x101 || msg == 0x104 || msg == 0x105) ? (int)message.WParam.ToInt64() : 0;
            bool tracked = msg == 0x302 || msg == 7 || msg == 8 || RelevantKey(key);
            TraceBoundary boundary = msg == 0x302 ? TraceBoundary.Paste : msg == 7 ? TraceBoundary.FocusGained : msg == 8 ? TraceBoundary.FocusLost : TraceBoundary.Key;
            IntPtr window = message.HWnd;
            if (tracked) Observe(window, msg, key, false, boundary, TracePhase.Before);
            try { base.WndProc(ref message); }
            finally { if (tracked) Observe(window, msg, key, false, boundary, TracePhase.After); }
        }
        // WinForms may handle Ctrl+V during preprocessing without a visible WM_PASTE.
        protected override bool ProcessCmdKey(ref Message message, Keys keyData)
        {
            int key = (int)(keyData & Keys.KeyCode);
            bool tracked = RelevantKey(key);
            IntPtr window = message.HWnd;
            int msg = message.Msg;
            if (tracked) Observe(window, msg, key, true, TraceBoundary.Key, TracePhase.Before);
            try { return base.ProcessCmdKey(ref message, keyData); }
            finally { if (tracked) Observe(window, msg, key, true, TraceBoundary.Key, TracePhase.After); }
        }
    }

    internal static class TraceJson
    {
        private static void Field(StringBuilder row, string name, object value)
        {
            row.Append(',').Append(TextEncoding.Json(name)).Append(':');
            row.Append(value is string ? TextEncoding.Json((string)value) : value is bool ? ((bool)value ? "true" : "false") : Convert.ToString(value, CultureInfo.InvariantCulture));
        }
        internal static string Batch(string reason, MetadataTrace trace)
        {
            StringBuilder row = new StringBuilder("{\"type\":\"batch\"");
            Field(row, "flush", reason); Field(row, "utc", DateTimeOffset.UtcNow.ToString("o", CultureInfo.InvariantCulture));
            Field(row, "frequency", System.Diagnostics.Stopwatch.Frequency); Field(row, "count", trace.Count); Field(row, "dropped", trace.Dropped);
            return row.Append('}').ToString();
        }
        internal static string Entry(TraceEntry entry)
        {
            ImeSnapshot state = entry.State;
            StringBuilder row = new StringBuilder("{\"type\":\"sample\"");
            Field(row, "seq", entry.Sequence); Field(row, "ticks", entry.Timestamp); Field(row, "caseIndex", entry.CaseIndex);
            Field(row, "boundary", entry.Boundary.ToString()); Field(row, "phase", entry.Phase.ToString());
            Field(row, "message", state.Message); Field(row, "virtualKey", state.VirtualKey); Field(row, "commandKey", state.CommandKey);
            Field(row, "window", state.Window.ToInt64()); Field(row, "focus", state.Focus.ToInt64()); Field(row, "context", state.Context.ToInt64());
            Field(row, "threadId", state.ThreadId); Field(row, "availability", state.Availability.ToString());
            Field(row, "open", state.Open); Field(row, "conversion", state.Conversion); Field(row, "sentence", state.Sentence);
            Field(row, "modifiersAvailable", state.ModifiersAvailable); Field(row, "modifierMask", state.ModifierMask); Field(row, "releaseFailed", state.ReleaseFailed);
            return row.Append('}').ToString();
        }
    }

    internal sealed class TextBoxIme
    {
        internal static ImeSnapshot Sample(IntPtr window)
        {
            ImeSnapshot result = new ImeSnapshot { Window = window, Availability = ImeAvailability.QueryException };
            IntPtr context = IntPtr.Zero;
            try
            {
                result.ThreadId = GetCurrentThreadId(); result.Focus = GetFocus();
                byte[] keys = new byte[256];
                result.ModifiersAvailable = GetKeyboardState(keys);
                if (result.ModifiersAvailable)
                    result.ModifierMask = (Down(keys, 0x11, 0xA2, 0xA3) ? 1 : 0) | (Down(keys, 0x10, 0xA0, 0xA1) ? 2 : 0)
                        | (Down(keys, 0x12, 0xA4, 0xA5) ? 4 : 0) | (Down(keys, 0x5B, 0x5C, 0x5C) ? 8 : 0) | ((keys[0x56] & 0x80) != 0 ? 16 : 0);
                context = ImmGetContext(window); result.Context = context;
                if (context == IntPtr.Zero) result.Availability = ImeAvailability.NoContext;
                else if (!ImmGetConversionStatus(context, out result.Conversion, out result.Sentence)) result.Availability = ImeAvailability.QueryFailed;
                else { result.Open = ImmGetOpenStatus(context); result.Availability = ImeAvailability.Available; }
            }
            catch (Exception) { result.Availability = ImeAvailability.QueryException; }
            finally
            {
                if (context != IntPtr.Zero)
                {
                    try { result.ReleaseFailed = !ImmReleaseContext(window, context); }
                    catch (Exception) { result.ReleaseFailed = true; result.Availability = ImeAvailability.QueryException; }
                }
            }
            return result;
        }
        private static bool Down(byte[] keys, int generic, int left, int right) { return ((keys[generic] | keys[left] | keys[right]) & 0x80) != 0; }
        [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
        [DllImport("user32.dll")] private static extern IntPtr GetFocus();
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetKeyboardState([Out] byte[] state);

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
        private readonly ObservedTextBox input = new ObservedTextBox();
        private readonly Label heading = new Label();
        private readonly Label sample = new Label();
        private readonly CanonicalView escaped = new CanonicalView();
        private bool traceWriteFailed;
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
            float[] heights = new float[] { 34, 44, 115, 168, 26, 140, 44, 58, 60, 36 };
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
            escaped.Font = new Font("Consolas", 18F);
            escaped.Dock = DockStyle.Fill;
            layout.Controls.Add(escaped, 0, 3);
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
            AddLabel(layout, new Label { Text = "Escape closes this fixture only. Auto-close: 15 minutes from launch (not reset).\r\nText evidence: Check only; metadata trace: Check/close. No native baseline." }, 9);

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
            escaped.Payload = item.Expected;
            escaped.Invalidate();
            input.CaseIndex = caseIndex;
            input.ReadOnly = false;
            input.Clear();
            check.Enabled = clear.Enabled = enableIme.Enabled = true;
            next.Enabled = false;
            result.Text = "WAITING - paste once, then Check";
            result.BackColor = Color.FromArgb(238, 242, 246);
            result.ForeColor = Color.FromArgb(30, 45, 60);
            input.Focus();
        }

        private void FlushTrace(string reason)
        {
            if (traceWriteFailed || (input.Trace.Count == 0 && input.Trace.Dropped == 0)) return;
            try
            {
                StringBuilder rows = new StringBuilder(TraceJson.Batch(reason, input.Trace)).Append('\n');
                foreach (TraceEntry entry in input.Trace.Snapshot()) rows.Append(TraceJson.Entry(entry)).Append('\n');
                File.AppendAllText(evidenceFile + ".ime-trace.jsonl", rows.ToString(), new UTF8Encoding(false, true));
                input.Trace.MarkFlushed();
            }
            catch (Exception error)
            {
                traceWriteFailed = true; // No retry; text acceptance and dispatch remain independent.
                Console.Error.WriteLine("IME trace unavailable: " + error.GetType().Name);
            }
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
            FlushTrace("Check");
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
            FlushTrace("Close");
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
