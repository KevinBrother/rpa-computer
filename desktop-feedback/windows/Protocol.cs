using System;
using System.Collections.Generic;
using System.Globalization;

namespace DesktopFeedback {
    internal sealed class Session {
        internal readonly string Id;
        internal readonly ulong Generation;
        internal Session(string id, ulong generation) { Id = id; Generation = generation; }
        internal bool Same(Session other) { return other != null && Id == other.Id && Generation == other.Generation; }
    }
    internal sealed class Surface {
        internal readonly string Id, Version;
        internal readonly double X, Y, Width, Height;
        internal Surface(string id, string version, double x, double y, double width, double height) {
            Id = id; Version = version; X = x; Y = y; Width = width; Height = height;
        }
    }
    internal sealed class Pointer {
        internal readonly double X, Y;
        internal readonly string Kind;
        internal Pointer(double x, double y, string kind) { X = x; Y = y; Kind = kind; }
    }
    internal sealed class Snapshot {
        internal readonly ulong Sequence;
        internal readonly Session Session;
        internal readonly string Phase, Cleanup;
        internal readonly Surface Surface;
        internal readonly Pointer Pointer;
        Snapshot(ulong sequence, Session session, string phase, string cleanup, Surface surface, Pointer pointer) {
            Sequence = sequence; Session = session; Phase = phase; Cleanup = cleanup; Surface = surface; Pointer = pointer;
        }
        internal static Snapshot Parse(byte[] bytes) {
            Dictionary<string, JsonValue> o = Fields(JsonValue.Parse(bytes), "type", "version", "sequence", "session", "phase", "cleanup", "surface", "pointer");
            if (o["type"].String() != "snapshot" || o["version"].UInt() != 1) throw new WireFailure("unsupported_protocol");
            string phase = o["phase"].String(), cleanup = o["cleanup"].String();
            if (Array.IndexOf(new [] {"starting", "idle", "observing", "executing", "paused", "stopping", "faulted", "closed"}, phase) < 0 || Array.IndexOf(new [] {"not_needed", "pending", "released", "failed", "unknown"}, cleanup) < 0) throw new WireFailure("invalid_state");
            Session session = null; Surface surface = null; Pointer pointer = null;
            if (!o["session"].IsNull) {
                Dictionary<string, JsonValue> s = Fields(o["session"], "id", "generation");
                session = new Session(Token(s["id"], false), s["generation"].UInt());
            }
            if (!o["surface"].IsNull) {
                Dictionary<string, JsonValue> s = Fields(o["surface"], "id", "version", "x", "y", "width", "height");
                double x = s["x"].Finite(), y = s["y"].Finite(), w = s["width"].Finite(), h = s["height"].Finite();
                if (w <= 0 || h <= 0 || Double.IsInfinity(x + w) || Double.IsInfinity(y + h)) throw new WireFailure("invalid_surface");
                surface = new Surface(Token(s["id"], false), Token(s["version"], true), x, y, w, h);
            }
            if (!o["pointer"].IsNull) {
                if (surface == null) throw new WireFailure("invalid_pointer");
                Dictionary<string, JsonValue> p = Fields(o["pointer"], "x", "y", "kind");
                string kind = p["kind"].String();
                if (Array.IndexOf(new [] {"move", "click", "drag"}, kind) < 0) throw new WireFailure("invalid_pointer");
                pointer = new Pointer(p["x"].Finite(), p["y"].Finite(), kind);
            }
            return new Snapshot(o["sequence"].UInt(), session, phase, cleanup, surface, pointer);
        }
        static Dictionary<string, JsonValue> Fields(JsonValue value, params string[] keys) {
            Dictionary<string, JsonValue> map = value.Object();
            if (map.Count != keys.Length) throw new WireFailure("invalid_fields");
            foreach (string key in keys) if (!map.ContainsKey(key)) throw new WireFailure("invalid_fields");
            return map;
        }
        static string Token(JsonValue value, bool allowComma) {
            string s = value.String();
            // Allowed characters are ASCII, so accepted character and byte counts agree.
            if (s.Length == 0 || s.Length > 128) throw new WireFailure("invalid_identifier");
            foreach (char c in s) {
                bool alphanumeric = (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9');
                if (!alphanumeric && c != '_' && c != '.' && c != ':' && c != '-' && !(allowComma && c == ','))
                    throw new WireFailure("invalid_identifier");
            }
            return s;
        }
    }
    internal static class WireOutput {
        internal static string Stop(Session session) {
            return "{\"type\":\"stop\",\"version\":1,\"session\":{\"id\":" + JsonValue.Quote(session.Id) + ",\"generation\":" + session.Generation.ToString(CultureInfo.InvariantCulture) + "}}";
        }
        internal static string Ready(string exclusion) { return "{\"type\":\"ready\",\"version\":1,\"capture_exclusion\":\"" + exclusion + "\",\"pointer_feedback\":true}"; }
        internal const string Heartbeat = "{\"type\":\"heartbeat\",\"version\":1}";
        internal static string Error(string code) {
            foreach (char c in code) if (!(c >= 'a' && c <= 'z') && !(c >= '0' && c <= '9') && c != '_') { code = "renderer_error"; break; }
            return "{\"type\":\"error\",\"version\":1,\"code\":\"" + code + "\"}";
        }
    }
}
