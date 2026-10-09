using System;
using System.Text;
using System.Collections.Generic;
using System.Globalization;

namespace DesktopFeedback {
    internal sealed class WireFailure : Exception {
        internal readonly string Code;
        internal WireFailure(string code) : base(code) { Code = code; }
    }
    internal sealed class JsonValue {
        internal enum ValueKind { Object, String, Number, Null }
        readonly ValueKind kind;
        readonly string raw;
        readonly Dictionary<string, JsonValue> fields;
        JsonValue(ValueKind kind, string raw, Dictionary<string, JsonValue> fields) { this.kind = kind; this.raw = raw; this.fields = fields; }
        internal bool IsNull { get { return kind == ValueKind.Null; } }
        internal Dictionary<string, JsonValue> Object() {
            if (kind != ValueKind.Object) throw new WireFailure("invalid_fields"); return fields;
        }
        internal string String() {
            if (kind != ValueKind.String) throw new WireFailure("invalid_fields"); return raw;
        }
        internal ulong UInt() {
            ulong n;
            if (kind != ValueKind.Number || raw.Length == 0) throw new WireFailure("invalid_unsigned_integer");
            foreach (char c in raw) if (c < '0' || c > '9') throw new WireFailure("invalid_unsigned_integer");
            if (!UInt64.TryParse(raw, NumberStyles.None, CultureInfo.InvariantCulture, out n)) throw new WireFailure("invalid_unsigned_integer");
            return n;
        }
        internal double Finite() {
            double n;
            if (kind != ValueKind.Number || !Double.TryParse(raw, NumberStyles.Float, CultureInfo.InvariantCulture, out n) || Double.IsInfinity(n) || Double.IsNaN(n)) throw new WireFailure("invalid_coordinate");
            return n;
        }
        internal static JsonValue Parse(byte[] bytes) {
            if (bytes.Length > 16383) throw new WireFailure("frame_too_large");
            string text;
            try { text = new UTF8Encoding(false, true).GetString(bytes); }
            catch (DecoderFallbackException) { throw new WireFailure("invalid_utf8"); }
            Parser parser = new Parser(text);
            JsonValue value = parser.Value(0); parser.Space();
            if (!parser.End) throw new WireFailure("invalid_json"); return value;
        }
        internal static string Quote(string text) {
            StringBuilder result = new StringBuilder("\"");
            foreach (char c in text) {
                if (c == '"') result.Append("\\\"");
                else if (c == '\\') result.Append("\\\\");
                else if (c < 32) result.Append("\\u" + ((int)c).ToString("x4", CultureInfo.InvariantCulture));
                else result.Append(c);
            }
            return result.Append('"').ToString();
        }
        sealed class Parser {
            readonly string text;
            int index;
            internal Parser(string text) { this.text = text; }
            internal bool End { get { return index == text.Length; } }
            internal void Space() { while (index < text.Length && (text[index] == ' ' || text[index] == '\t' || text[index] == '\r' || text[index] == '\n')) index++; }
            bool Consume(char c) { Space(); if (index < text.Length && text[index] == c) { index++; return true; } return false; }
            internal JsonValue Value(int depth) {
                Space(); if (depth > 8 || End) throw new WireFailure("invalid_json");
                if (text[index] == '{') {
                    index++; Dictionary<string, JsonValue> map = new Dictionary<string, JsonValue>(StringComparer.Ordinal);
                    if (Consume('}')) return new JsonValue(ValueKind.Object, null, map);
                    do {
                        Space(); string key = ParseString();
                        if (map.ContainsKey(key) || map.Count >= 32 || !Consume(':')) throw new WireFailure("invalid_fields");
                        map.Add(key, Value(depth + 1));
                        if (Consume('}')) return new JsonValue(ValueKind.Object, null, map);
                    } while (Consume(','));
                    throw new WireFailure("invalid_json");
                }
                if (text[index] == '"') return new JsonValue(ValueKind.String, ParseString(), null);
                if (index + 4 <= text.Length && text.Substring(index, 4) == "null") { index += 4; return new JsonValue(ValueKind.Null, null, null); }
                // Arrays and booleans are not valid in inbound v1 snapshots.
                int start = index;
                if (text[index] == '-') index++;
                if (End) throw new WireFailure("invalid_json");
                if (text[index] == '0') index++;
                else { if (text[index] < '1' || text[index] > '9') throw new WireFailure("invalid_json"); Digits(); }
                if (!End && text[index] == '.') { index++; int p = index; Digits(); if (p == index) throw new WireFailure("invalid_json"); }
                if (!End && (text[index] == 'e' || text[index] == 'E')) {
                    index++; if (!End && (text[index] == '+' || text[index] == '-')) index++;
                    int p = index; Digits(); if (p == index) throw new WireFailure("invalid_json");
                }
                return new JsonValue(ValueKind.Number, text.Substring(start, index - start), null);
            }
            void Digits() { while (!End && text[index] >= '0' && text[index] <= '9') index++; }
            string ParseString() {
                if (End || text[index++] != '"') throw new WireFailure("invalid_json");
                StringBuilder result = new StringBuilder();
                while (!End) {
                    char c = text[index++];
                    if (c == '"') {
                        string value = result.ToString();
                        for (int i = 0; i < value.Length; i++) {
                            if (Char.IsHighSurrogate(value[i])) { if (i + 1 >= value.Length || !Char.IsLowSurrogate(value[++i])) throw new WireFailure("invalid_json_string"); }
                            else if (Char.IsLowSurrogate(value[i])) throw new WireFailure("invalid_json_string");
                        }
                        return value;
                    }
                    if (c < 32) throw new WireFailure("invalid_json_string");
                    if (c == '\\') {
                        if (End) throw new WireFailure("invalid_json_string");
                        char escape = text[index++];
                        switch (escape) {
                            case '"': c = '"'; break; case '\\': c = '\\'; break; case '/': c = '/'; break;
                            case 'b': c = '\b'; break; case 'f': c = '\f'; break; case 'n': c = '\n'; break;
                            case 'r': c = '\r'; break; case 't': c = '\t'; break;
                            case 'u':
                                ushort code;
                                if (index + 4 > text.Length || !UInt16.TryParse(text.Substring(index, 4), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out code)) throw new WireFailure("invalid_json_string");
                                index += 4; c = (char)code; break;
                            default: throw new WireFailure("invalid_json_string");
                        }
                    }
                    result.Append(c);
                }
                throw new WireFailure("invalid_json_string");
            }
        }
    }
}
