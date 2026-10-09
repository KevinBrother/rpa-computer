// Pure policy regressions. CC executes; no controls, OS input or normalization of actual.
using System;
using System.Collections.Generic;
using System.Collections;
using System.Web.Script.Serialization;
namespace AcceptanceFixture {
    static class NativeTextSelfTest {
        public static List<SelfTestResult> Run() {
            var results = new List<SelfTestResult>();
            Action<string,bool> add = (name,ok) => results.Add(new SelfTestResult("native-text-" + name,ok,"pure Windows expected policy; not GUI evidence"));
            FixtureCase known = CaseCatalog.KnownInputCases[8];
            string canonical = "第一行\n第二行\n第三行";
            string native = "第一行\r\n第二行\r\n第三行";
            add("catalog-payload-unchanged",known.Payload == native);
            add("catalog-canonical-unchanged",known.ExpectedText == canonical);
            add("old-canonical-rejects-native",!CaseContract.Utf16ExactMatch(native,known.ExpectedText));
            add("old-canonical-still-accepts-LF",CaseContract.Utf16ExactMatch(canonical,known.ExpectedText));
            string expected = NativeTextPolicy.Expected(SuiteKind.KnownInput,known);
            add("native-expected-explicit",expected == native);
            add("native-exact",CaseContract.Utf16ExactMatch(native,expected));
            add("native-rejects-canonical-LF",!CaseContract.Utf16ExactMatch(canonical,expected));
            string[] bad = { "第一行\r\n第二行\n第三行", "第一行\n第二行\r\n第三行",
                "第一行\r第二行\r第三行", "第一行\r\r\n第二行\r\n第三行",
                "第一行\r\n\n第二行\r\n第三行", native + "\r", native + "\n", native + "\r\n",
                native + " ", " " + native, "第一行\r\n第二行\r\n\r\n第三行" };
            for (int i=0;i<bad.Length;i++)
                add("reject-exact-newline-deviation-"+i,!CaseContract.Utf16ExactMatch(bad[i],expected));
            add("null-actual-rejected",!CaseContract.Utf16ExactMatch(null,expected));
            add("only-known-suite-overrides",NativeTextPolicy.Expected(SuiteKind.Baseline,known)==canonical);
            var different = new FixtureCase("other",native);
            add("no-other-ID-generalization",NativeTextPolicy.Expected(SuiteKind.KnownInput,different)==canonical);
            bool changedCatalogRejected=false;
            try { NativeTextPolicy.Expected(SuiteKind.KnownInput,new FixtureCase("known-09","a\r\nb\nc")); }
            catch (InvalidOperationException) { changedCatalogRejected=true; }
            add("catalog-drift-fails-closed",changedCatalogRejected);
            bool preserved=true;
            foreach(var suite in SuiteNames.Ordered) foreach(var c in CaseCatalog.CasesFor(suite))
                if (!(suite==SuiteKind.KnownInput && c.Id=="known-09"))
                    preserved &= CaseContract.Utf16ExactMatch(c.ExpectedText,NativeTextPolicy.Expected(suite,c));
            add("all-other-text-expected-unchanged",preserved);
            add("raw-native-13-units",expected.Length==13);
            add("canonical-11-units",known.ExpectedText.Length==11);
            add("native-hex-keeps-CR",CaseContract.Utf16Hex(expected)=="7B2C 4E00 884C 000D 000A 7B2C 4E8C 884C 000D 000A 7B2C 4E09 884C");
            string oldManifest=GestureExporter.ManifestJson();
            add("legacy63-has-canonical-known09",LegacyKnown09Matches(oldManifest));
            add("legacy63-no-native-policy-overlay",!oldManifest.Contains(NativeTextPolicy.Version));
            add("manifest-version",NativeTextPolicy.ManifestJson().Contains(NativeTextPolicy.Version));
            add("manifest-not-legacy63",NativeTextPolicy.ManifestJson().Contains("\"schema\":\"windows-native-text-policy-v1\""));
            add("export-path",NativeTextPolicy.ParseExport(new[]{"--export-native-text-policy","fresh.json"})=="fresh.json");
            foreach(var argv in new[]{new[]{"--export-native-text-policy"},new[]{"--export-native-text-policy"," "},
                new[]{"--export-native-text-policy","--self-test"},new[]{"--export-native-text-policy","x","--self-test"}}) {
                bool rejected=false;try{NativeTextPolicy.ParseExport(argv);}catch(ArgumentException){rejected=true;}
                add("export-strict-"+results.Count,rejected);
            }
            // Appended so all pre-existing check names/order (including export-strict IDs) remain intact.
            AddManifestRegressions(add);
            return results;
        }

        // Fixed independent JSON test data: no production exporter/escape function builds these expectations.
        private const string PayloadField = "\"task_payload\":\"第一行\\r\\n第二行\\r\\n第三行\"";
        private const string ExpectedField = "\"expected_text\":\"第一行\\n第二行\\n第三行\"";
        private const string CanonicalRecord = "{\"id\":\"known-09\"," + PayloadField + "," + ExpectedField + "}";

