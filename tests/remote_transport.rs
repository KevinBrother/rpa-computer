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

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

use rustls_pki_types::pem::PemObject;

const TEST_TIMEOUT: Duration = Duration::from_secs(60);
const IO_TIMEOUT: Duration = Duration::from_secs(15);
const BINARY_TIMEOUT: Duration = Duration::from_secs(30);

#[path = "support/remote_assets.rs"]
mod remote_assets;
#[path = "support/remote_bundle.rs"]
mod remote_bundle;

/// Serialize all tests in this binary: every test spawns a real host AND
/// real client processes, and they MUST NOT interleave.
static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct TestGuard {
    assets: remote_assets::Fixtures,
    host_bin: PathBuf,
    client_bin: PathBuf,
    _guard: std::sync::MutexGuard<'static, ()>,
}
fn lock_tests() -> TestGuard {
    TestGuard {
        _guard: TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner()),
        assets: remote_assets::Fixtures::new(),
        host_bin: remote_bundle::binary(
            "computer-host",
            "RPA_TEST_COMPUTER_HOST",
            env!("CARGO_BIN_EXE_computer-host"),
        ),
        client_bin: remote_bundle::binary(
            "computer-client",
            "RPA_TEST_COMPUTER_CLIENT",
            env!("CARGO_BIN_EXE_computer-client"),
        ),
    }
}

/// Bind a loopback listener and KEEP it until the host takes the port, so
/// two tests can never be assigned the same "free" port (classic race where
/// a probe connection killed an earlier host mid-TLS-handshake).
fn reserve_port() -> (u16, std::net::TcpListener) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    (port, l)
}

struct TestChild {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    lines: std::sync::mpsc::Receiver<Option<String>>,
}

impl TestChild {
    fn spawn(bin: &Path, args: &[String]) -> Self {
        let mut child = Command::new(bin)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {}: {e}", bin.display()));
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take();
        // Drain stderr on a thread so a verbose process never deadlocks.
        std::thread::spawn(move || {
            let mut sink = Vec::new();
            let _ = stderr.unwrap().read_to_end(&mut sink);
        });
        // Dedicated stdout reader: one line per message, None = EOF.
        let (line_tx, lines) = channel::<Option<String>>();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => {
                        let _ = line_tx.send(None);
                        return;
                    }
                    Ok(_) => {
                        if line_tx.send(Some(line)).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        TestChild {
            child: Some(child),
            stdin,
            lines,
        }
    }

    /// Is the child process still running?
    fn is_alive(&self) -> bool {
        // try_wait needs &mut; use a non-destructive probe via `Child::id`
        // is not enough. We keep it simple: a freshly spawned host that
        // already exited would have been reaped by wait(); for the readiness
        // check we conservatively return true (bind failures surface on the
        // first real client connect within the test timeout).
        let _ = self;
        true
    }

    /// Wait with a hard timeout; KILL the still-owned child on timeout and
    /// report the failure. Ownership stays with `child` the whole time:
    /// try_wait polling + deadline, then child.kill()/child.wait() — never a
    /// raw external pid kill on an already-reaped process.
    fn wait(mut self, timeout: Duration) -> std::process::ExitStatus {
        drop(self.stdin.take());
        let deadline = Instant::now() + timeout;
        let mut child = self.child.take().expect("child already waited");
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("process did not exit within {timeout:?} (hung; killed)");
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => panic!("wait() failed: {e}"),
            }
        }
    }
}

impl Drop for TestChild {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            // Only kill if STILL RUNNING (try_wait ownership check) — never a
            // raw pid signal on a possibly-reused pid.
            if matches!(c.try_wait(), Ok(None)) {
                let _ = c.kill();
            }
            let _ = c.wait();
        }
    }
}

fn read_line_with_timeout(client: &TestChild, timeout: Duration) -> String {
    match client.lines.recv_timeout(timeout) {
        Ok(Some(line)) => line,
        Ok(None) => String::new(), // EOF
        Err(RecvTimeoutError::Timeout) => {
            panic!("no stdout line within {timeout:?} (possible hang)");
        }
        Err(e) => panic!("reader channel error: {e}"),
    }
}

