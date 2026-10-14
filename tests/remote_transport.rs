//! Black-box integration tests for the direct TLS remote mode:
//! `computer-host --remote-listen` + `computer-client --connect`.
//!
//! All tests run against the REAL compiled binaries (stdio child mode with
//! the explicit `--mock-backend` flag — no native GUI is ever touched, and
//! mock is loudly reported by the host itself). Fixture certificates are a
//! throwaway private CA committed for TESTS ONLY; they are never deployed.
//!
//! Every subprocess is run under a hard wall-clock timeout and the real exit
//! code is asserted — a hung child is a failed test, never a silent pass.
//!
//! Auth protocol under test: TLS handshake → client sends the token line →
//! host answers the literal `AUTH_OK\n` line BEFORE any MCP data; a rejected
//! token closes with NO ack (and the token bytes are never logged or echoed).
//! Raw-TLS tests MUST consume the explicit ack before reading any RPC.

use rustls_pki_types::pem::PemObject;

include!("support/remote_harness.rs");

#[test]
fn remote_end_to_end_initialize_open_observe_close() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut client = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut client);

    send_frame(
        &mut client,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let r = read_response(&client);
    let names: Vec<&str> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"computer_open"), "tools: {names:?}");
    assert!(names.contains(&"computer_observe"));

    let session_id = open_session(&mut client, 3);

    send_frame(
        &mut client,
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"computer_observe","arguments":{"session_id":session_id}}}),
    );
    let r = read_response(&client);
    expect_png_in_observe(&r);

    send_frame(
        &mut client,
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"computer_close","arguments":{"session_id":session_id}}}),
    );
    let r = read_response(&client);
    assert_eq!(
        r["result"]["isError"],
        serde_json::json!(false),
        "close: {r}"
    );

    let status = client.wait(TEST_TIMEOUT);
    assert!(status.success(), "client exit status: {status}");
}

#[test]
fn remote_wrong_token_refused() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let bad = g
        .assets
        .create_file("bad-token", b"definitely-the-wrong-token\n");
    let mut args = client_args(&g, &host, "good/ca.pem");
    let idx = args.iter().position(|a| a == "--token-file").unwrap();
    args[idx + 1] = bad.to_string_lossy().into();
    let client = TestChild::spawn(&g.client_bin, &args);
    let status = client.wait(TEST_TIMEOUT);
    assert!(!status.success(), "wrong token must fail: {status}");
}

#[test]
fn remote_raw_tls_auth_ok_then_initialize() {
    let g = lock_tests();
    // Raw TLS peer: the literal AUTH_OK ack must arrive BEFORE any MCP data
    // and must be consumed first; bytes after the ack in the same flight
    // (here: nothing) are preserved. Then a real initialize RPC works.
    let host = start_host(&g, &[]);
    let mut peer = TlsPeer::connect(&g, host.port, IO_TIMEOUT);
    let token = std::fs::read(g.assets.path("good/client.token")).unwrap();
    peer.send(&token);
    let ack = peer.read_line(IO_TIMEOUT);
    assert_eq!(ack, b"AUTH_OK\n", "explicit auth ack required, got {ack:?}");
    let init = serde_json::to_string(&serde_json::json!({"jsonrpc":"2.0","id":1,
        "method":"initialize","params":{"protocolVersion":"2024-11-05",
        "capabilities":{},"clientInfo":{"name":"raw-tls-test","version":"0"}}}))
    .unwrap();
    peer.send(init.as_bytes());
    peer.send(b"\n");
    let line = peer.read_line(IO_TIMEOUT);
    let r: serde_json::Value = serde_json::from_slice(&line)
        .unwrap_or_else(|e| panic!("invalid JSON after ack {e}: {line:?}"));
    assert_eq!(r["id"], serde_json::json!(1));
    assert!(r["result"]["protocolVersion"].is_string(), "init: {r}");
    // Clean half-close: our close_notify, host reaps and exits the session.
    peer.conn.send_close_notify();
    let _ = peer.conn.complete_io(&mut peer.sock);
}