        private static string TestManifest(string records, string suite = "known-input") {
            return "{\"suites\":{\"" + suite + "\":{\"cases\":[" + records + "]}}}";
        }

        private static void AddManifestRegressions(Action<string,bool> add) {
            add("legacy63-json-short-escapes",LegacyKnown09Matches(TestManifest(CanonicalRecord)));
            const string unicodeRecord = "{\"id\":\"known-09\",\"task_payload\":\"第一行\\u000d\\u000a第二行\\u000d\\u000a第三行\",\"expected_text\":\"第一行\\u000a第二行\\u000a第三行\"}";
            add("legacy63-json-unicode-escapes",LegacyKnown09Matches(TestManifest(unicodeRecord)));
            string wrongExpected = CanonicalRecord.Replace(ExpectedField,"\"expected_text\":\"第一行\\r\\n第二行\\r\\n第三行\"");
            add("legacy63-rejects-native-expected",!LegacyKnown09Matches(TestManifest(wrongExpected)));
            string[] badExpected = {
                "第一行\\r\\n第二行\\n第三行", "第一行\\n第二行\\r\\n第三行",
                "第一行\\r第二行\\r第三行", "第一行\\n\\n第二行\\n第三行",
                "第一行\\n第二行\\n第三行\\r", "第一行\\n第二行\\n第三行\\n"
            };
            for(int i=0;i<badExpected.Length;i++)
                add("legacy63-rejects-expected-newline-"+i,!LegacyKnown09Matches(TestManifest(
                    CanonicalRecord.Replace(ExpectedField,"\"expected_text\":\""+badExpected[i]+"\""))));
            string[] badPayload = {
                "第一行\\n第二行\\n第三行", "第一行\\r\\n第二行\\n第三行",
                "第一行\\n第二行\\r\\n第三行", "第一行\\r\\r\\n第二行\\r\\n第三行",
                "第一行\\r\\n第二行\\r\\n第三行\\n"
            };
            for(int i=0;i<badPayload.Length;i++)
                add("legacy63-rejects-payload-newline-"+i,!LegacyKnown09Matches(TestManifest(
                    CanonicalRecord.Replace(PayloadField,"\"task_payload\":\""+badPayload[i]+"\""))));
            string other = CanonicalRecord.Replace("known-09","known-08");
            add("legacy63-rejects-wrong-ID",!LegacyKnown09Matches(TestManifest(other)));
            add("legacy63-rejects-wrong-suite",!LegacyKnown09Matches(TestManifest(CanonicalRecord,"baseline")));
            add("legacy63-rejects-missing-case",!LegacyKnown09Matches(TestManifest("")));
            add("legacy63-rejects-duplicate-case",!LegacyKnown09Matches(TestManifest(CanonicalRecord+","+CanonicalRecord)));
            add("legacy63-rejects-good-plus-bad-duplicate",!LegacyKnown09Matches(TestManifest(CanonicalRecord+","+wrongExpected)));
            add("legacy63-other-case-cannot-cover-bad-known09",!LegacyKnown09Matches(TestManifest(other+","+wrongExpected)));
            add("legacy63-rejects-missing-expected",!LegacyKnown09Matches(TestManifest(CanonicalRecord.Replace(","+ExpectedField,""))));
            add("legacy63-rejects-nonstring-expected",!LegacyKnown09Matches(TestManifest(CanonicalRecord.Replace(ExpectedField,"\"expected_text\":11"))));
            add("legacy63-rejects-nonstring-payload",!LegacyKnown09Matches(TestManifest(CanonicalRecord.Replace(PayloadField,"\"task_payload\":13"))));
            add("legacy63-rejects-malformed-json",!LegacyKnown09Matches("{\"suites\":"));
        }

        // Use the existing Framework parser, not a new parser or an exporter substring comparison.
        // This checks only the scoped known-09 record; the unchanged parity tool checks the whole catalog.
        private static object Member(object value,string key) {
            var map=value as IDictionary<string,object>;
            object result;
            return map!=null && map.TryGetValue(key,out result) ? result : null;
        }

        private static bool LegacyKnown09Matches(string json) {
            object root;
            try {
                root=new JavaScriptSerializer { MaxJsonLength=2*1024*1024, RecursionLimit=32 }.DeserializeObject(json);
            } catch (ArgumentException) { return false; }
              catch (InvalidOperationException) { return false; }
            var cases=Member(Member(Member(root,"suites"),"known-input"),"cases") as IList;
            if(cases==null) return false;
            int count=0;
            bool fieldsMatch=false;
            foreach(object item in cases) {
                string id=Member(item,"id") as string;
                if(!string.Equals(id,"known-09",StringComparison.Ordinal)) continue;
                count++;
                // Literal reviewed UTF-16 values, independent of catalog/policy/JSON-escape generators.
                fieldsMatch=string.Equals(Member(item,"task_payload") as string,"第一行\r\n第二行\r\n第三行",StringComparison.Ordinal)
                    && string.Equals(Member(item,"expected_text") as string,"第一行\n第二行\n第三行",StringComparison.Ordinal);
            }
            return count==1 && fieldsMatch;
        }
    }
}