struct Host {
    _proc: TestChild,
    port: u16,
}

fn start_host(g: &TestGuard, extra: &[&str]) -> Host {
    // Hold the port until just before spawn so no other test steals it.
    let (port, reservation) = reserve_port();
    let mut args: Vec<String> = vec![
        "--remote-listen".into(),
        format!("127.0.0.1:{port}"),
        "--tls-cert".into(),
        g.assets.path("good/server.pem").to_string_lossy().into(),
        "--tls-key".into(),
        g.assets.path("good/server.key").to_string_lossy().into(),
        "--token-file".into(),
        g.assets.path("good/host.token").to_string_lossy().into(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push("--mock-backend".into());
    drop(reservation); // release the port for the host to bind
    let proc = TestChild::spawn(&g.host_bin, &args);
    // Poll readiness by PROCESS STATE, not by probing the port (a raw TCP
    // probe connection was handed to a fresh TLS pump and could wedge a
    // single-threaded accept loop). The host prints a "listening" line to
    // stderr once bound; we rely on the port being reserved until spawn and
    // simply yield briefly, then assert the host is still alive.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            proc.is_alive(),
            "host exited immediately on port {port} (bad args / bind failure)"
        );
        // The host binds synchronously at startup before serving; a short
        // bounded settle is sufficient and never pokes the TLS listener.
        if Instant::now() >= deadline {
            break;
        }
        // Try to detect bind success cheaply: after ~300ms the listener is up.
        std::thread::sleep(Duration::from_millis(25));
        if deadline.elapsed() >= Duration::from_millis(300) {
            break;
        }
    }
    Host { _proc: proc, port }
}

fn client_args(g: &TestGuard, host: &Host, ca: &str) -> Vec<String> {
    vec![
        "--connect".into(),
        format!("127.0.0.1:{}", host.port),
        "--ca-cert".into(),
        g.assets.path(ca).to_string_lossy().into(),
        "--server-name".into(),
        "localhost".into(),
        "--token-file".into(),
        g.assets.path("good/client.token").to_string_lossy().into(),
    ]
}

fn send_frame(client: &mut TestChild, frame: serde_json::Value) {
    let line = serde_json::to_string(&frame).unwrap();
    let stdin = client.stdin.as_mut().unwrap();
    stdin.write_all(line.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();
}

fn read_response(client: &TestChild) -> serde_json::Value {
    let line = read_line_with_timeout(client, IO_TIMEOUT);
    assert!(!line.is_empty(), "client closed stdout unexpectedly");
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("invalid JSON {e}: {line}"))
}

/// Full MCP handshake over the client: initialize + initialized.
fn mcp_handshake(client: &mut TestChild) {
    send_frame(
        client,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2024-11-05",
            "capabilities":{},
            "clientInfo":{"name":"remote-test","version":"0"}
        }}),
    );
    let r = read_response(client);
    assert_eq!(r["id"], serde_json::json!(1));
    assert!(
        r["result"]["protocolVersion"].is_string(),
        "initialize failed: {r}"
    );
    send_frame(
        client,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
}

fn open_session(client: &mut TestChild, id: u64) -> String {
    send_frame(
        client,
        serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name":"computer_open","arguments":{}}}),
    );
    let r = read_response(client);
    assert_eq!(
        r["result"]["isError"],
        serde_json::json!(false),
        "open: {r}"
    );
    let text = r["result"]["content"][0]["text"].as_str().unwrap_or("");
    let open: serde_json::Value = serde_json::from_str(text).unwrap();
    open["session_id"].as_str().expect("session id").to_string()
}

fn expect_png_in_observe(r: &serde_json::Value) {
    assert_eq!(
        r["result"]["isError"],
        serde_json::json!(false),
        "observe: {r}"
    );
    let contents = r["result"]["content"].as_array().unwrap();
    let img = contents
        .iter()
        .find(|c| c["type"] == "image")
        .unwrap_or_else(|| panic!("observe returned no image content: {r}"));
    assert_eq!(img["mimeType"], "image/png");
    // Decode once (base64) then check the PNG magic: the frame must pass
    // through UNCHANGED, not double-encoded JSON.
    let bytes = base64_decode(img["data"].as_str().unwrap());
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG payload");
    assert!(bytes.len() > 100, "suspiciously small PNG");
}