#[test]
fn remote_raw_tls_wrong_token_gets_no_ack() {
    let g = lock_tests();
    // Rejected auth: the host closes with NO ack and NEVER echoes the
    // presented token. The listener must survive and still serve a good
    // client afterwards (no worker was ever started for the bad peer).
    let host = start_host(&g, &[]);
    let mut peer = TlsPeer::connect(&g, host.port, IO_TIMEOUT);
    peer.send(b"totally-wrong-token\n");
    let started = Instant::now();
    let mut got: Vec<u8> = Vec::new();
    let mut saw_eof = false;
    while started.elapsed() < IO_TIMEOUT {
        // Drive I/O first: a graceful TLS shutdown reports EOF as
        // UnexpectedEof from complete_io, which is what we wait for.
        // A bare timeout (WouldBlock / TimedOut) is NOT a closure: the
        // socket simply has nothing new yet — keep polling. Only a real
        // EOF, reset, or TLS alert counts as the host closing on us.
        match peer.conn.complete_io(&mut peer.sock) {
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                saw_eof = true;
                break;
            }
            Err(_) => {
                saw_eof = true; // TLS alert / reset after a reject is fine too
                break;
            }
        }
        loop {
            let mut buf = [0u8; 8192];
            match peer.conn.reader().read(&mut buf) {
                // Clean TLS close surfaces as Ok(0) on the reader: the host
                // closed the connection, which IS the rejection signal.
                Ok(0) => {
                    saw_eof = true;
                    break;
                }
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        if saw_eof {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(saw_eof, "rejected connection was never closed by the host");
    assert!(
        got.is_empty(),
        "rejected peer must receive NO ack and NO echo, got {got:?}"
    );
    assert!(
        !got.windows(b"totally".len()).any(|w| w == b"totally"),
        "host must never echo the presented token"
    );
    // Reciprocate the close before reconnecting: the host is intentionally
    // busy while the rejected session is being torn down, and a raw TLS peer
    // that never answers close_notify would hold that cleanup open. Send our
    // close_notify best-effort (the host may already be gone — fine), then
    // DROP the peer so the TCP side fully closes.
    peer.conn.send_close_notify();
    let _ = peer.conn.complete_io(&mut peer.sock);
    drop(peer);
    // Bounded settle for the host's fail-closed busy→free transition; the
    // reconnect below is the assertion, and it has its own hard timeouts.
    // (Same explicit small delay pattern as the other reconnect tests.)
    std::thread::sleep(Duration::from_millis(300));
    // Host still serves a real client afterwards.
    let mut client = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut client);
    assert!(client.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_raw_tls_pipelined_token_and_initialize() {
    let g = lock_tests();
    // TLS record != application message: a peer may coalesce token+initialize
    // into ONE TLS write. The suffix after the token newline must survive
    // BOTH the auth split AND the ack: ack first, then the init response.
    let host = start_host(&g, &[]);
    let mut peer = TlsPeer::connect(&g, host.port, IO_TIMEOUT);
    let token = std::fs::read(g.assets.path("good/client.token")).unwrap();
    let init = serde_json::to_string(&serde_json::json!({"jsonrpc":"2.0","id":7,
        "method":"initialize","params":{"protocolVersion":"2024-11-05",
        "capabilities":{},"clientInfo":{"name":"pipelined","version":"0"}}}))
    .unwrap();
    let mut flight = token;
    flight.extend_from_slice(init.as_bytes());
    flight.push(b'\n');
    peer.send(&flight); // one writer() call → one coalesced TLS flight
    let ack = peer.read_line(IO_TIMEOUT);
    assert_eq!(ack, b"AUTH_OK\n", "pipelined auth ack: {ack:?}");
    let line = peer.read_line(IO_TIMEOUT);
    let r: serde_json::Value = serde_json::from_slice(&line)
        .unwrap_or_else(|e| panic!("pipelined init lost its response {e}: {line:?}"));
    assert_eq!(r["id"], serde_json::json!(7));
    assert!(r["result"]["protocolVersion"].is_string(), "init: {r}");
    peer.conn.send_close_notify();
    let _ = peer.conn.complete_io(&mut peer.sock);
}

#[test]
fn remote_client_fails_closed_without_auth_ack() {
    let g = lock_tests();
    // A server that completes TLS but NEVER sends the AUTH_OK ack must make
    // the client fail with a non-zero, bounded exit (never a silent success
    // and never a hang). The fixture CA signs the fake server's cert so the
    // failure is provably the MISSING ACK, not TLS verification.
    let (port, listener) = reserve_port();
    let cert_pem = std::fs::read(g.assets.path("good/server.pem")).unwrap();
    let key_pem = std::fs::read(g.assets.path("good/server.key")).unwrap();
    let server = std::thread::spawn(move || {
        let certs: Vec<rustls_pki_types::CertificateDer<'static>> =
            rustls_pki_types::pem::PemObject::pem_slice_iter(&cert_pem[..])
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        let key = rustls_pki_types::PrivateKeyDer::from_pem_slice(&key_pem[..]).unwrap();
        let config = std::sync::Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .unwrap(),
        );
        let (mut sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        sock.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        let mut conn = rustls::ServerConnection::new(config).unwrap();
        let started = Instant::now();
        while conn.is_handshaking() {
            assert!(
                started.elapsed() < IO_TIMEOUT,
                "fake server TLS handshake hung"
            );
            conn.complete_io(&mut sock).unwrap();
        }
        // Read the token line (and ignore it), then stay SILENT: no ack,
        // no close — the client must hit its own auth deadline and exit
        // non-zero well before this thread gives up.
        let mut buf = [0u8; 4096];
        while started.elapsed() < Duration::from_secs(20) {
            match conn.reader().read(&mut buf) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let _ = conn.complete_io(&mut sock);
                }
                Err(_) => break, // client gave up (its deadline fired)
            }
        }
    });
    let mut args = vec![
        "--connect".into(),
        format!("127.0.0.1:{port}"),
        "--ca-cert".into(),
        g.assets.path("good/ca.pem").to_string_lossy().into(),
        "--server-name".into(),
        "localhost".into(),
        "--token-file".into(),
        g.assets.path("good/client.token").to_string_lossy().into(),
    ];
    // Explicitly the normal client path; no special flags.
    args.shrink_to_fit();
    let started = Instant::now();
    let client = TestChild::spawn(&g.client_bin, &args);
    let status = client.wait(TEST_TIMEOUT);
    assert!(
        !status.success(),
        "client must fail when the server never sends AUTH_OK: {status}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "missing-ack failure must be bounded, took {:?}",
        started.elapsed()
    );
    let _ = server.join();
}

#[test]
fn remote_wrong_ca_refused() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let client = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "wrong/ca.pem"));
    let status = client.wait(TEST_TIMEOUT);
    assert!(!status.success(), "wrong CA must fail: {status}");
}

