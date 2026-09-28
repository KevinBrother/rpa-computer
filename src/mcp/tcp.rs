//! Loopback-only, token-authenticated TCP transport (Windows remote-test).
//!
//! Security properties:
//! - Binds to 127.0.0.1 only; any other bind address is rejected at CLI parse.
//! - The first newline-delimited line from a client must be the shared token
//!   read from `--token-file`; authentication precedes any JSON-RPC handling.
//! - Token comparison is constant-time; the token is never logged or printed.
//! - Auth has an ABSOLUTE wall-clock deadline (a slow-drip client cannot
//!   extend it byte by byte); frames are size-capped the same as stdio.
//! - The reader→main frame channel is BOUNDED: a flooding client fills it
//!   and OVERFLOW IS EXPLICIT — the reader cancels the runtime, the client
//!   gets a protocol error, and the connection is torn down; no accepted
//!   request is ever silently dropped behind a still-answering loop. EOF
//!   rides a separate control slot so it is never starved.
//! - The reader strictly parses every frame and applies ONLY validated
//!   control directly: a matching `notifications/cancelled`, a
//!   `computer_pause`/`computer_close` tool call, or EOF set the runtime
//!   cancel flag immediately, so control lands even while the main thread
//!   is blocked inside a stuck tool call (up to the call deadline).
//! - A single client controls the desktop at a time; extra concurrent
//!   connections are refused. On disconnect the session is cancelled and the
//!   runtime shut down.
//! - FATAL lifecycle: if a native call never returns (shutdown/call outcome
//!   unknown), the worker is QUARANTINED and NO further client is accepted —
//!   an abandoned native thread may still inject input, so a new session on
//!   the same worker (or a released writer lock) would allow two live
//!   writers. The listener exits and the process must be restarted instead.
//! - All diagnostics go to stderr, never to the client stream.

use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::mcp::ingress::{self, Control, FrameKind, ReaderIngress};
use crate::mcp::jsonrpc::{self, FrameReader};
use crate::mcp::server::{Action, McpService};
use crate::mcp::worker::Worker;

/// Max time a client may take to present the token after connecting.
/// ABSOLUTE: anchored at accept time; per-byte reads use only the REMAINING
/// budget, so a byte-dripping client cannot extend the deadline.
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
/// Max token line length; also caps the file we read.
pub const MAX_TOKEN_BYTES: usize = 4096;
/// Bound on reader→main queued frames per client. Overflow is EXPLICIT
/// (cancel + refuse + teardown): bounded memory, never an unbounded backlog,
/// never a silently dropped accepted request.
const MAX_QUEUED_FRAMES: usize = 64;

#[derive(Debug)]
pub enum TcpError {
    BindRefused(String),
    Io(std::io::Error),
    Token(String),
    /// The worker faulted (an abandoned native thread may still be live);
    /// the listener refuses all further clients and the host must restart.
    WorkerFaulted,
}

impl std::fmt::Display for TcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TcpError::BindRefused(m) => write!(f, "refusing to listen: {m}"),
            TcpError::Io(e) => write!(f, "tcp transport I/O error: {e}"),
            TcpError::Token(m) => write!(f, "token error: {m}"),
            TcpError::WorkerFaulted => write!(
                f,
                "worker faulted (a native call never returned; outcome unknown); \
                 listener quarantined, restart the host"
            ),
        }
    }
}

impl From<std::io::Error> for TcpError {
    fn from(e: std::io::Error) -> Self {
        TcpError::Io(e)
    }
}

/// Validate that `addr` is strictly loopback (IPv4 127.0.0.1 or ::1).
pub fn ensure_loopback(addr: &str) -> Result<std::net::SocketAddr, TcpError> {
    let parsed: std::net::SocketAddr = addr
        .parse()
        .map_err(|_| TcpError::BindRefused(format!("{addr} is not a valid host:port")))?;
    let ok = match parsed.ip() {
        std::net::IpAddr::V4(v4) => v4.is_loopback(),
        std::net::IpAddr::V6(v6) => v6.is_loopback(),
    };
    if ok {
        Ok(parsed)
    } else {
        Err(TcpError::BindRefused(format!(
            "{addr} is not loopback; refusing to expose an unauthenticated LAN listener"
        )))
    }
}

