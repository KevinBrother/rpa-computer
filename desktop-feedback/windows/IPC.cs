using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Collections.Generic;

namespace DesktopFeedback {
    internal sealed class LineResult {
        internal readonly byte[] Bytes;
        internal readonly string Failure;
        internal LineResult(byte[] bytes, string failure) { Bytes = bytes; Failure = failure; }
    }
    internal sealed class BoundedLines {
        readonly byte[] buffer = new byte[16383];
        int count;
        bool overflow;
        internal List<LineResult> Feed(byte[] chunk, int length) {
            List<LineResult> result = new List<LineResult>();
            for (int i = 0; i < length; i++) {
                byte b = chunk[i];
                if (b == 10) {
                    if (!overflow) {
                        int size = count > 0 && buffer[count - 1] == 13 ? count - 1 : count;
                        byte[] line = new byte[size]; Array.Copy(buffer, line, size); result.Add(new LineResult(line, null));
                    }
                    count = 0; overflow = false;
                } else if (!overflow) {
                    if (count == buffer.Length) { overflow = true; count = 0; result.Add(new LineResult(null, "frame_too_large")); }
                    else buffer[count++] = b;
                }
            }
            return result;
        }
        internal LineResult Finish() {
            string failure = overflow ? "frame_too_large" : count == 0 ? null : "truncated_frame";
            count = 0; overflow = false; return failure == null ? null : new LineResult(null, failure);
        }
    }
    internal sealed class LatestMailbox {
        Snapshot pending;
        ulong high;
        bool seen;
        internal void Offer(Snapshot s) { if (seen && s.Sequence <= high) return; seen = true; high = s.Sequence; pending = s; }
        internal Snapshot Take() { Snapshot s = pending; pending = null; return s; }
    }
    internal sealed class InputPump {
        readonly object gate = new object();
        readonly LatestMailbox mailbox = new LatestMailbox();
        string failure;
        bool eof;
        internal void Start() { Thread thread = new Thread(Read); thread.IsBackground = true; thread.Name = "feedback-stdin"; thread.Start(); }
        void Read() {
            try {
                Stream input = Console.OpenStandardInput(); BoundedLines lines = new BoundedLines(); byte[] chunk = new byte[4096];
                while (true) {
                    int count = input.Read(chunk, 0, chunk.Length);
                    if (count == 0) {
                        LineResult final = lines.Finish();
                        lock (gate) { if (final != null) failure = final.Failure; else eof = true; }
                        return;
                    }
                    foreach (LineResult line in lines.Feed(chunk, count)) {
                        if (line.Failure != null) { lock (gate) failure = line.Failure; return; }
                        Snapshot s = Snapshot.Parse(line.Bytes); lock (gate) mailbox.Offer(s);
                    }
                }
            } catch (WireFailure e) { lock (gate) failure = e.Code; }
            catch (Exception) { lock (gate) failure = "stdin_read_failed"; }
        }
        internal Snapshot Poll(out string error, out bool end) { lock (gate) { error = failure; end = eof; return mailbox.Take(); } }
    }
    internal sealed class OutputPump {
        readonly object gate = new object();
        readonly Queue<byte[]> queue = new Queue<byte[]>();
        bool closing;
        int exitCode;
        internal OutputPump() { Thread worker = new Thread(Write); worker.IsBackground = true; worker.Name = "feedback-stdout"; worker.Start(); }
        internal void Send(string frame) {
            byte[] bytes = new UTF8Encoding(false, true).GetBytes(frame + "\n");
            if (bytes.Length > 16384) { Console.Error.WriteLine("outbound_frame_too_large"); Environment.Exit(74); return; }
            lock (gate) {
                if (closing) return;
                if (queue.Count >= 32) { Console.Error.WriteLine("stdout_queue_overflow"); Environment.Exit(74); return; }
                queue.Enqueue(bytes); Monitor.Pulse(gate);
            }
        }
        internal void Finish(int code) {
            lock (gate) { closing = true; exitCode = code; Monitor.Pulse(gate); }
            Thread watchdog = new Thread(delegate() { Thread.Sleep(300); Environment.Exit(code); });
            watchdog.IsBackground = true; watchdog.Start();
        }
        void Write() {
            try {
                Stream output = Console.OpenStandardOutput();
                while (true) {
                    byte[] frame;
                    lock (gate) {
                        while (queue.Count == 0 && !closing) Monitor.Wait(gate);
                        if (queue.Count == 0 && closing) { Environment.Exit(exitCode); return; }
                        frame = queue.Dequeue();
                    }
                    output.Write(frame, 0, frame.Length); output.Flush();
                }
            } catch (Exception) { Console.Error.WriteLine("stdout_write_failed"); Environment.Exit(74); }
        }
    }
}
