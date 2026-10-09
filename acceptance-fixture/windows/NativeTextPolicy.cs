// Default Windows native expected policy. Does NOT change CaseContract or catalog.
// Included by build-windows.ps1; no opt-in flag or canonical runtime fallback.
using System;
namespace AcceptanceFixture {
    static class NativeTextPolicy {
        public const string Version = "windows-winforms-known09-crlf-v1";
        public const string Control = "System.Windows.Forms.TextBox.Multiline";
        public const string KnownPayload = "第一行\r\n第二行\r\n第三行";
        public const string KnownCanonical = "第一行\n第二行\n第三行";

        public static bool Applies(SuiteKind suite,FixtureCase c) {
            return suite==SuiteKind.KnownInput && c!=null && c.Id=="known-09";
        }
        public static string Expected(SuiteKind suite,FixtureCase c) {
            if (c==null) return "";
            if (!Applies(suite,c)) return c.ExpectedText;
            // No general CR/LF replacement: only the reviewed, frozen case is supported.
            // If the catalog changes, require a new policy review instead of silently adapting.
            if (!CaseContract.Utf16ExactMatch(c.Payload,KnownPayload) ||
                !CaseContract.Utf16ExactMatch(c.ExpectedText,KnownCanonical))
                throw new InvalidOperationException("native text policy catalog drift; review a new policy version");
            return KnownPayload;
        }
        public static string ParseExport(string[] args) {
            if (args.Length!=2 || args[0]!="--export-native-text-policy" ||
                string.IsNullOrWhiteSpace(args[1]) || args[1].StartsWith("--",StringComparison.Ordinal))
                throw new ArgumentException("use --export-native-text-policy PATH alone; output must be new");
            return args[1];
        }
        public static string ManifestJson() {
            var c=CaseCatalog.KnownInputCases[8];
            if(c.Id!="known-09") throw new InvalidOperationException("known-09 catalog position drift");
            string expected=Expected(SuiteKind.KnownInput,c);
            Func<string,string> j=GestureExporter.JsonString;
            return "{\"schema\":\"windows-native-text-policy-v1\",\"platform\":\"windows\",\"policy_version\":"+j(Version)+
                ",\"native_control\":"+j(Control)+",\"scope\":\"known-input/known-09 only\",\"case_id\":"+j(c.Id)+
                ",\"suite\":\"known-input\",\"task_payload\":"+j(c.Payload)+
                ",\"canonical_expected_text\":"+j(c.ExpectedText)+",\"canonical_expected_utf16_hex\":"+j(CaseContract.Utf16Hex(c.ExpectedText))+
                ",\"native_expected_text\":"+j(expected)+",\"native_expected_utf16_hex\":"+j(CaseContract.Utf16Hex(expected))+
                ",\"comparison\":\"strict-utf16-code-units\",\"actual_normalized\":false,\"legacy63_changed\":false,\"gui_verified\":false}\n";
        }
    }
}
