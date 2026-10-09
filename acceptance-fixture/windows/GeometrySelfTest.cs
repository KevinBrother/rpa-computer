// Stage A only: executable requirements against the CURRENT parser, no product stubs.
// CC must observe native RED before Stage B implements geometry parsing/export/GUI.
// These tests parse strings only; the export-looking path is never opened or written.
using System;
using System.Collections.Generic;

namespace AcceptanceFixture
{
    static class GeometrySelfTest
    {
        public static List<SelfTestResult> Run()
        {
            var results = new List<SelfTestResult>();
            string observedSuite = null;
            string suiteDetail;
            try
            {
                var parsed = BasicArguments.Parse(new[] { "--suite", "geometry" });
                observedSuite = parsed.Suite;
                suiteDetail = "required --suite geometry; parser returned suite: " + observedSuite;
            }
            catch (ArgumentException ex)
            {
                suiteDetail = "required --suite geometry; parser rejected with ArgumentException: " + ex.Message;
            }
            results.Add(new SelfTestResult("geometry-args-accepts-geometry-suite",
                string.Equals(observedSuite, "geometry", StringComparison.Ordinal), suiteDetail));

            BasicArguments exportParsed = null;
            string exportDetail;
            try
            {
                exportParsed = BasicArguments.Parse(new[] { "--export-geometry-cases", "geometry-stage-a-never-written.json" });
                exportDetail = "required independent --export-geometry-cases PATH; parser accepted; no file operation; existing console modes must stay unset";
            }
            catch (ArgumentException ex)
            {
                exportDetail = "required independent --export-geometry-cases PATH; parser rejected with ArgumentException: " + ex.Message;
            }
            results.Add(new SelfTestResult("geometry-args-accepts-independent-geometry-export",
                exportParsed != null && !exportParsed.SelfTest && exportParsed.ExportPath == null
                    && exportParsed.PlatformExportPath == null && exportParsed.FocusExportPath == null,
                exportDetail));
            return results;
        }
    }
}