#[test]
fn remote_wrong_server_name_refused() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut args = client_args(&g, &host, "good/ca.pem");
    let idx = args.iter().position(|a| a == "--server-name").unwrap();
    args[idx + 1] = "not-the-server.example".into();
    let client = TestChild::spawn(&g.client_bin, &args);
    let status = client.wait(TEST_TIMEOUT);
    assert!(!status.success(), "wrong server name must fail: {status}");
}

#[test]
fn remote_plaintext_connection_refused() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    // Raw TCP garbage: TLS handshake must fail and NO desktop worker may
    // ever be created for this peer. The listener must stay alive.
    let mut s = TcpStream::connect(("127.0.0.1", host.port)).unwrap();
    s.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut buf = [0u8; 16];
    let r = s.read(&mut buf);
    // Either an immediate close or a TLS alert — never a protocol answer.
    match r {
        Ok(0) => {}
        Ok(n) => assert_eq!(
            buf[0], 0x15,
            "expected TLS alert record, got {buf:?} (n={n})"
        ),
        Err(_) => {}
    }
    drop(s);
    // Host still serves a real client afterwards.
    let mut client = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut client);
    let status = client.wait(TEST_TIMEOUT);
    assert!(status.success());
}

#[test]
fn remote_second_client_conflicts_at_open_then_reuses_connection() {
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

#[test]
fn remote_sequential_reconnect_gets_fresh_session() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut sessions = Vec::new();
    for i in 0..2u64 {
        let mut client = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
        mcp_handshake(&mut client);
        sessions.push(open_session(&mut client, 3 + i));
        let status = client.wait(TEST_TIMEOUT);
        assert!(status.success(), "client exit: {status}");
    }
    assert_eq!(sessions.len(), 2);
    assert!(
        sessions.iter().all(|s| !s.is_empty()),
        "sessions: {sessions:?}"
    );
}