/// Load and normalize the token from a file: UTF-8, first line, no
/// whitespace padding. The token is never logged.
pub fn load_token(path: &std::path::Path) -> Result<Vec<u8>, TcpError> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| TcpError::Token(format!("cannot open {}: {e}", path.display())))?;
    let mut buf = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_TOKEN_BYTES as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| TcpError::Token(format!("cannot read {}: {e}", path.display())))?;
    if buf.len() > MAX_TOKEN_BYTES {
        return Err(TcpError::Token("token file exceeds size limit".into()));
    }
    let line = buf
        .split(|b| *b == b'\n')
        .next()
        .unwrap_or(&[])
        .iter()
        .copied()
        .filter(|b| *b != b'\r')
        .collect::<Vec<u8>>();
    if line.is_empty() {
        return Err(TcpError::Token("token file is empty".into()));
    }
    Ok(line)
}

/// Constant-time byte comparison (length difference leaks only length).
pub fn token_matches(expected: &[u8], presented: &[u8]) -> bool {
    let max = expected.len().max(presented.len());
    let mut diff = expected.len() ^ presented.len();
    for i in 0..max {
        let a = expected.get(i).copied().unwrap_or(0);
        let b = presented.get(i).copied().unwrap_or(0);
        diff |= (a ^ b) as usize;
    }
    diff == 0
}

/// Read the auth line (bounded, ABSOLUTE-deadline enforced) and verify the
/// token. Each per-byte read gets only the time REMAINING before the
/// absolute deadline, so slow-drip input cannot extend authentication.
fn authenticate(stream: &mut TcpStream, expected: &[u8]) -> Result<(), TcpError> {
    let deadline = Instant::now() + AUTH_TIMEOUT;
    let mut buf = Vec::with_capacity(128);
    let mut byte = [0u8; 1];
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(TcpError::Token("authentication timeout".into()));
        }
        stream.set_read_timeout(Some(deadline - now))?;
        let n = stream.read(&mut byte).map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut
            {
                TcpError::Token("authentication timeout".into())
            } else {
                TcpError::Io(e)
            }
        })?;
        if n == 0 {
            return Err(TcpError::Token(
                "connection closed before authentication".into(),
            ));
        }
        if byte[0] == b'\n' {
            break;
        }
        if byte[0] != b'\r' {
            buf.push(byte[0]);
        }
        if buf.len() > MAX_TOKEN_BYTES {
            return Err(TcpError::Token("authentication line too long".into()));
        }
    }
    stream.set_read_timeout(None)?;
    if token_matches(expected, &buf) {
        Ok(())
    } else {
        Err(TcpError::Token("invalid token".into()))
    }
}

