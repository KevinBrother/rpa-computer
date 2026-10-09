using System;
using System.Text;
using System.Collections.Generic;

namespace DesktopFeedback {
    // No WinForms, DPI, windows, timers, input or ready in this execution path.
    internal static class SelfTests {
        static int checks;
        static void Check(bool value, string name) {
            checks++; if (!value) throw new WireFailure("self_test_" + name);
        }
        static void Reject(string text) {
            checks++;
            try { Snapshot.Parse(Encoding.UTF8.GetBytes(text)); }
            catch (WireFailure) { return; }
            throw new WireFailure("self_test_accepted_invalid_frame");
        }
        // Stage A: named strict-v1 regressions; production parsing stays unchanged.
        static void RejectNamed(string text, string name) {
            checks++;
            try { Snapshot.Parse(Encoding.UTF8.GetBytes(text)); }
            catch (WireFailure) { return; }
            throw new WireFailure("self_test_" + name);
        }
        static Snapshot ParseStrictCase(string text) {
            return Snapshot.Parse(Encoding.UTF8.GetBytes(text));
        }
        static void StrictProtocolCases(string basic) {
            string[] fields = { "session_id", "surface_id", "surface_version" };
            string[] originals = { "\"id\":\"s\"", "\"id\":\"windows:1\"", "\"version\":\"v1\"" };
            string[] keys = { "id", "id", "version" };
            string[] invalidNames = { "empty", "129_bytes", "space", "slash", "unicode", "escaped_newline", "escaped_unicode" };
            string[] invalidTokens = { "", new string('a', 129), "a b", "a/b", "é", "a\\nb", "\\u00e9" };
            for (int field = 0; field < fields.Length; field++) {
                for (int i = 0; i < invalidTokens.Length; i++) {
                    RejectNamed(basic.Replace(originals[field], "\"" + keys[field] + "\":\"" + invalidTokens[i] + "\""),
                        "strict_" + fields[field] + "_reject_" + invalidNames[i]);
                }
                if (field < 2)
                    RejectNamed(basic.Replace(originals[field], "\"id\":\"a,b\""), "strict_" + fields[field] + "_reject_comma");
                string alphabet = field == 2 ? "AZaz09_.:,-" : "AZaz09_.:-";
                foreach (string token in new [] { "a", alphabet, new string('a', 128) }) {
                    Snapshot parsed = ParseStrictCase(basic.Replace(originals[field], "\"" + keys[field] + "\":\"" + token + "\""));
                    string actual = field == 0 ? parsed.Session.Id : field == 1 ? parsed.Surface.Id : parsed.Surface.Version;
                    Check(actual == token, "strict_" + fields[field] + "_accept_" + (token.Length == 128 ? "128_bytes" : token.Length == 1 ? "1_byte" : "alphabet"));
                }
            }
            const string surface = "\"surface\":{\"id\":\"windows:1\",\"version\":\"v1\",\"x\":-1920,\"y\":-200,\"width\":1920,\"height\":1080}";
            const string pointer = "\"pointer\":{\"x\":-1800,\"y\":-100,\"kind\":\"click\"}";
            RejectNamed(basic.Replace(surface, "\"surface\":null"), "strict_pointer_requires_surface");
            Snapshot absent = ParseStrictCase(basic.Replace(surface, "\"surface\":null").Replace(pointer, "\"pointer\":null"));
            Check(absent.Surface == null && absent.Pointer == null, "strict_null_surface_null_pointer");
            Check(ParseStrictCase(basic.Replace(pointer, "\"pointer\":null")).Pointer == null, "strict_surface_without_pointer");
            const string geometry = "d1:o-1920,-200:i1920x1080:c1920x1080:r0";
            Check(ParseStrictCase(basic.Replace("\"version\":\"v1\"", "\"version\":\"" + geometry + "\"")).Surface.Version == geometry,
                "strict_geometry_version_signed_origin_commas");

            string[] numberNames = { "leading_zero", "negative_leading_zero", "double_zero_fraction", "leading_zero_exponent", "plus", "missing_fraction", "missing_integer", "missing_exponent", "missing_signed_exponent" };
            string[] badNumbers = { "01", "-01", "00.1", "01e2", "+1", "1.", ".1", "1e", "1e+" };
            for (int i = 0; i < badNumbers.Length; i++) {
                RejectNamed(basic.Replace("\"sequence\":1", "\"sequence\":" + badNumbers[i]), "strict_sequence_json_" + numberNames[i]);
                RejectNamed(basic.Replace("\"x\":-1920", "\"x\":" + badNumbers[i]), "strict_coordinate_json_" + numberNames[i]);
            }
            string[] unsignedFields = { "sequence", "generation" };
            string[] unsignedOriginals = { "\"sequence\":1", "\"generation\":18446744073709551615" };
            string[] badUnsigned = { "18446744073709551616", "1.5", "1.0", "1e0", "-1", "-0" };
            string[] unsignedNames = { "overflow", "fraction", "decimal_integer", "exponent_integer", "negative", "negative_zero" };
            for (int field = 0; field < unsignedFields.Length; field++) {
                for (int i = 0; i < badUnsigned.Length; i++)
                    RejectNamed(basic.Replace(unsignedOriginals[field], "\"" + unsignedFields[field] + "\":" + badUnsigned[i]),
                        "strict_" + unsignedFields[field] + "_reject_" + unsignedNames[i]);
                string[] validUnsigned = { "0", "9007199254740993", "18446744073709551615" };
                ulong[] expected = { 0UL, 9007199254740993UL, UInt64.MaxValue };
                for (int i = 0; i < validUnsigned.Length; i++) {
                    Snapshot parsed = ParseStrictCase(basic.Replace(unsignedOriginals[field], "\"" + unsignedFields[field] + "\":" + validUnsigned[i]));
                    Check((field == 0 ? parsed.Sequence : parsed.Session.Generation) == expected[i],
                        "strict_" + unsignedFields[field] + "_exact_" + validUnsigned[i]);
                }
            }
            string[] validCoordinates = { "0", "-0", "-1800.5", "-1.8e3", "-1.8E+3", "-18000e-1" };
            double[] expectedCoordinates = { 0, 0, -1800.5, -1800, -1800, -1800 };
            for (int i = 0; i < validCoordinates.Length; i++) {
                Snapshot parsed = ParseStrictCase(basic.Replace("\"x\":-1920", "\"x\":" + validCoordinates[i]).Replace(pointer, "\"pointer\":null"));
                Check(parsed.Surface.X == expectedCoordinates[i], "strict_coordinate_valid_json_" + i);
            }
            Snapshot fractionalPointer = ParseStrictCase(basic.Replace("\"x\":-1800", "\"x\":-1.8005e3").Replace("\"y\":-100", "\"y\":-100.25"));
            Check(fractionalPointer.Pointer.X == -1800.5 && fractionalPointer.Pointer.Y == -100.25, "strict_pointer_fraction_exponent");
            RejectNamed(basic.Replace("\"x\":-1800", "\"x\":1e999"), "strict_pointer_nonfinite");
        }
        internal static void Run() {
            const string basic = "{\"type\":\"snapshot\",\"version\":1,\"sequence\":1,\"session\":{\"id\":\"s\",\"generation\":18446744073709551615},\"phase\":\"idle\",\"cleanup\":\"not_needed\",\"surface\":{\"id\":\"windows:1\",\"version\":\"v1\",\"x\":-1920,\"y\":-200,\"width\":1920,\"height\":1080},\"pointer\":{\"x\":-1800,\"y\":-100,\"kind\":\"click\"}}";
            Snapshot first = Snapshot.Parse(Encoding.UTF8.GetBytes(basic));
            Check(first.Session.Generation == UInt64.MaxValue, "generation_exact");
            Check(Snapshot.Parse(Encoding.UTF8.GetBytes(basic.Replace("\"sequence\":1", "\"sequence\":18446744073709551615"))).Sequence == UInt64.MaxValue, "sequence_exact");
            const string geometryVersion = "d1:o0,0:i1512x982:c3024x1964:r0";
            Snapshot actualGeometry = Snapshot.Parse(Encoding.UTF8.GetBytes(basic.Replace("\"version\":\"v1\"", "\"version\":\"" + geometryVersion + "\"")));
            Check(actualGeometry.Surface.Version == geometryVersion, "surface_version_commas_preserved");
            string[,] invalid = {
                {"\"version\":1", "\"version\":2"}, {"\"version\":1", "\"version\":1.0"},
                {"\"sequence\":1", "\"sequence\":-1"}, {"\"sequence\":1", "\"sequence\":true"},
                {"\"sequence\":1", "\"sequence\":1,\"sequen\\u0063e\":2"},
                {"\"generation\":18446744073709551615", "\"generation\":-1"},
                {"\"version\":\"v1\"", "\"version\":\"\""},
                {"\"sequence\":1", "\"sequence\":18446744073709551616"},
                {"\"sequence\":1", "\"sequence\":1,\"sequence\":2"},
                {"\"id\":\"s\"", "\"id\":\"\""}, {"\"phase\":\"idle\"", "\"phase\":\"running\""},
                {"\"cleanup\":\"not_needed\"", "\"cleanup\":\"safe\""},
                {"\"width\":1920", "\"width\":0"}, {"\"x\":-1920", "\"x\":1e999"},
                {"\"kind\":\"click\"", "\"kind\":\"key\""},
                {"\"version\":1", "\"version\":1,\"text\":\"secret\""},
                {"\"pointer\":{\"x\":-1800,\"y\":-100,\"kind\":\"click\"}", "\"pointer\":{}"}
            };
            for (int i = 0; i < invalid.GetLength(0); i++) Reject(basic.Replace(invalid[i, 0], invalid[i, 1]));
            Reject(basic + "{}"); Reject("[]"); Reject("{broken}");
            Reject(basic.Replace("\"session\":{\"id\":\"s\",\"generation\":18446744073709551615},", ""));
            Reject(basic.Replace("\"id\":\"s\"", "\"id\":\"\\ud800\""));
            checks++;
            try { Snapshot.Parse(new byte[] {0xff}); throw new Exception("self_test_invalid_utf8"); }
            catch (WireFailure) { }
            const string empty = "{\"type\":\"snapshot\",\"version\":1,\"sequence\":0,\"session\":null,\"phase\":\"starting\",\"cleanup\":\"not_needed\",\"surface\":null,\"pointer\":null}";
            FeedbackState state = new FeedbackState();
            Check(!state.CanStop && state.Status == "Waiting for Host", "honest_initial");
            Check(state.Apply(Snapshot.Parse(Encoding.UTF8.GetBytes(empty))), "first_zero");
            Check(state.Apply(first) && state.CanStop, "session_enables_stop");
            Check(!state.Apply(first), "duplicate_frame");
            Check(!state.Apply(Snapshot.Parse(Encoding.UTF8.GetBytes(empty))), "old_frame");
            Check(state.RequestStop().Generation == UInt64.MaxValue && !state.CanStop, "current_stop");
            Check(state.Status == "Stop requested · awaiting Host", "no_local_safe_claim");
            Check(state.RequestStop() == null, "deduplicate_stop");
            Snapshot regrant = Snapshot.Parse(Encoding.UTF8.GetBytes(basic.Replace("\"sequence\":1", "\"sequence\":2").Replace("18446744073709551615", "0")));
            Check(state.Apply(regrant) && state.CanStop, "new_generation_resets_stop");
            Check(state.RequestStop().Generation == 0, "new_stop_current_generation");
            FeedbackState noSession = new FeedbackState();
            noSession.Apply(Snapshot.Parse(Encoding.UTF8.GetBytes(empty.Replace("starting", "executing"))));
            Check(!noSession.CanStop && noSession.RequestStop() == null && noSession.Status == "Waiting for control session", "no_session_no_executing_claim");
            foreach (string cleanup in new [] {"released", "failed", "unknown", "pending", "not_needed"}) {
                FeedbackState fresh = new FeedbackState();
                fresh.Apply(Snapshot.Parse(Encoding.UTF8.GetBytes(basic.Replace("\"phase\":\"idle\"", "\"phase\":\"closed\"").Replace("\"cleanup\":\"not_needed\"", "\"cleanup\":\"" + cleanup + "\""))));
                Check(!fresh.CanStop, "closed_no_stop");
                Check(fresh.Status.Contains(cleanup == "not_needed" ? "no cleanup" : cleanup), "cleanup_truth");
            }
            Check(Geometry.Contains(first.Surface, -1800, -100), "negative_native_pixels");
            Check(!Geometry.Contains(first.Surface, 0, -100), "exclusive_surface_edge");
            Check(Geometry.AppKitY(-100, first.Surface, 800) == 700, "coordinate_contract");
            Style style = Style.Parse(new [] {"--accent", "#12aBcD", "--label", "AI 控制"});
            Check(style.Rgb == 0x12ABCD && style.Label == "AI 控制", "style_valid");
            foreach (string[] args in new [] {new [] {"--accent", "red"}, new [] {"--label", ""}, new [] {"--label", "a\nb"}, new [] {"--label", new string('x', 49)}, new [] {"--foo"}, new [] {"--accent"}, new [] {"--label", "x", "--label", "y"}, new [] {"--label", "a\u202eb"}}) {
                checks++;
                try { Style.Parse(args); }
                catch (WireFailure) { continue; }
                throw new WireFailure("self_test_invalid_style");
            }
            JsonValue output = JsonValue.Parse(Encoding.UTF8.GetBytes(WireOutput.Stop(new Session("q\"\\\n", UInt64.MaxValue))));
            Check(output.Object()["session"].Object()["id"].String() == "q\"\\\n", "output_escape");
            Check(!output.Object().ContainsKey("sequence"), "authorization_not_sequence");
            BoundedLines lines = new BoundedLines();
            Check(lines.Feed(new byte[16383], 16383).Count == 0, "bound");
            List<LineResult> over = lines.Feed(new byte[] {97}, 1);
            Check(over.Count == 1 && over[0].Failure == "frame_too_large", "bounded_overflow");
            lines.Feed(new byte[] {10}, 1);
            Check(Encoding.UTF8.GetString(lines.Feed(Encoding.UTF8.GetBytes("{}\r\n"), 4)[0].Bytes) == "{}", "crlf");
            lines.Feed(Encoding.UTF8.GetBytes("{}"), 2);
            Check(lines.Finish().Failure == "truncated_frame", "eof_partial");
            LatestMailbox mailbox = new LatestMailbox();
            mailbox.Offer(first); mailbox.Offer(Snapshot.Parse(Encoding.UTF8.GetBytes(empty)));
            Check(mailbox.Take().Sequence == 1, "mailbox_no_rollback");
            mailbox.Offer(Snapshot.Parse(Encoding.UTF8.GetBytes(empty)));
            Check(mailbox.Take() == null, "mailbox_watermark");
            StrictProtocolCases(basic);
            RendererDesktopSelfTests.Run(Check);
            RendererBoundarySelfTests.Run(Check);
            Console.Error.WriteLine("self-test: " + checks + " checks passed (pure; no GUI/ready/input)");
        }
    }
    // Pure supplied facts only. Never enumerate screens or initialize WinForms.
    internal static class RendererDesktopSelfTests {
        internal static void Run(Action<bool, string> check) {
            ScreenFacts left = new ScreenFacts("left", new System.Drawing.Rectangle(-1920, -200, 1920, 1080),
                new System.Drawing.Rectangle(-1920, -200, 1920, 1040));
            ScreenFacts right = new ScreenFacts("right", new System.Drawing.Rectangle(0, 0, 2560, 1440),
                new System.Drawing.Rectangle(0, 0, 2560, 1400));
            ScreenFacts[] screens = { left, right };
            Surface single = new Surface("opaque:left", "v1", -1920, -200, 1920, 1080);
            Pointer onLeft = new Pointer(-1800, -100, "click");
            ScreenResolution result = ScreenResolver.Resolve(single, onLeft, screens);
            check(result != null && result.Anchor == left, "renderer_single_exact_negative_origin");
            check(result != null && result.CanShowPointer, "renderer_single_pointer_inside");
            check(ScreenResolver.Resolve(new Surface("opaque", "v1", -1919.51, -199.51, 1919.51, 1079.51), null, screens) != null,
                "renderer_single_preserves_sub_half_pixel_tolerance");
            check(ScreenResolver.Resolve(new Surface("opaque", "v1", -1919.5, -200, 1920, 1080), null, screens) == null,
                "renderer_single_rejects_half_pixel_boundary");
            check(ScreenResolver.Resolve(new Surface("opaque", "v1", 5, 0, 2560, 1440), null, screens) == null,
                "renderer_single_unmatched_geometry");
            ScreenFacts duplicate = new ScreenFacts("duplicate", left.Bounds, left.WorkingArea);
            check(ScreenResolver.Resolve(single, null, new [] { left, duplicate }).Anchor == left,
                "renderer_single_preserves_first_match");
            check(!ScreenResolver.Resolve(single, new Pointer(0, -100, "move"), screens).CanShowPointer,
                "renderer_single_pointer_right_edge_excluded");
            check(!ScreenResolver.Resolve(single, null, screens).CanShowPointer, "renderer_single_null_pointer_hidden");
            // Other IDs must not gain desktop semantics solely from matching the bbox.
            check(ScreenResolver.Resolve(new Surface("opaque:desktop", "v1", -1920, -200, 4480, 1640), onLeft, screens) == null,
                "renderer_other_id_bbox_not_desktop");
            check(ScreenResolver.Resolve(new Surface("Desktop", "v1", -1920, -200, 4480, 1640), onLeft, screens) == null,
                "renderer_desktop_id_case_sensitive");
            check(ScreenResolver.Resolve(new Surface("desktop:1", "v1", -1920, -200, 4480, 1640), onLeft, screens) == null,
                "renderer_desktop_id_no_prefix_inference");
            check(ScreenResolver.Resolve(new Surface("desktop", "v1", -1920, -200, 1920, 1080), onLeft, new [] { left }) != null,
                "renderer_legacy_single_desktop_id_matches_geometry");

            // Frozen StageA RED requirements; expected results are unchanged in StageB.
            Surface desktop = new Surface("desktop", "v1", -1920, -200, 4480, 1640);
            result = ScreenResolver.Resolve(desktop, onLeft, screens);
            check(result != null, "renderer_desktop_negative_origin_dual_screen_bbox");
            check(result != null && (result.Anchor == left || result.Anchor == right) &&
                result.Anchor.Bounds.Contains(result.Anchor.WorkingArea), "renderer_desktop_anchor_real_controlled_workarea");
            check(result != null && result.CanShowPointer, "renderer_desktop_pointer_on_real_screen");
            // Inside bbox, below left screen and left of right screen: must not draw a ring.
            Pointer gap = new Pointer(-100, 1200, "move");
            result = ScreenResolver.Resolve(desktop, gap, screens);
            check(result != null && !result.CanShowPointer, "renderer_desktop_gap_pointer_rejected_without_losing_surface");
        }
    }

