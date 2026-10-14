// Shared black-box transport harness; included by integration test crates.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

const TEST_TIMEOUT: Duration = Duration::from_secs(60);
const IO_TIMEOUT: Duration = Duration::from_secs(15);
const BINARY_TIMEOUT: Duration = Duration::from_secs(30);

#[path = "remote_assets.rs"]
mod remote_assets;
#[path = "remote_bundle.rs"]
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
    stderr_lines: std::sync::mpsc::Receiver<String>,
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
        // Drain stderr continuously; startup is gated on the host's real log,
        // not a TCP probe (which itself consumes a transport slot).
        let (stderr_tx, stderr_lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr.unwrap()).lines() {
                match line {
                    Ok(line) => {
                        // Keep child diagnostics in the outer runner's full stderr.
                        eprintln!("[remote-test child] {line}");
                        let _ = stderr_tx.send(line);
                    }
                    Err(_) => return,
                }
            }
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
            stderr_lines,
        }
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
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut startup = Vec::new();
    loop {
        assert!(
            Instant::now() < deadline,
            "host startup deadline: {startup:?}"
        );
        let remaining = deadline.saturating_duration_since(Instant::now());
        let line = proc
            .stderr_lines
            .recv_timeout(remaining)
            .unwrap_or_else(|e| panic!("host startup failed ({e}): {startup:?}"));
        let ready = line.contains("remote TLS") && line.contains("listening on");
        startup.push(line);
        if ready {
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
    assert!(bytes.len() >= 8, "truncated PNG payload");
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