/// Serve the loopback TCP host until `shutdown_flag` is set, an
/// unrecoverable listener error occurs, or the worker faults (quarantine).
/// One authenticated client at a time.
pub fn run(
    worker: &Worker,
    version: &str,
    bind: std::net::SocketAddr,
    token: Vec<u8>,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<(), TcpError> {
    let listener = TcpListener::bind(bind)?;
    listener.set_nonblocking(true)?;
    eprintln!(
        "[computer-host] loopback test transport listening on {} (single client, token auth)",
        bind
    );

    let client_busy = Arc::new(Mutex::new(false));
    let token = Arc::new(token);

    while !shutdown_flag.load(Ordering::SeqCst) {
        // Quarantine: once the worker is faulted, an abandoned native
        // thread may still be live. NO new client may ever be served and
        // the writer lock must stay held — exit so the host is restarted.
        if worker.is_faulted() {
            eprintln!(
                "[computer-host] worker faulted; refusing further clients and exiting \
                 (quarantine: an abandoned native thread may still be live)"
            );
            return Err(TcpError::WorkerFaulted);
        }
        match listener.accept() {
            Ok((mut stream, peer)) => {
                // Windows semantics: a socket accepted from a NON-blocking
                // listener INHERITS non-blocking mode, and per-socket
                // SO_RCVTIMEO is ignored there — read() returns WouldBlock
                // instantly, so auth sees a bogus "timeout" and the frame
                // reader sees EOF. Force blocking mode on the accepted
                // stream before any read; timeouts are enforced through
                // set_read_timeout (authenticate's absolute deadline).
                if let Err(e) = stream.set_nonblocking(false) {
                    eprintln!(
                        "[computer-host] cannot set accepted stream blocking for {peer}: {e}"
                    );
                    continue;
                }
                let mut busy = client_busy.lock().unwrap();
                if *busy {
                    eprintln!(
                        "[computer-host] refusing {peer}: a client already controls the desktop"
                    );
                    drop(busy);
                    continue; // dropped stream => immediate close
                }
                *busy = true;
                drop(busy);

                if let Err(e) = authenticate(&mut stream, &token) {
                    eprintln!("[computer-host] auth rejected for {peer}: {e}");
                    *client_busy.lock().unwrap() = false;
                    continue;
                }
                eprintln!("[computer-host] client {peer} authenticated");

                serve_client(worker, version, stream, &shutdown_flag);

                // Disconnect semantics: cancel everything and shut the
                // runtime down so a reconnect cannot continue a half-known
                // input sequence.
                let cancel = worker.cancel_handle();
                cancel.cancel();
                worker.shutdown();
                *client_busy.lock().unwrap() = false;
                if worker.is_faulted() {
                    eprintln!(
                        "[computer-host] client {peer} disconnected with worker faulted; \
                         quarantining listener (restart the host)"
                    );
                    return Err(TcpError::WorkerFaulted);
                }
                // Clean disconnect: this worker has completed its runtime
                // shutdown, so it cannot serve another client. Stop the
                // listener rather than loop on a shut-down worker — the
                // host must be restarted per connection (documented for
                // the interactive Windows test path).
                eprintln!(
                    "[computer-host] client {peer} disconnected; session cancelled and runtime \
                     shut down; listener exiting (one client per host process)"
                );
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(TcpError::Io(e)),
        }
    }
    Ok(())
}

enum ClientEvent {
    Frame(ingress::Frame),
    Eof,
}

/// Run one authenticated client connection: frames in, frames out.
fn serve_client(
    worker: &Worker,
    version: &str,
    stream: TcpStream,
    shutdown_flag: &Arc<AtomicBool>,
) {
    let reader = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[computer-host] cannot clone client stream: {e}");
            return;
        }
    };
    let mut writer = stream;

    // Bounded frame channel + control slot for EOF/overflow (same shape as
    // stdio — the reader side IS the shared ingress path).
    let (tx, rx) = sync_channel::<ClientEvent>(MAX_QUEUED_FRAMES);
    let (ctrl_tx, ctrl_rx) = channel::<ClientEvent>();
    // When the main loop REFUSES the connection (overflow), the reader must
    // stop reading so its socket clone is dropped: this shuts the clone
    // down, which makes the peer's reads return EOF once the main side
    // closes its half. Without it the cloned reader would survive the
    // refused client and keep cancelling a later session.
    let refused = Arc::new(AtomicBool::new(false));
    {
        let cancel = worker.cancel_handle();
        let active = worker.active_request();
        let gate = worker.transport_gate();
        let generation = worker.request_generation();
        let reader_refused = Arc::clone(&refused);
        std::thread::Builder::new()
            .name("computer-tcp-reader".into())
            .spawn(move || {
                let fr = FrameReader::new(BufReader::new(reader));
                let ingress_gate = ReaderIngress::new();
                let _ = fr.run(
                    |line| {
                        if ingress_gate.is_refused() {
                            // Connection already refused: nothing more is
                            // enqueued, and the read loop ENDS so the socket
                            // clone is dropped (peer sees EOF; no surviving
                            // reader behind a refused client).
                            reader_refused.store(true, Ordering::SeqCst);
                            return;
                        }
                        // Sanitize FIRST (erase forged __ch_* members), then
                        // admission BEFORE side effects: a refused duplicate
                        // pause/close applies no control below.
                        let frame = ingress::Frame::sanitize(line);
                        let frame = ingress::stamped_frame(frame, &generation, &gate);
                        ingress::apply_direct_cancel(&frame, &cancel, &active, &gate);
                        if tx.try_send(ClientEvent::Frame(frame)).is_err() {
                            // Overflow is explicit and happens ONCE (latched
                            // for BOTH normal and oversized failures).
                            ingress_gate.refuse_once(&cancel, |m| {
                                let _ = ctrl_tx.send(ClientEvent::Frame(m));
                            });
                            eprintln!(
                                "[computer-host] inbound frame queue full ({MAX_QUEUED_FRAMES}); \
                                 cancelling and refusing the client"
                            );
                        }
                    },
                    |_err| {
                        // Oversized frames share the ONE-SHOT refusal latch:
                        // one marker total, never an unbounded stream onto
                        // the control slot after the main loop moved on.
                        if ingress_gate.is_refused() {
                            reader_refused.store(true, Ordering::SeqCst);
                            return;
                        }
                        if tx
                            .try_send(ClientEvent::Frame(ingress::Frame::oversized_control()))
                            .is_err()
                        {
                            ingress_gate.refuse_once(&cancel, |m| {
                                let _ = ctrl_tx.send(ClientEvent::Frame(m));
                            });
                            eprintln!(
                                "[computer-host] oversized frame could not be queued \
                                 ({MAX_QUEUED_FRAMES} pending); refusing the client"
                            );
                        }
                    },
                );
                // EOF cancels directly on the reader too, then posts the
                // control marker.
                cancel.cancel();
                let _ = ctrl_tx.send(ClientEvent::Eof);
            })
            .expect("spawn tcp reader");
    }

    let mut service = McpService::new(worker, version.to_string());

    loop {
        if shutdown_flag.load(Ordering::SeqCst) {
            break;
        }
        match ctrl_rx.try_recv() {
            Ok(ClientEvent::Eof) => {
                service.handle_eof();
                break;
            }
            Ok(ClientEvent::Frame(m)) if m.kind == FrameKind::Control(Control::Overflow) => {
                // Explicit refusal: answer once, then REALLY close the
                // connection (both directions) — the session is already
                // cancelled and no further inbound frame is dispatched.
                let _ = writer.write_all(
                    jsonrpc::error_response(
                        serde_json::Value::Null,
                        jsonrpc::SERVER_BUSY,
                        "inbound frame backlog overflowed; connection refused",
                    )
                    .as_bytes(),
                );
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
                let _ = writer.shutdown(std::net::Shutdown::Both);
                eprintln!(
                    "[computer-host] tcp client refused after backlog overflow; socket closed"
                );
                break;
            }
            Ok(_) => {}
            Err(_) => {}
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(ClientEvent::Frame(frame)) => {
                let (frames, action) = if frame.kind == FrameKind::Control(Control::Oversized) {
                    (
                        vec![jsonrpc::error_response(
                            serde_json::Value::Null,
                            jsonrpc::INVALID_REQUEST,
                            "frame exceeds maximum size",
                        )],
                        Action::Continue,
                    )
                } else {
                    service.handle_frame(frame)
                };
                let mut broken = false;
                for f in frames {
                    if writer.write_all(f.as_bytes()).is_err()
                        || writer.write_all(b"\n").is_err()
                        || writer.flush().is_err()
                    {
                        broken = true;
                        break;
                    }
                }
                if broken || matches!(action, Action::Stop | Action::Fatal) {
                    if matches!(action, Action::Fatal) {
                        eprintln!(
                            "[computer-host] FATAL: runtime worker faulted; tearing down client, \
                             restart the host"
                        );
                    }
                    break;
                }
            }
            Ok(ClientEvent::Eof) => {
                service.handle_eof();
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_validation() {
        assert!(ensure_loopback("127.0.0.1:9000").is_ok());
        assert!(ensure_loopback("[::1]:9000").is_ok());
        assert!(ensure_loopback("0.0.0.0:9000").is_err());
        assert!(ensure_loopback("192.168.1.10:9000").is_err());
        assert!(ensure_loopback("8.8.8.8:53").is_err());
        assert!(ensure_loopback("not-an-addr").is_err());
    }

    #[test]
    fn constant_time_token_compare() {
        assert!(token_matches(b"secret", b"secret"));
        assert!(!token_matches(b"secret", b"secreu"));
        assert!(!token_matches(b"secret", b"secret2"));
        assert!(!token_matches(b"", b"x"));
    }

    #[test]
    fn load_token_reads_first_line() {
        let dir = std::env::temp_dir().join(format!("ch-token-test-{}", std::process::id()));
        std::fs::write(&dir, b"abc123\r\nextra\n").unwrap();
        let token = load_token(&dir).unwrap();
        assert_eq!(token, b"abc123");
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn load_token_rejects_empty() {
        let dir = std::env::temp_dir().join(format!("ch-token-empty-{}", std::process::id()));
        std::fs::write(&dir, b"\n").unwrap();
        assert!(load_token(&dir).is_err());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn authenticate_accepts_correct_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            authenticate(&mut s, b"tok")
        });
        let mut client = TcpStream::connect(addr).unwrap();
        client.write_all(b"tok\r\n").unwrap();
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn authenticate_rejects_wrong_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            authenticate(&mut s, b"tok")
        });
        let mut client = TcpStream::connect(addr).unwrap();
        client.write_all(b"wrong\n").unwrap();
        let result = server.join().unwrap();
        assert!(matches!(result, Err(TcpError::Token(_))));
    }

    #[test]
    fn authenticate_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            authenticate(&mut s, b"tok")
        });
        let _client = TcpStream::connect(addr).unwrap(); // sends nothing
        let started = Instant::now();
        let result = server.join().unwrap();
        assert!(matches!(result, Err(TcpError::Token(_))));
        assert!(started.elapsed() >= AUTH_TIMEOUT);
        assert!(started.elapsed() < AUTH_TIMEOUT + Duration::from_secs(2));
    }

    #[test]
    fn authenticate_slow_drip_cannot_extend_absolute_deadline() {
        // A client that drips one byte just under the per-read timeout must
        // still be cut off at the ABSOLUTE deadline, not get a fresh
        // AUTH_TIMEOUT per byte.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            authenticate(&mut s, b"tok")
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let started = Instant::now();
        // Drip bytes until the server cuts us off (or a sanity cap).
        let drip = Duration::from_millis(500);
        loop {
            if client.write_all(b"x").is_err() {
                break; // server closed the connection
            }
            if started.elapsed() > AUTH_TIMEOUT + Duration::from_secs(4) {
                break;
            }
            std::thread::sleep(drip);
        }
        let result = server.join().unwrap();
        assert!(matches!(result, Err(TcpError::Token(_))));
        // Absolute deadline: total auth time must be ~AUTH_TIMEOUT, not
        // drips * AUTH_TIMEOUT.
        assert!(
            started.elapsed() < AUTH_TIMEOUT + Duration::from_secs(4),
            "auth must have an absolute deadline; took {:?}",
            started.elapsed()
        );
    }
}
