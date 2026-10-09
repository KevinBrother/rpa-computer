import Foundation

// Deliberately Foundation-only. Called before NSApplication.shared is touched.
enum SelfTests {
    static func run() throws {
        var checks = 0
        func check(_ value: @autoclosure () -> Bool, _ name: String) throws {
            checks += 1
            if !value() { throw WireFailure("self_test_" + name) }
        }
        func rejects(_ json: String) throws {
            checks += 1
            do { _ = try Snapshot.parse(Data(json.utf8)) }
            catch is WireFailure { return }
            throw WireFailure("self_test_accepted_invalid_frame")
        }
        let base = """
        {"type":"snapshot","version":1,"sequence":1,"session":{"id":"s","generation":18446744073709551615},"phase":"idle","cleanup":"not_needed","surface":{"id":"macos:1","version":"v1","x":-1512,"y":-200,"width":1512,"height":982},"pointer":{"x":-1400,"y":-100,"kind":"click"}}
        """
        let initial = try Snapshot.parse(Data(base.utf8))
        let geometryVersion = "d1:o0,0:i1512x982:c3024x1964:r0"
        let actualGeometry = try Snapshot.parse(Data(base.replacingOccurrences(of: "\"version\":\"v1\"", with: "\"version\":\"\(geometryVersion)\"").utf8))
        try check(actualGeometry.surface?.version == geometryVersion, "surface_version_commas_preserved")
        try check(initial.session?.generation == UInt64.max, "generation_exact")
        try check(initial.phase == "idle", "idle_parse")
        let maximumSequence = try Snapshot.parse(Data(base.replacingOccurrences(of: "\"sequence\":1", with: "\"sequence\":18446744073709551615").utf8))
        try check(maximumSequence.sequence == UInt64.max, "sequence_exact")
        for (old, new) in [
            ("\"version\":1", "\"version\":2"),
            ("\"version\":1", "\"version\":1.0"),
            ("\"sequence\":1", "\"sequence\":-1"),
            ("\"sequence\":1", "\"sequence\":true"),
            ("\"sequence\":1", "\"sequence\":1,\"sequen\\u0063e\":2"),
            ("\"generation\":18446744073709551615", "\"generation\":-1"),
            ("\"version\":\"v1\"", "\"version\":\"\""),
            ("\"sequence\":1", "\"sequence\":18446744073709551616"),
            ("\"sequence\":1", "\"sequence\":1,\"sequence\":2"),
            ("\"id\":\"s\"", "\"id\":\"\""),
            ("\"phase\":\"idle\"", "\"phase\":\"running\""),
            ("\"cleanup\":\"not_needed\"", "\"cleanup\":\"safe\""),
            ("\"width\":1512", "\"width\":0"),
            ("\"x\":-1512", "\"x\":1e999"),
            ("\"kind\":\"click\"", "\"kind\":\"key\""),
            ("\"version\":1", "\"version\":1,\"text\":\"secret\""),
            ("\"pointer\":{\"x\":-1400,\"y\":-100,\"kind\":\"click\"}", "\"pointer\":{}")
        ] { try rejects(base.replacingOccurrences(of: old, with: new)) }
        try rejects(base + "{}"); try rejects("[]"); try rejects("{broken}")
        try rejects(base.replacingOccurrences(of: "\"session\":{\"id\":\"s\",\"generation\":18446744073709551615},", with: ""))
        try rejects(base.replacingOccurrences(of: "\"id\":\"s\"", with: "\"id\":\"\\ud800\""))
        checks += 1
        if tryValue({ try Snapshot.parse(Data([0xff])) }) != nil { throw WireFailure("self_test_invalid_utf8") }
        try check(tryValue { try Snapshot.parse(Data(base.replacingOccurrences(of: "\"generation\":18446744073709551615", with: "\"generation\":0").utf8)) } != nil, "zero_generation")
        let nulls = "{\"type\":\"snapshot\",\"version\":1,\"sequence\":0,\"session\":null,\"phase\":\"starting\",\"cleanup\":\"not_needed\",\"surface\":null,\"pointer\":null}"
        var state = FeedbackState()
        try check(!state.canStop && state.status == "Waiting for Host", "honest_initial")
        let emptySnapshot = try Snapshot.parse(Data(nulls.utf8))
        try check(state.apply(emptySnapshot), "first_zero_sequence")
        try check(state.apply(initial) && state.canStop, "session_enables_stop")
        try check(!state.apply(emptySnapshot), "old_frame")
        try check(!state.apply(initial), "duplicate_frame")
        let stop = state.requestStop()
        try check(stop?.generation == UInt64.max && !state.canStop, "stop_current")
        try check(state.status == "Stop requested · awaiting Host", "no_local_safe_claim")
        try check(state.requestStop() == nil, "deduplicate_stop")
        let regrant = try Snapshot.parse(Data(base.replacingOccurrences(of: "\"sequence\":1", with: "\"sequence\":2").replacingOccurrences(of: "18446744073709551615", with: "0").utf8))
        try check(state.apply(regrant) && state.canStop, "new_generation_resets_stop")
        try check(state.requestStop()?.generation == 0, "new_stop_current_generation")
        var noSession = FeedbackState()
        let unauthorized = try Snapshot.parse(Data(nulls.replacingOccurrences(of: "starting", with: "executing").utf8))
        _ = noSession.apply(unauthorized)
        try check(!noSession.canStop && noSession.requestStop() == nil && noSession.status == "Waiting for control session", "no_session_no_executing_claim")
        for cleanup in ["failed", "unknown", "pending", "released", "not_needed"] {
            var fresh = FeedbackState()
            let closed = base.replacingOccurrences(of: "\"phase\":\"idle\"", with: "\"phase\":\"closed\"").replacingOccurrences(of: "\"cleanup\":\"not_needed\"", with: "\"cleanup\":\"\(cleanup)\"")
            _ = fresh.apply(try Snapshot.parse(Data(closed.utf8)))
            try check(!fresh.canStop, "closed_no_stop")
            try check(fresh.status.contains(cleanup == "released" ? "released" : cleanup == "not_needed" ? "no cleanup" : cleanup), "cleanup_truth")
        }
        let point = Geometry.appKitPointer(initial.pointer!, surface: initial.surface!, frameLeft: -1512, frameTop: 800)
        try check(point?.x == -1400 && point?.y == 700, "negative_origin_no_pixel_scaling")
        try check(Geometry.contains(initial.surface!, x: 0, y: -100) == false, "exclusive_surface_edge")
        try check(Geometry.appKitPointer(Pointer(x: 999, y: 0, kind: "move"), surface: initial.surface!, frameLeft: 0, frameTop: 0) == nil, "outside_hidden")
        let style = try Style.parse(["--accent", "#12aBcD", "--label", "AI 控制"])
        try check(style.rgb == 0x12ABCD && style.label == "AI 控制", "style_valid")
        for args in [["--accent", "red"], ["--label", ""], ["--label", "a\nb"], ["--label", String(repeating: "x", count: 49)], ["--foo"], ["--accent"], ["--label", "x", "--label", "y"], ["--label", "a\u{202e}b"]] {
            checks += 1
            if tryValue({ try Style.parse(args) }) != nil { throw WireFailure("self_test_invalid_style") }
        }
        let encoded = try WireOutput.stop(Session(id: "q\"\\\n", generation: UInt64.max))
        let value = try JSONValue.parse(encoded)
        try check(tryValue { try value.object()["session"]!.object()["id"]!.string() } == "q\"\\\n", "stop_escape")
        try check(!String(decoding: encoded, as: UTF8.self).contains("sequence"), "stop_not_sequence_authorization")
        var lines = BoundedLines()
        try check(lines.feed(Data(repeating: 97, count: 16_383)).isEmpty, "line_at_bound")
        let overflow = lines.feed(Data([97]))
        try check(overflow.count == 1 && overflow[0].failure == "frame_too_large", "oversize_bounded")
        _ = lines.feed(Data([10]))
        try check(lines.feed(Data("{}\r\n".utf8)).first?.bytes == Data("{}".utf8), "crlf")
        _ = lines.feed(Data("{}".utf8))
        try check(lines.finish()?.failure == "truncated_frame", "eof_partial")
        var mailbox = LatestMailbox()
        mailbox.offer(initial)
        mailbox.offer(try Snapshot.parse(Data(nulls.utf8)))
        try check(mailbox.take()?.sequence == 1, "mailbox_no_rollback")
        mailbox.offer(try Snapshot.parse(Data(nulls.utf8)))
        try check(mailbox.take() == nil, "mailbox_seen_watermark")
        // stdout is reserved for actual protocol in normal mode, stderr for self-test summary.
        FileHandle.standardError.write(Data("self-test: \(checks) checks passed (pure; no GUI/ready/input)\n".utf8))
    }
    private static func tryValue<T>(_ block: () throws -> T) -> T? { try? block() }
}