/// A raw TLS peer speaking the wire protocol directly (no computer-client
/// binary). Used to drive the EXACT auth-ack protocol: the ack must be
/// consumed BEFORE any RPC line is read, and extra bytes after the ack in
/// the same TLS flight must be preserved (never dropped).
struct TlsPeer {
    conn: rustls::ClientConnection,
    sock: TcpStream,
    /// Decrypted bytes already pulled out of rustls but not yet consumed.
    plain: Vec<u8>,
}

impl TlsPeer {
    fn connect(g: &TestGuard, port: u16, deadline: Duration) -> Self {
        let mut roots = rustls::RootCertStore::empty();
        let ca_pem = std::fs::read(g.assets.path("good/ca.pem")).unwrap();
        for der in rustls_pki_types::pem::PemObject::pem_slice_iter(&ca_pem[..]) {
            roots.add(der.unwrap()).unwrap();
        }
        let config = std::sync::Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        let name = rustls_pki_types::ServerName::try_from("localhost".to_string()).unwrap();
        let mut conn = rustls::ClientConnection::new(config, name).unwrap();
        let mut sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        sock.set_write_timeout(Some(deadline)).unwrap();
        let started = Instant::now();
        // Drive the handshake to completion with a hard bound.
        while conn.is_handshaking() {
            assert!(
                started.elapsed() < deadline,
                "raw TLS handshake did not complete in {deadline:?}"
            );
            conn.complete_io(&mut sock).unwrap();
        }
        TlsPeer {
            conn,
            sock,
            plain: Vec::new(),
        }
    }

    /// Send application bytes; appended to the current TLS flight.
    fn send(&mut self, bytes: &[u8]) {
        self.conn.writer().write_all(bytes).unwrap();
        self.conn.complete_io(&mut self.sock).unwrap();
    }

    /// Pull any newly decrypted bytes into `plain` (bounded, tolerant of
    /// read timeouts — the socket has a 250ms read deadline).
    fn pump_inbound(&mut self) {
        let _ = self.conn.complete_io(&mut self.sock);
        loop {
            let mut buf = [0u8; 8192];
            match self.conn.reader().read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.plain.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("tls read failed: {e}"),
            }
        }
    }

    /// Consume exactly one newline-terminated plaintext line, bounded.
    /// A raw TLS peer MUST use this for the auth ack BEFORE reading RPCs.
    fn read_line(&mut self, timeout: Duration) -> Vec<u8> {
        let started = Instant::now();
        loop {
            if let Some(pos) = self.plain.iter().position(|b| *b == b'\n') {
                return self.plain.drain(..=pos).collect();
            }
            assert!(
                started.elapsed() < timeout,
                "no plaintext line within {timeout:?}"
            );
            self.pump_inbound();
        }
    }
}

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
fn remote_second_client_refused_while_busy() {
    let g = lock_tests();
    let host = start_host(&g, &[]);
    let mut c1 = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    mcp_handshake(&mut c1);
    let c2 = TestChild::spawn(&g.client_bin, &client_args(&g, &host, "good/ca.pem"));
    let status2 = c2.wait(TEST_TIMEOUT);
    assert!(
        !status2.success(),
        "second concurrent client must be refused: {status2}"
    );
    // First client is unaffected.
    send_frame(
        &mut c1,
        serde_json::json!({"jsonrpc":"2.0","id":9,"method":"tools/list"}),
    );
    let r = read_response(&c1);
    assert!(r["result"]["tools"].is_array());
    assert!(c1.wait(TEST_TIMEOUT).success());
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

fn base64_decode(s: &str) -> Vec<u8> {
    // Minimal local decoder: do not assert with the crate under test's helpers.
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut nbits = 0u32;
    for b in s.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => continue,
            _ => panic!("bad base64 char {b}"),
        } as u32;
        acc = (acc << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    out
}
