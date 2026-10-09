// Stage A only: pure regression for the required focus suite argument.
// Deliberately calls the current parser; no focus implementation/stub or GUI.
// CC must execute RED before Stage B changes production behavior.
using System;
using System.Collections.Generic;

namespace AcceptanceFixture
{
    static class FocusSelfTest
    {
        public static List<SelfTestResult> Run()
        {
            string observedSuite = null;
            string detail;
            try
            {
                var parsed = BasicArguments.Parse(new[] { "--suite", "focus" });
                observedSuite = parsed.Suite;
                detail = "required --suite focus; parser returned suite: " + observedSuite;
            }
            catch (ArgumentException ex)
            {
                detail = "required --suite focus; parser rejected with ArgumentException: " + ex.Message;
            }

            return new List<SelfTestResult>
            {
                new SelfTestResult("focus-args-accepts-focus-suite",
                    string.Equals(observedSuite, "focus", StringComparison.Ordinal), detail)
            };
        }
    }
}