#[test]
fn remote_eof_closes_client_and_host_stays_up() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut client = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut client);
    // stdin EOF: the client must shut the TLS session down properly and EXIT
    // on its own (this is the "no stdin thread hang" requirement).
    let status = client.wait(TEST_TIMEOUT);
    assert!(status.success(), "client did not exit cleanly: {status}");
    // Host must accept a brand-new connection afterwards.
    let mut c2 = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut c2);
    assert!(c2.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_unauthenticated_peer_gets_no_worker_then_host_serves() {
    let g = lock_tests();
    // A peer that completes TLS but presents a wrong token must not start a
    // desktop worker; the listener must survive. Verified behaviorally: the
    // host still serves a good client afterwards and the bad peer got cut.
    let host = start_host(&g, &[]);
    let bad = g.assets.create_file("bad-token", b"nope\n");
    let mut args = client_args(&g, &host, "good/ca.pem");
    let idx = args.iter().position(|a| a == "--token-file").unwrap();
    args[idx + 1] = bad.to_string_lossy().into();
    let badc = TestChild::spawn(&g.client_bin, &args);
    assert!(!badc.wait(TEST_TIMEOUT).success());
    let mut good = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut good);
    assert!(good.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_large_observe_frames_pass_unchanged() {
    let g = lock_tests();
    // Observe repeatedly and confirm each PNG decodes (backpressure path:
    // frames larger than one TCP segment / TLS record).
    let host = start_host(&g, &[]);
    let mut client = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut client);
    let session_id = open_session(&mut client, 3);
    for i in 0..3u64 {
        send_frame(
            &mut client,
            serde_json::json!({"jsonrpc":"2.0","id":100+i,"method":"tools/call",
                "params":{"name":"computer_observe","arguments":{"session_id":session_id}}}),
        );
        let r = read_response(&client);
        assert_eq!(r["id"], serde_json::json!(100 + i));
        expect_png_in_observe(&r);
    }
    assert!(client.wait(TEST_TIMEOUT).success());
}

#[test]
fn remote_host_rejects_invalid_arg_combinations() {
    let g = lock_tests();
    // Missing TLS material / mixing --remote-listen with --listen: fail closed.
    let cases: Vec<Vec<String>> = vec![
        vec!["--remote-listen".into(), "127.0.0.1:1".into()],
        vec![
            "--remote-listen".into(),
            "127.0.0.1:1".into(),
            "--tls-cert".into(),
            g.assets.path("good/server.pem").to_string_lossy().into(),
        ],
        vec![
            "--tls-cert".into(),
            g.assets.path("good/server.pem").to_string_lossy().into(),
            "--tls-key".into(),
            g.assets.path("good/server.key").to_string_lossy().into(),
            "--token-file".into(),
            g.assets.path("good/host.token").to_string_lossy().into(),
        ],
        vec![
            "--remote-listen".into(),
            "127.0.0.1:1".into(),
            "--tls-cert".into(),
            g.assets.path("good/server.pem").to_string_lossy().into(),
            "--tls-key".into(),
            g.assets.path("good/server.key").to_string_lossy().into(),
            "--token-file".into(),
            g.assets.path("good/host.token").to_string_lossy().into(),
            "--listen".into(),
            "127.0.0.1:2".into(),
        ],
    ];
    for mut args in cases {
        args.push("--mock-backend".into());
        let c = TestChild::spawn(&g.host_bin, &args);
        let status = c.wait(BINARY_TIMEOUT);
        assert!(
            !status.success(),
            "host accepted invalid args {args:?}: {status}"
        );
    }
}

#[test]
fn client_rejects_invalid_args_and_reports_failure() {
    let g = lock_tests();
    let cases: Vec<Vec<String>> = vec![
        vec!["--connect".into(), "127.0.0.1:1".into()],
        vec![
            "--ca-cert".into(),
            g.assets.path("good/ca.pem").to_string_lossy().into(),
            "--token-file".into(),
            g.assets.path("good/client.token").to_string_lossy().into(),
        ],
        vec![
            "--connect".into(),
            "127.0.0.1:1".into(),
            "--ca-cert".into(),
            g.assets.path("good/ca.pem").to_string_lossy().into(),
        ],
    ];
    for args in cases {
        let c = TestChild::spawn(&g.client_bin, &args);
        let status = c.wait(BINARY_TIMEOUT);
        assert!(
            !status.success(),
            "client accepted invalid args {args:?}: {status}"
        );
    }
}

#[test]
fn client_connect_failure_is_nonzero_and_bounded() {
    let g = lock_tests();
    // Nothing listens: bounded, non-zero exit (no hang, no silent success).
    let (port, _hold) = reserve_port();
    let args = vec![
        "--connect".into(),
        format!("127.0.0.1:{port}"),
        "--ca-cert".into(),
        g.assets.path("good/ca.pem").to_string_lossy().into(),
        "--server-name".into(),
        "localhost".into(),
        "--token-file".into(),
        g.assets.path("good/client.token").to_string_lossy().into(),
    ];
    let started = Instant::now();
    let c = TestChild::spawn(&g.client_bin, &args);
    let status = c.wait(BINARY_TIMEOUT);
    assert!(!status.success());
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "connect failure took too long: {:?}",
        started.elapsed()
    );
}
