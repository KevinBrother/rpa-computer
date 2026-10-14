//! Desktop authority contracts using mock backend and REAL built host/client TLS.
//! These are NOT Windows GUI acceptance tests. CC + GLM runs them on Windows.
//! Serial execution is required: --test-threads=1 (also across integration binaries).

include!("support/remote_harness.rs");

#[test]
fn remote_control_minimal_handoff() {
    // Mock backend + real TLS binaries: transport regression, NOT GUI acceptance.
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut a);
    let a_session = open_session(&mut a, 2);
    let mut b = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    // Old behavior rejects this connection; this is the minimal RED assertion.
    mcp_handshake(&mut b);
    send_frame(
        &mut b,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let discovery = read_response(&b);
    assert_eq!(discovery["id"], 2);
    assert!(
        discovery["result"]["tools"].is_array(),
        "discovery: {discovery}"
    );
    send_frame(
        &mut b,
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"computer_open","arguments":{}}}),
    );
    let conflict = read_response(&b);
    assert_eq!(conflict["id"], 3);
    assert_eq!(conflict["result"]["isError"], true, "conflict: {conflict}");
    let payload: serde_json::Value = serde_json::from_str(
        conflict["result"]["content"][0]["text"]
            .as_str()
            .expect("structured error text"),
    )
    .unwrap();
    assert_eq!(
        payload["error"]["code"], "lease_conflict",
        "conflict: {conflict}"
    );
    send_frame(
        &mut a,
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"computer_close","arguments":{"session_id":a_session}}}),
    );
    let closed = read_response(&a);
    assert_eq!(closed["id"], 4);
    assert_eq!(closed["result"]["isError"], false, "close: {closed}");
    let b_session = open_session(&mut b, 4);
    send_frame(
        &mut b,
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
        "params":{"name":"computer_observe","arguments":{"session_id":b_session}}}),
    );
    expect_png_in_observe(&read_response(&b));
    send_frame(
        &mut b,
        serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call",
        "params":{"name":"computer_close","arguments":{"session_id":b_session}}}),
    );
    let closed = read_response(&b);
    assert_eq!(closed["result"]["isError"], false, "close: {closed}");
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

include!("support/session_control.rs");