    internal static class RendererBoundarySelfTests {
        static ScreenFacts Fact(string id, int x, int y, int width, int height) {
            System.Drawing.Rectangle b = new System.Drawing.Rectangle(x, y, width, height);
            return new ScreenFacts(id, b, b);
        }
        static void RejectLayout(Action<bool, string> check, ScreenFacts screen, double dpi, string code, string name) {
            try { OverlayLayout.Calculate(screen, dpi); }
            catch (WireFailure error) { check(error.Code == code, name); return; }
            check(false, name);
        }
        internal static void Run(Action<bool, string> check) {
            ScreenFacts left = Fact("left", -1920, -200, 1920, 1080);
            ScreenFacts right = Fact("right", 0, 0, 2560, 1440);
            ScreenFacts[] screens = { left, right };
            Surface desktop = new Surface("desktop", "v1", -1920, -200, 4480, 1640);
            Pointer point = new Pointer(-100, 100, "move");
            check(ScreenResolver.Resolve(new Surface("desktop", "v1", -1920, -200, 1920, 1080), point, screens) == null,
                "renderer_desktop_partial_bbox_rejected");
            double[] box = { -1920, -200, 4480, 1640 };
            for (int field = 0; field < 4; field++) {
                double[] changed = (double[])box.Clone(); changed[field] += 0.25;
                check(ScreenResolver.Resolve(new Surface("desktop", "v1", changed[0], changed[1], changed[2], changed[3]), point, screens) == null,
                    "renderer_desktop_exact_component_" + field);
                double[] nonfinite = { Double.NaN, Double.PositiveInfinity, Double.NegativeInfinity };
                for (int kind = 0; kind < nonfinite.Length; kind++) {
                    changed = (double[])box.Clone(); changed[field] = nonfinite[kind];
                    check(ScreenResolver.Resolve(new Surface("desktop", "v1", changed[0], changed[1], changed[2], changed[3]), point, screens) == null,
                        "renderer_desktop_nonfinite_" + field + "_" + kind);
                }
            }
            check(ScreenResolver.Resolve(desktop, point, null) == null, "renderer_desktop_unknown_topology");
            check(ScreenResolver.Resolve(desktop, point, new ScreenFacts[0]) == null, "renderer_desktop_empty_topology");
            check(ScreenResolver.Resolve(desktop, point, new [] { left, (ScreenFacts)null }) == null, "renderer_desktop_unknown_member");
            check(ScreenResolver.Resolve(desktop, point, new [] { left, Fact("left", 0, 0, 2560, 1440) }) == null,
                "renderer_desktop_duplicate_identity_rejected");
            check(ScreenResolver.Resolve(desktop, point, new [] { left, Fact("", 0, 0, 2560, 1440) }) == null,
                "renderer_desktop_missing_identity_rejected");
            check(ScreenResolver.Resolve(new Surface("desktop", "v1", -1920, -200, 1920, 1080), point,
                new [] { left, Fact("mirror", -1920, -200, 1920, 1080) }) == null, "renderer_desktop_mirror_not_first_match");
            check(ScreenResolver.Resolve(new Surface("desktop", "v1", -1920, -200, 4320, 1640), point,
                new [] { left, Fact("overlap", -160, 0, 2560, 1440) }) == null, "renderer_desktop_overlap_rejected");
            check(ScreenResolver.Resolve(desktop, point, new [] { left, right, Fact("zero", 0, 0, 0, 10) }) == null,
                "renderer_desktop_zero_screen_rejected");
            check(ScreenResolver.Resolve(desktop, point, new [] { left, right, Fact("negative", 0, 0, -1, 10) }) == null,
                "renderer_desktop_negative_screen_rejected");
            check(ScreenResolver.Resolve(desktop, point, new [] { left, right, Fact("overflow", Int32.MaxValue, 0, 1, 10) }) == null,
                "renderer_desktop_edge_overflow_rejected");
            ScreenFacts outsideWork = new ScreenFacts("outside", right.Bounds, new System.Drawing.Rectangle(-1, 0, 2560, 1400));
            check(ScreenResolver.Resolve(desktop, point, new [] { left, outsideWork }) == null, "renderer_desktop_workarea_outside_rejected");
            ScreenFacts emptyWork = new ScreenFacts("empty", right.Bounds, new System.Drawing.Rectangle(0, 0, 0, 0));
            check(ScreenResolver.Resolve(desktop, point, new [] { left, emptyWork }) == null, "renderer_desktop_empty_workarea_rejected");
            check(ScreenResolver.Resolve(new Surface("desktop", "v1", 0, 0, -1, 1440), null, screens) == null,
                "renderer_desktop_negative_surface_rejected");
            check(ScreenResolver.Resolve(new Surface("desktop", "v1", Double.MaxValue, 0, Double.MaxValue, 1440), null, screens) == null,
                "renderer_desktop_surface_edge_overflow_rejected");
            check(ScreenResolver.Resolve(null, null, screens) == null, "renderer_null_surface_unavailable");

            ScreenResolution result = ScreenResolver.Resolve(desktop, null, screens);
            check(result != null && !result.CanShowPointer, "renderer_desktop_null_pointer_keeps_surface");
            result = ScreenResolver.Resolve(desktop, new Pointer(Double.NaN, 0, "move"), screens);
            check(result != null && !result.CanShowPointer, "renderer_desktop_nan_pointer_hidden");
            result = ScreenResolver.Resolve(desktop, new Pointer(0, Double.PositiveInfinity, "move"), screens);
            check(result != null && !result.CanShowPointer, "renderer_desktop_infinite_pointer_hidden");
            result = ScreenResolver.Resolve(desktop, new Pointer(0, 0, "move"), screens);
            check(result != null && result.CanShowPointer && result.PointerScreen == right,
                "renderer_desktop_touching_edge_belongs_to_right");
            result = ScreenResolver.Resolve(desktop, new Pointer(2560, 100, "move"), screens);
            check(result != null && !result.CanShowPointer, "renderer_desktop_right_edge_excluded");
            result = ScreenResolver.Resolve(desktop, new Pointer(100, 1440, "move"), screens);
            check(result != null && !result.CanShowPointer, "renderer_desktop_bottom_edge_excluded");
            result = ScreenResolver.Resolve(desktop, new Pointer(-0.1, -100, "move"), screens);
            check(result != null && result.CanShowPointer && result.RingCenter.X == -1 && result.RingCenter.Y == -100,
                "renderer_desktop_fractional_rounding_not_into_gap");
            result = ScreenResolver.Resolve(desktop, new Pointer(-1921, 0, "move"), screens);
            check(result != null && !result.CanShowPointer, "renderer_desktop_outside_pointer_keeps_surface");
            result = ScreenResolver.Resolve(new Surface("opaque", "v1", -1920, -200, 1920, 1080), new Pointer(100, 100, "move"), screens);
            check(result != null && !result.CanShowPointer, "renderer_single_pointer_other_screen_hidden");

            ScreenFacts upper = new ScreenFacts("upper", new System.Drawing.Rectangle(0, -1200, 1600, 1200),
                new System.Drawing.Rectangle(40, -1170, 1560, 1170));
            ScreenFacts lower = new ScreenFacts("lower", new System.Drawing.Rectangle(0, 0, 1920, 1080),
                new System.Drawing.Rectangle(0, 0, 1920, 1040));
            Surface vertical = new Surface("desktop", "v1", 0, -1200, 1920, 2280);
            result = ScreenResolver.Resolve(vertical, new Pointer(100, -100, "click"), new [] { upper, lower });
            check(result != null && result.Anchor == upper && result.PointerScreen == upper, "renderer_desktop_vertical_negative_origin");
            result = ScreenResolver.Resolve(vertical, new Pointer(1700, -100, "move"), new [] { upper, lower });
            check(result != null && !result.CanShowPointer && result.Anchor == upper, "renderer_desktop_vertical_gap_stable_anchor");
            result = ScreenResolver.Resolve(vertical, new Pointer(100, -100, "move"), new [] { lower, upper });
            check(result != null && result.Anchor == lower && result.PointerScreen == upper, "renderer_desktop_anchor_order_not_pointer");
            check(ScreenResolver.Resolve(new Surface("opaque", "v1", 0, -1200, 1920, 2280), null, new [] { upper, lower }) == null,
                "renderer_single_vertical_bbox_not_desktop");
            ScreenFacts diagonalLeft = Fact("diagonal-left", -1200, 0, 1200, 900);
            ScreenFacts diagonalRight = Fact("diagonal-right", 0, -800, 1600, 800);
            Surface diagonal = new Surface("desktop", "v1", -1200, -800, 2800, 1700);
            result = ScreenResolver.Resolve(diagonal, new Pointer(-1200, -800, "move"), new [] { diagonalLeft, diagonalRight });
            check(result != null && !result.CanShowPointer && result.Anchor == diagonalLeft, "renderer_desktop_bbox_origin_itself_is_gap");
            OverlayLayout diagonalLayout = OverlayLayout.Calculate(result.Anchor, 96);
            check(diagonalLeft.WorkingArea.Contains(diagonalLayout.Stop) && diagonalLayout.Stop.Top >= 0,
                "renderer_layout_diagonal_anchor_not_bbox_origin");
            check(ScreenResolver.Resolve(desktop, null, new [] { left, Fact("LEFT", 0, 0, 2560, 1440) }) == null,
                "renderer_desktop_case_alias_identity_rejected");
            ScreenFacts farLeft = Fact("far-left", Int32.MinValue, 0, 10, 100);
            ScreenFacts farRight = Fact("far-right", Int32.MaxValue - 10, 0, 10, 100);
            check(ScreenResolver.Resolve(new Surface("desktop", "v1", Int32.MinValue, 0, 4294967295d, 100), null,
                new [] { farLeft, farRight }) == null, "renderer_desktop_union_span_overflow_rejected");
            LayoutCases(check, upper);
        }
        static void LayoutCases(Action<bool, string> check, ScreenFacts upper) {
            OverlayLayout layout = OverlayLayout.Calculate(upper, 96);
            check(layout.Bar == new System.Drawing.Rectangle(56, -1154, 352, 48) &&
                layout.Stop == new System.Drawing.Rectangle(416, -1154, 88, 48), "renderer_layout_96_workarea_offsets");
            check(upper.WorkingArea.Contains(layout.Bar) && upper.WorkingArea.Contains(layout.Stop), "renderer_layout_negative_monitor_containment");
            ScreenFacts small = Fact("small", -450, -200, 450, 200);
            layout = OverlayLayout.Calculate(small, 192);
            check(small.WorkingArea.Contains(layout.Bar) && small.WorkingArea.Contains(layout.Stop) && layout.Stop.Width == 176 &&
                layout.Stop.Height == 96 && layout.Bar.Width >= 160, "renderer_layout_high_dpi_small_area_stop_reachable");
            ScreenFacts exact = Fact("exact", 0, 0, 416, 160);
            layout = OverlayLayout.Calculate(exact, 192);
            check(layout.Bar.Width == 160 && layout.Stop.Right == 384 && layout.Stop.Bottom == 128, "renderer_layout_minimum_exact_fit");
            RejectLayout(check, Fact("narrow", 0, 0, 415, 160), 192, "surface_workarea_too_small", "renderer_layout_one_pixel_too_narrow");
            RejectLayout(check, Fact("short", 0, 0, 416, 159), 192, "surface_workarea_too_small", "renderer_layout_one_pixel_too_short");
            RejectLayout(check, Fact("tiny", 0, 0, 20, 20), 96, "surface_workarea_too_small", "renderer_layout_tiny_area_not_claimed_visible");
            RejectLayout(check, upper, -96, "window_dpi_failed", "renderer_layout_negative_dpi");
            RejectLayout(check, upper, Double.Epsilon, "window_dpi_failed", "renderer_layout_sub_native_dpi");
            RejectLayout(check, upper, 0, "window_dpi_failed", "renderer_layout_zero_dpi");
            RejectLayout(check, upper, Double.NaN, "window_dpi_failed", "renderer_layout_nan_dpi");
            RejectLayout(check, upper, Double.PositiveInfinity, "window_dpi_failed", "renderer_layout_infinite_dpi");
            RejectLayout(check, upper, Double.MaxValue, "surface_workarea_too_small", "renderer_layout_huge_dpi_no_integer_overflow");
            RejectLayout(check, null, 96, "surface_geometry_unavailable", "renderer_layout_unknown_screen");
            RejectLayout(check, Fact("overflow", Int32.MaxValue, 0, 10, 100), 96, "surface_geometry_unavailable", "renderer_layout_overflow_bounds");
            RejectLayout(check, new ScreenFacts("invalid", upper.Bounds, new System.Drawing.Rectangle(-1, -1200, 1600, 1200)),
                96, "surface_geometry_unavailable", "renderer_layout_workarea_outside_bounds");
            double[] dpis = { 96, 120, 144, 168, 192, 240, 288 };
            for (int i = 0; i < dpis.Length; i++) {
                layout = OverlayLayout.Calculate(upper, dpis[i]);
                check(upper.WorkingArea.Contains(layout.Bar) && upper.WorkingArea.Contains(layout.Stop) && layout.Bar.Right < layout.Stop.Left &&
                    layout.Stop.Width >= Math.Ceiling(88 * dpis[i] / 96), "renderer_layout_dpi_containment_" + i);
            }
        }
    }

}