#[test]
fn remote_control_concurrent_discovery_does_not_reserve_control() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    for client in [&mut a, &mut b] {
        let r = call_tool(client, 3, "computer_describe", json!({}));
        success(&r);
        assert_no_image(&r);
    }
    // B (not the first connection) takes the first task.
    let session = open_session(&mut b, 4);
    conflict(&mut a, 4);
    observe(&mut b, 5, &session);
    close_session(&mut b, 6, &session);
    let next = open_session(&mut a, 5);
    observe(&mut a, 6, &next);
    close_session(&mut a, 7, &next);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_stale_and_foreign_requests_cannot_stop_new_owner() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    let old = open_session(&mut a, 3);
    observe(&mut a, 4, &old);
    close_session(&mut a, 5, &old);
    let current = open_session(&mut b, 3);
    observe(&mut b, 4, &current);
    // Include the CORRECT B session: possession of a session ID is not authority.
    for (index, session) in [old.as_str(), current.as_str(), "wrong-session"]
        .iter()
        .enumerate()
    {
        for (offset, name) in [
            "computer_observe",
            "computer_pause",
            "computer_resume",
            "computer_close",
        ]
        .iter()
        .enumerate()
        {
            let id = 10 + (index * 10 + offset) as u64;
            rejected(&call_tool(&mut a, id, name, json!({"session_id":session})));
            observe(&mut b, id, &current);
        }
    }
    let fresh = observe(&mut b, 49, &current);
    let step_args = json!({"session_id":current,"request_id":"foreign-step",
        "based_on":fresh["observation_id"],"action":{"kind":"move","position":[1,1]}});
    let denied = call_tool(&mut a, 50, "computer_step", step_args.clone());
    assert_eq!(rejected(&denied)["error"]["code"], "lease_conflict");
    // Identical request is valid for the actual owner: no invalid action or
    // stale based_on can masquerade as an authority denial.
    expect_png_in_observe(&call_tool(&mut b, 50, "computer_step", step_args));
    conflict(&mut a, 51);
    // Also validate session identity for requests from the owning transport.
    for (id, name) in [(52, "computer_close"), (53, "computer_pause")] {
        rejected(&call_tool(&mut b, id, name, json!({"session_id":old})));
        observe(&mut b, id + 10, &current);
        conflict(&mut a, id);
    }
    close_session(&mut b, 70, &current);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_idle_and_former_owner_disconnects_do_not_stop_owner() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut b = connect_client(&g, &host);
    mcp_handshake(&mut b);
    for formerly_owned in [false, true] {
        let mut a = connect_client(&g, &host);
        mcp_handshake(&mut a);
        if formerly_owned {
            let old = open_session(&mut a, 2);
            close_session(&mut a, 3, &old);
        }
        let current = open_session(&mut b, 10);
        conflict(&mut a, 4);
        send_tool(
            &mut b,
            11,
            "computer_observe",
            json!({"session_id":current,"wait_ms":1000}),
        );
        // EOF on an authenticated nonowner, while B has outstanding work.
        assert!(a.wait(TEST_TIMEOUT).success());
        expect_png_in_observe(&reply_for(&b, 11));
        observe(&mut b, 12, &current);
        close_session(&mut b, 13, &current);
    }
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_repeated_tasks_retire_old_session_authority() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    let mut retired = Vec::<String>::new();
    for turn in 0..4 {
        let (owner, other) = if turn % 2 == 0 {
            (&mut a, &mut b)
        } else {
            (&mut b, &mut a)
        };
        let id = 10 + turn * 20;
        let current = open_session(owner, id);
        conflict(other, id);
        for (offset, old) in retired.iter().enumerate() {
            rejected(&call_tool(
                owner,
                id + 1 + offset as u64,
                "computer_close",
                json!({"session_id":old}),
            ));
            rejected(&call_tool(
                owner,
                id + 100 + offset as u64,
                "computer_pause",
                json!({"session_id":old}),
            ));
        }
        // No resume/reopen here: invalid close must not cancel the new task.
        observe(owner, id + 6, &current);
        close_session(owner, id + 7, &current);
        retired.push(current);
        discovery_barrier(owner, id + 8);
        discovery_barrier(other, id + 8); // no queued open or unsolicited reply
    }
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_pause_retains_authority_until_valid_close() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    let session = open_session(&mut a, 3);
    observe(&mut a, 4, &session);
    let paused = success(&call_tool(
        &mut a,
        5,
        "computer_pause",
        json!({"session_id":session}),
    ));
    assert_eq!(paused["state"], "paused");
    conflict(&mut b, 3);
    rejected(&call_tool(
        &mut b,
        4,
        "computer_resume",
        json!({"session_id":session}),
    ));
    rejected(&call_tool(
        &mut a,
        6,
        "computer_observe",
        json!({"session_id":session}),
    ));
    let description = call_tool(&mut b, 5, "computer_describe", json!({}));
    success(&description);
    assert_no_image(&description);
    conflict(&mut b, 6);
    success(&call_tool(
        &mut a,
        7,
        "computer_resume",
        json!({"session_id":session}),
    ));
    observe(&mut a, 8, &session);
    close_session(&mut a, 9, &session);
    let next = open_session(&mut b, 7);
    observe(&mut b, 8, &next);
    close_session(&mut b, 9, &next);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_invalid_and_failed_open_unwind_only_their_own_authority() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    // Last case is valid syntax but backend selection failure, not argument parsing.
    for (index, args) in [
        json!({"max_width":0}),
        json!({"display":{"kind":"id","id":""}}),
        json!({"display":{"kind":"id","id":"nonexistent-mock-display"}}),
    ]
    .into_iter()
    .enumerate()
    {
        let id = 10 + index as u64 * 10;
        rejected(&call_tool(&mut a, id, "computer_open", args.clone()));
        let session = open_session(&mut b, id);
        rejected(&call_tool(&mut a, id + 1, "computer_open", args));
        // The owner itself attempting an already-open task must not release it.
        rejected(&call_tool(&mut b, id + 1, "computer_open", json!({})));
        for (offset, bad_close) in [
            json!({}),
            json!({"session_id":42}),
            json!({"session_id":"stale"}),
        ]
        .into_iter()
        .enumerate()
        {
            rejected(&call_tool(
                &mut b,
                id + 2 + offset as u64,
                "computer_close",
                bad_close,
            ));
            conflict(&mut a, id + 2 + offset as u64);
        }
        observe(&mut b, id + 6, &session);
        close_session(&mut b, id + 7, &session);
    }
    let session = open_session(&mut a, 50);
    close_session(&mut a, 51, &session);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_nonowner_cannot_retrieve_cached_screenshot() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    let session = open_session(&mut a, 3);
    let observation = observe(&mut a, 4, &session);
    let step = call_tool(
        &mut a,
        5,
        "computer_step",
        json!({"session_id":session,
        "request_id":"private-result","based_on":observation["observation_id"],
        "action":{"kind":"move","position":[1,1]}}),
    );
    expect_png_in_observe(&step);
    let cached = call_tool(
        &mut a,
        6,
        "computer_get_step",
        json!({"session_id":session,"request_id":"private-result"}),
    );
    expect_png_in_observe(&cached); // positive control: a screenshot actually exists
    rejected(&call_tool(
        &mut b,
        3,
        "computer_get_step",
        json!({"session_id":session,"request_id":"private-result"}),
    ));
    rejected(&call_tool(
        &mut b,
        4,
        "computer_observe",
        json!({"session_id":session}),
    ));
    let description = call_tool(&mut b, 5, "computer_describe", json!({}));
    success(&description);
    assert_no_image(&description);
    observe(&mut a, 7, &session);
    close_session(&mut a, 8, &session);
    let next = open_session(&mut b, 6);
    rejected(&call_tool(
        &mut b,
        7,
        "computer_get_step",
        json!({"session_id":next,"request_id":"private-result"}),
    ));
    observe(&mut b, 8, &next);
    close_session(&mut b, 9, &next);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_cancellation_ids_are_scoped_to_connection_and_task() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    let old = open_session(&mut a, 3);
    send_tool(
        &mut a,
        40,
        "computer_observe",
        json!({"session_id":old,"wait_ms":1500}),
    );
    cancel(&mut b, json!(40)); // same numeric ID, different connection
    cancel(&mut b, json!("40")); // different type, also different connection
    discovery_barrier(&mut b, 41);
    expect_png_in_observe(&reply_for(&a, 40));
    observe(&mut a, 42, &old);
    close_session(&mut a, 43, &old);
    let current = open_session(&mut b, 42);
    send_tool(
        &mut b,
        40,
        "computer_observe",
        json!({"session_id":current,"wait_ms":1500}),
    );
    cancel(&mut a, json!(40)); // formerly owned task's delayed cancellation
                               // A caller-supplied CURRENT owner session cannot upgrade a foreign cancel.
    send_frame(
        &mut a,
        json!({"jsonrpc":"2.0","method":"notifications/cancelled",
        "params":{"requestId":40,"session_id":current}}),
    );
    discovery_barrier(&mut a, 44);
    expect_png_in_observe(&reply_for(&b, 40));
    observe(&mut b, 43, &current);
    close_session(&mut b, 44, &current);
    // Same transport, new task: cancelling a COMPLETED prior task request
    // must not latch a stop in the fresh executor (do not reuse that ID).
    let reopened = open_session(&mut a, 50);
    cancel(&mut a, json!(40));
    discovery_barrier(&mut a, 51);
    observe(&mut a, 52, &reopened);
    close_session(&mut a, 53, &reopened);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_simultaneous_open_has_exactly_one_winner() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    for round in 0..3 {
        let id = 10 + round * 10;
        send_tool(&mut a, id, "computer_open", json!({}));
        send_tool(&mut b, id, "computer_open", json!({}));
        let ra = reply_for(&a, id);
        let rb = reply_for(&b, id);
        assert_ne!(
            ra["result"]["isError"], rb["result"]["isError"],
            "exactly one simultaneous open must succeed: A={ra}, B={rb}"
        );
        let (owner, waiter, accepted, denied) = if ra["result"]["isError"] == false {
            (&mut a, &mut b, ra, rb)
        } else {
            (&mut b, &mut a, rb, ra)
        };
        assert_eq!(rejected(&denied)["error"]["code"], "lease_conflict");
        let session = success(&accepted)["session_id"]
            .as_str()
            .expect("session")
            .to_owned();
        observe(owner, id + 1, &session);
        close_session(owner, id + 2, &session);
        // A denied open cannot be queued/replayed on release.
        discovery_barrier(waiter, id + 1);
        let next = open_session(waiter, id + 2);
        observe(waiter, id + 3, &next);
        close_session(waiter, id + 4, &next);
    }
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_owner_eof_or_abrupt_exit_allows_bounded_clean_handoff() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut b = connect_client(&g, &host);
    mcp_handshake(&mut b);
    for abrupt in [false, true] {
        let mut a = connect_client(&g, &host);
        mcp_handshake(&mut a);
        let old = open_session(&mut a, 2);
        observe(&mut a, 3, &old);
        conflict(&mut b, 10);
        if abrupt {
            // Kill only this fixture-owned client via its retained Child handle.
            a.child.as_mut().unwrap().kill().expect("kill owned client");
            assert!(!a.wait(BINARY_TIMEOUT).success());
        } else {
            assert!(a.wait(BINARY_TIMEOUT).success()); // stdin EOF, no computer_close
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut id = 11;
        let current = loop {
            assert!(
                Instant::now() < deadline,
                "mock owner disconnect never retired authority"
            );
            let r = call_tool(&mut b, id, "computer_open", json!({}));
            id += 1;
            if r["result"]["isError"] == false {
                break success(&r)["session_id"]
                    .as_str()
                    .expect("session")
                    .to_owned();
            }
            // Explicit caller retry, not queueing/automatic host replay. The
            // connection remains intact while cleanup is in progress.
            rejected(&r);
            std::thread::sleep(Duration::from_millis(50));
        };
        observe(&mut b, id, &current);
        rejected(&call_tool(
            &mut b,
            id + 1,
            "computer_close",
            json!({"session_id":old}),
        ));
        observe(&mut b, id + 2, &current);
        close_session(&mut b, id + 3, &current);
    }
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_slow_and_malformed_nonowners_cannot_block_owner() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut owner = connect_client(&g, &host);
    mcp_handshake(&mut owner);
    let session = open_session(&mut owner, 2);
    // Silent pre-TLS peer: it must not occupy the accept loop.
    let slow_handshake = TcpStream::connect(("127.0.0.1", host.port)).unwrap();
    let mut partial = TlsPeer::connect(&g, host.port, IO_TIMEOUT);
    let token = std::fs::read(g.assets.path("good/client.token")).unwrap();
    let token = String::from_utf8(token).unwrap();
    partial.send(format!("{}\n", token.trim()).as_bytes());
    assert_eq!(partial.read_line(IO_TIMEOUT), b"AUTH_OK\n");
    partial.send(b"{\"jsonrpc\":\"2.0\""); // authenticated unfinished frame
    observe(&mut owner, 3, &session);
    let mut other = connect_client(&g, &host);
    mcp_handshake(&mut other);
    // Real client forwards a malformed JSON line; response must not affect owner.
    other
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"{invalid-json\n")
        .unwrap();
    other.stdin.as_mut().unwrap().flush().unwrap();
    let malformed = read_response(&other);
    assert_eq!(
        malformed["error"]["code"], -32700,
        "parse error: {malformed}"
    );
    assert_no_image(&malformed);
    discovery_barrier(&mut other, 2);
    conflict(&mut other, 3);
    drop(partial);
    drop(slow_handshake);
    assert!(other.wait(TEST_TIMEOUT).success());
    observe(&mut owner, 4, &session);
    close_session(&mut owner, 5, &session);
    assert!(owner.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_discovery_is_local_and_no_executor_starts_before_open() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    for client in [&mut a, &mut b] {
        let data = control_status(client, 3, "available", false);
        // Dynamic capture facts are not fabricated by an idle broker.
        assert!(
            data.get("available").is_some_and(Value::is_null),
            "idle available: {data}"
        );
        assert!(
            data.get("display_topology").is_some_and(Value::is_null),
            "idle topology: {data}"
        );
    }
    // Child mode has a stable startup log. Check absence after discovery has
    // completed, then require the SAME marker after a genuine open as a
    // positive control for the log drain. No process enumeration involved.
    let log_deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < log_deadline {
        match host
            ._proc
            .stderr_lines
            .recv_timeout(log_deadline.saturating_duration_since(Instant::now()))
        {
            Ok(line) => assert!(
                !line.contains("serving MCP on stdio"),
                "discovery spawned an executor: {line}"
            ),
            Err(RecvTimeoutError::Timeout) => break,
            Err(e) => panic!("host stderr closed during discovery: {e}"),
        }
    }
    let session = open_session(&mut a, 4);
    let deadline = Instant::now() + IO_TIMEOUT;
    loop {
        assert!(Instant::now() < deadline, "missing executor startup log");
        let line = host
            ._proc
            .stderr_lines
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("open must produce child stdio startup log");
        if line.contains("serving MCP on stdio") {
            break;
        }
    }
    control_status(&mut a, 5, "owned", true);
    control_status(&mut b, 4, "owned", false);
    close_session(&mut a, 6, &session);
    control_status(&mut a, 7, "available", false);
    control_status(&mut b, 5, "available", false);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_malformed_open_is_validated_before_authority_check() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    let invalid = [
        json!({"max_width":0}),
        json!({"max_height":1.5}),
        json!({"display":{"kind":"id","id":""}}),
        json!({"surprise":true}),
    ];
    for (index, args) in invalid.iter().enumerate() {
        let r = call_tool(&mut b, 10 + index as u64, "computer_open", args.clone());
        assert_eq!(rejected(&r)["error"]["code"], "invalid_arguments");
        control_status(&mut a, 10 + index as u64, "available", false);
    }
    let session = open_session(&mut a, 20);
    for (index, args) in invalid.into_iter().enumerate() {
        let r = call_tool(&mut b, 20 + index as u64, "computer_open", args);
        assert_eq!(
            rejected(&r)["error"]["code"],
            "invalid_arguments",
            "malformed open must be validated before checking another owner's lease"
        );
        control_status(&mut b, 30 + index as u64, "owned", false);
    }
    observe(&mut a, 21, &session);
    close_session(&mut a, 22, &session);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_duplicate_pending_id_does_not_execute_close_or_cancel_original() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    let session = open_session(&mut a, 3);
    // One write pipelines both frames; a bounded observe delay keeps the first
    // ID outstanding. Duplicate control must be refused BEFORE stop signaling.
    let observe_frame = json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{
        "name":"computer_observe","arguments":{"session_id":session,"wait_ms":2000}}});
    let duplicate = json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{
        "name":"computer_close","arguments":{"session_id":session}}});
    let stdin = a.stdin.as_mut().unwrap();
    stdin
        .write_all(format!("{observe_frame}\n{duplicate}\n").as_bytes())
        .unwrap();
    stdin.flush().unwrap();
    let replies = [reply_for(&a, 10), reply_for(&a, 10)];
    let accepted: Vec<_> = replies
        .iter()
        .filter(|r| r["result"]["isError"] == false)
        .collect();
    let denied: Vec<_> = replies
        .iter()
        .filter(|r| r.get("error").is_some())
        .collect();
    assert_eq!(
        accepted.len(),
        1,
        "original must complete once: {replies:?}"
    );
    assert_eq!(
        denied.len(),
        1,
        "duplicate must be explicitly refused: {replies:?}"
    );
    expect_png_in_observe(accepted[0]);
    // Duplicate JSON-RPC IDs are transport admission errors, not tool errors.
    assert_eq!(
        denied[0]["error"]["code"], -32000,
        "duplicate refusal: {replies:?}"
    );
    assert_no_image(denied[0]);
    conflict(&mut b, 3);
    observe(&mut a, 11, &session);
    control_status(&mut a, 12, "owned", true);
    close_session(&mut a, 13, &session);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_owner_cancel_matches_id_type_and_stops_only_matching_work() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    let session = open_session(&mut a, 3);
    send_tool(
        &mut a,
        10,
        "computer_observe",
        json!({"session_id":session,"wait_ms":1500}),
    );
    cancel(&mut a, json!("10")); // a string must not match numeric request 10
    expect_png_in_observe(&reply_for(&a, 10));
    send_tool(
        &mut a,
        11,
        "computer_observe",
        json!({"session_id":session,"wait_ms":2000}),
    );
    // A same-transport discovery reply proves the observe frame was admitted;
    // the original must still be outstanding, not a completed-ID notification.
    discovery_barrier(&mut a, 90);
    assert!(
        matches!(
            a.lines.recv_timeout(Duration::from_millis(150)),
            Err(RecvTimeoutError::Timeout)
        ),
        "delayed observe completed before the in-flight cancellation check"
    );
    cancel(&mut a, json!(11)); // positive control for the cancellation path
    let cancelled = reply_for(&a, 11);
    assert_eq!(rejected(&cancelled)["error"]["code"], "cancelled");
    conflict(&mut b, 3); // cancel is stop, not an implicit authority handoff
    success(&call_tool(
        &mut a,
        12,
        "computer_resume",
        json!({"session_id":session}),
    ));
    observe(&mut a, 13, &session);
    close_session(&mut a, 14, &session);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_connection_capacity_is_bounded_without_stopping_owner() {
    // Default host policy currently includes all handshaking/authenticated
    // transports in its 16-slot bound. Deliberately black-box (no host imports).
    const CLIENT_LIMIT: usize = 16;
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut owner = connect_client(&g, &host);
    mcp_handshake(&mut owner);
    let session = open_session(&mut owner, 2);
    let mut idle = Vec::new();
    for _ in 1..CLIENT_LIMIT {
        let mut client = connect_client(&g, &host);
        mcp_handshake(&mut client);
        discovery_barrier(&mut client, 2);
        idle.push(client);
    }
    let overflow = connect_client(&g, &host);
    assert!(
        !overflow.wait(IO_TIMEOUT).success(),
        "17th client must be refused at the transport bound"
    );
    observe(&mut owner, 3, &session);
    for client in idle {
        assert!(client.wait(BINARY_TIMEOUT).success());
    }
    observe(&mut owner, 4, &session);
    close_session(&mut owner, 5, &session);
    assert!(owner.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_control_open_dimension_bounds_do_not_poison_next_acquisition() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut a = connect_client(&g, &host);
    let mut b = connect_client(&g, &host);
    concurrent_discovery(&mut a, &mut b);
    for (index, args) in [
        json!({"max_width":4097}),
        json!({"max_height":-1}),
        json!({"max_width":"32"}),
    ]
    .into_iter()
    .enumerate()
    {
        let r = call_tool(&mut a, 10 + index as u64, "computer_open", args);
        assert_eq!(rejected(&r)["error"]["code"], "invalid_arguments");
        control_status(&mut b, 10 + index as u64, "available", false);
    }
    let opened = success(&call_tool(
        &mut b,
        20,
        "computer_open",
        json!({"max_width":32,"max_height":32}),
    ));
    let session = opened["session_id"].as_str().expect("session");
    let image = observe(&mut b, 21, session);
    for dimension in ["width_px", "height_px"] {
        let value = image[dimension].as_u64().expect("image dimension");
        assert!(
            value > 0 && value <= 32,
            "dimension budget ignored: {image}"
        );
    }
    close_session(&mut b, 22, session);
    assert!(a.wait(TEST_TIMEOUT).success());
    assert!(b.wait(TEST_TIMEOUT).success());
}
