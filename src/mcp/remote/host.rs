//! Host-side TLS supervisor.
//!
//! Lifecycle contract (per the approved design):
//! - The listener NEVER stops for a bad/untrusted/busy peer: TLS failures,
//!   auth failures and extra concurrent clients close THAT connection only.
//! - Authentication (TLS handshake + token line) has ONE absolute 5s
//!   deadline; the token line is capped at 4096 bytes and compared in
//!   constant time. The token is sent only AFTER the TLS handshake and is
//!   never logged. On success the host answers with the literal
//!   `AUTH_OK\n` line BEFORE any MCP data or child output, so a client can
//!   tell "authenticated" apart from "silently dropped"; a REJECTED token
//!   closes with NO ack and the presented bytes are never echoed or logged.
//!   A TLS record is NOT an application message: any bytes after the token
//!   newline in the SAME chunk are preserved and fed FIRST into the bridge
//!   (the client may pipeline token+initialize in one TLS write).
//! - NO desktop worker exists before authentication: only after a valid
//!   token does the supervisor spawn the SAME current executable in its
//!   existing stdio mode (`current_exe() [--mock-backend]`), which builds
//!   the native worker with all existing cancel/watchdog/lock semantics.
//! - Exactly one control connection at a time: accept() runs on the main
//!   thread; each accepted connection is handled on its OWN worker thread.
//!   While a session is active, additional connections are accepted and
//!   closed IMMEDIATELY (never queued to become a later controlling client).
//!   The listener is marked idle ONLY after the active worker has reaped
//!   its child and torn down its pump.
//! - Fresh child per connection; the next child is spawned only after the
//!   previous one is FULLY reaped (never two live workers).
//! - Teardown: network EOF/TLS error/overflow closes the child's stdin so
//!   the native runtime cleans up; the child gets a bounded exit window,
//!   then ONLY this owned child is killed and reaped. A child that cannot
//!   be killed/reaped QUARANTINES the listener (fail closed — never claim
//!   safety and reuse).

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::{Connection, ServerConnection};

use crate::mcp::tcp::{load_token, token_matches, TcpError, AUTH_TIMEOUT, MAX_TOKEN_BYTES};

use super::pump::{run_pump, wait_join, Outcome, PumpEvent, PumpHandle, CHUNK_BYTES};

/// Bound on decrypted inbound chunks queued for line assembly.
const INBOUND_CHUNKS: usize = 32;
/// Bound on plaintext chunks queued network←child.
const OUTBOUND_CHUNKS: usize = 64;
/// How long the pump drains after the child closes.
const PUMP_DRAIN: Duration = Duration::from_secs(3);
/// Grace period for the owned child to exit after its stdin closes.
const CHILD_EXIT_GRACE: Duration = Duration::from_secs(5);
/// Bounded join for helper threads on teardown.
const THREAD_JOIN: Duration = Duration::from_secs(3);

pub struct RemoteArgs {
    pub listen: std::net::SocketAddr,
    pub tls_cert: PathBuf,
    pub tls_key: PathBuf,
    pub token_file: PathBuf,
    pub log_file: Option<PathBuf>,
    pub mock_backend: bool,
    pub feedback: crate::feedback::FeedbackConfig,
}

/// Outcome of one connection-handler worker thread.
enum WorkerResult {
    /// No child was ever spawned, or the owned child was reaped cleanly.
    Done,
    /// The owned child could not be killed/reaped: the listener must stop.
    Quarantine(String),
}

/// Serve the TLS remote mode forever (until `shutdown_flag` or fatal error).
pub fn run(args: RemoteArgs, shutdown_flag: Arc<AtomicBool>) -> Result<(), TcpError> {
    let config = super::tls::server_config(&args.tls_cert, &args.tls_key)
        .map_err(|e| TcpError::Token(format!("tls config: {e}")))?;
    let token = Arc::new(load_token(&args.token_file)?);
    let listener = TcpListener::bind(args.listen)?;
    listener.set_nonblocking(true)?;
    eprintln!(
        "[computer-host] remote TLS transport listening on {} \
         (single control client, TLS + token auth; fresh stdio worker per session)",
        args.listen
    );

    // Exactly one active session. The handler runs on its own thread so the
    // accept loop keeps refusing extra clients IMMEDIATELY while a session
    // is active — a queued backlog connection can never become a later
    // controlling client.
    let mut active: Option<std::thread::JoinHandle<WorkerResult>> = None;

    while !shutdown_flag.load(Ordering::SeqCst) {
        // Reap a finished worker BEFORE accepting anything new: the listener
        // is idle only once the previous child is fully reaped.
        if let Some(h) = &active {
            if h.is_finished() {
                match active.take().expect("checked").join() {
                    Ok(WorkerResult::Done) => {}
                    Ok(WorkerResult::Quarantine(m)) => {
                        return Err(TcpError::Token(format!(
                            "worker child unrecoverable ({m}); refusing to keep listening"
                        )));
                    }
                    Err(_) => {
                        return Err(TcpError::Token(
                            "connection worker panicked; refusing to keep listening".into(),
                        ));
                    }
                }
            }
        }

        match listener.accept() {
            Ok((sock, peer)) => {
                if active.is_some() {
                    // Busy: close the extra connection AT ONCE.
                    drop(sock);
                    continue;
                }
                let cfg = Arc::clone(&config);
                let tok = Arc::clone(&token);
                let flag = Arc::clone(&shutdown_flag);
                let mock = args.mock_backend;
                let feedback = args.feedback.clone();
                active = Some(
                    std::thread::Builder::new()
                        .name("computer-remote-session".into())
                        .spawn(move || {
                            serve_connection(
                                sock,
                                peer.to_string(),
                                &cfg,
                                &tok,
                                mock,
                                &feedback,
                                &flag,
                            )
                        })
                        .map_err(TcpError::Io)?,
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(TcpError::Io(e)),
        }
    }

    // Shutdown: stop the ACTIVE session too, bounded join; never lose its
    // child. serve_connection always reaps its owned child before returning.
    if shutdown_flag.load(Ordering::SeqCst) {
        if let Some(h) = active.take() {
            let _ = wait_join(h, CHILD_EXIT_GRACE + PUMP_DRAIN + Duration::from_secs(5));
        }
    }
    Ok(())
}

/// One TCP connection: TLS handshake + token auth, then bridge to a fresh
/// owned stdio child. Returns with the child reaped and the socket dead.
fn serve_connection(
    sock: TcpStream,
    peer: String,
    config: &Arc<rustls::ServerConfig>,
    token: &[u8],
    mock_backend: bool,
    feedback: &crate::feedback::FeedbackConfig,
    shutdown_flag: &Arc<AtomicBool>,
) -> WorkerResult {
    let server = match ServerConnection::new(Arc::clone(config)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[computer-host] cannot create TLS server state for {peer}: {e}");
            return WorkerResult::Done;
        }
    };
    let mut pump = run_pump(
        Connection::Server(server),
        sock,
        INBOUND_CHUNKS,
        OUTBOUND_CHUNKS,
        PUMP_DRAIN,
    );

    let prefix = match authenticate(&pump, token) {
        Ok(p) => p,
        Err(e) => {
            // Untrusted/broken peer: close THIS connection only, with NO
            // acknowledgement and without echoing the presented bytes. No
            // worker was ever created (spawn happens strictly after this).
            eprintln!("[computer-host] remote auth rejected for {peer}: {e}");
            let _ = pump.finish(PUMP_DRAIN + Duration::from_secs(2));
            return WorkerResult::Done;
        }
    };

    // Explicit acknowledgement: the literal AUTH_OK line is the FIRST
    // application bytes the host ever sends, strictly BEFORE any MCP data
    // or child output. On failure the connection is closed WITHOUT an ack —
    // the client fails non-zero (never a silent success).
    let to_pump = pump.take_sender();
    if to_pump.send(super::AUTH_ACK.to_vec()).is_err() {
        eprintln!("[computer-host] remote auth ack could not be sent for {peer}");
        let _ = pump.finish(PUMP_DRAIN + Duration::from_secs(2));
        return WorkerResult::Done;
    }
    eprintln!("[computer-host] remote client {peer} authenticated; starting stdio worker child");

    // Spawn the SAME executable in its existing stdio mode. Token and TLS
    // key material never reach the child. stderr goes to OUR stderr (which
    // the launcher redirects with --log-file).
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[computer-host] cannot resolve current_exe: {e}");
            let _ = pump.finish(PUMP_DRAIN + Duration::from_secs(2));
            return WorkerResult::Done;
        }
    };
    let mut cmd = Command::new(exe);
    if mock_backend {
        cmd.arg("--mock-backend");
    }
    feedback.append_args(&mut cmd);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: this supervisor may run detached (Scheduled
        // Task); the stdio child must not pop a console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[computer-host] cannot spawn stdio worker child: {e}");
            let _ = pump.finish(PUMP_DRAIN + Duration::from_secs(2));
            return WorkerResult::Done;
        }
    };
    let child_stdin = child.stdin.take().expect("piped child stdin");
    let child_stdout = child.stdout.take().expect("piped child stdout");

    bridge(
        pump,
        to_pump,
        prefix,
        child_stdin,
        child_stdout,
        shutdown_flag,
    );

    // Session over: reap the owned child (bounded grace, then kill).
    match reap_child(&mut child) {
        Ok(status) => {
            eprintln!("[computer-host] remote client {peer} session ended; worker child {status}");
            WorkerResult::Done
        }
        Err(m) => {
            eprintln!("[computer-host] worker child unrecoverable for {peer}: {m}");
            WorkerResult::Quarantine(m)
        }
    }
}

/// TLS handshake + token line, one ABSOLUTE deadline, 4096-byte cap,
/// constant-time compare. Never logs the presented bytes. Returns the bytes
/// AFTER the token newline in the SAME data chunk (TLS records do not match
/// application messages; a client may pipeline token+initialize). Those
/// bytes are fed FIRST into the bridge — never dropped.
fn authenticate(pump: &PumpHandle, expected: &[u8]) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + AUTH_TIMEOUT;
    let mut buf: Vec<u8> = Vec::with_capacity(128);
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("authentication timeout".into());
        }
        match pump.events.recv_timeout(deadline - now) {
            Ok(PumpEvent::Connected) => {}
            Ok(PumpEvent::Data(chunk)) => {
                for (i, b) in chunk.iter().enumerate() {
                    if *b == b'\n' {
                        if token_matches(expected, &buf) {
                            // Preserve everything after the newline.
                            return Ok(chunk[i + 1..].to_vec());
                        }
                        return Err("invalid token".into());
                    }
                    if *b != b'\r' {
                        buf.push(*b);
                    }
                    if buf.len() > MAX_TOKEN_BYTES {
                        return Err("authentication line too long".into());
                    }
                }
            }
            Ok(PumpEvent::ClosedByPeer) => {
                return Err("connection closed before authentication".into())
            }
            Ok(PumpEvent::Failed(m)) => return Err(m),
            Ok(PumpEvent::InboundOverflow) => {
                return Err("inbound queue overflow during authentication".into())
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                return Err("authentication timeout".into())
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("TLS pump died during authentication".into())
            }
        }
    }
}

/// Post-auth full-duplex bridge between the TLS pump and the owned child's
/// stdio. Any fatal direction tears the whole session down. `prefix` is any
/// post-token bytes from the auth chunk, fed FIRST. `to_pump` is the unique
/// outbound sender (already used for the AUTH_OK ack); it moves into the
/// child-stdout thread.
fn bridge(
    pump: PumpHandle,
    to_pump: std::sync::mpsc::SyncSender<Vec<u8>>,
    prefix: Vec<u8>,
    child_stdin: std::process::ChildStdin,
    child_stdout: std::process::ChildStdout,
    shutdown_flag: &Arc<AtomicBool>,
) {
    // Channel EOF (child dead / stdout EOF) is observable by the pump via
    // the moved-in sender, which sends its close_notify — the host can close
    // the session even if the peer's stdin direction is still open.
    let out_thread = std::thread::Builder::new()
        .name("computer-remote-out".into())
        .spawn(move || {
            let mut reader = std::io::BufReader::new(child_stdout);
            let mut chunk = vec![0u8; CHUNK_BYTES];
            loop {
                match std::io::Read::read(&mut reader, &mut chunk) {
                    Ok(0) => return, // child EOF: runtime cleaned up
                    Ok(n) => {
                        // <= CHUNK_BYTES per send: bounded memory per queued
                        // item; the pump owns TLS record framing.
                        if to_pump.send(chunk[..n].to_vec()).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
            // to_pump drops here → pump sends close_notify after flushing.
        })
        .expect("spawn remote out");

    // network → line assembly → child stdin (bounded queue + frame cap).
    // child_stdin lives in this thread; when the frame channel closes the
    // writer drops it and the child sees EOF and cleans up natively.
    let (child_tx, child_rx) = sync_channel::<Option<Vec<u8>>>(INBOUND_CHUNKS);
    let in_thread = std::thread::Builder::new()
        .name("computer-remote-in".into())
        .spawn(move || {
            let mut stdin = child_stdin;
            while let Ok(Some(frame)) = child_rx.recv() {
                if stdin.write_all(&frame).is_err() {
                    return;
                }
            }
            // Channel closed: `stdin` DROPS here → child sees EOF.
        })
        .expect("spawn remote in");

    // Feed any post-token bytes from the auth chunk FIRST (TLS record ≠
    // application message; the client may have pipelined initialize).
    let mut frame: Vec<u8> = Vec::with_capacity(4096);
    let mut fatal = false;
    let mut clean_peer_close = false;
    if !prefix.is_empty() {
        fatal = !feed_bytes(&prefix, &mut frame, &child_tx);
    }

    while !fatal {
        if shutdown_flag.load(Ordering::SeqCst) {
            break;
        }
        match pump.events.recv_timeout(Duration::from_millis(50)) {
            Ok(PumpEvent::Connected) => {} // already announced during auth
            Ok(PumpEvent::Data(chunk)) => {
                if !feed_bytes(&chunk, &mut frame, &child_tx) {
                    fatal = true;
                }
            }
            Ok(PumpEvent::ClosedByPeer) => {
                clean_peer_close = true;
                break;
            }
            Ok(PumpEvent::Failed(m)) => {
                eprintln!("[computer-host] remote transport failed: {m}");
                fatal = true;
            }
            Ok(PumpEvent::InboundOverflow) => {
                eprintln!("[computer-host] remote inbound overflow; closing session");
                fatal = true;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // Teardown: flush any final partial frame (clean close only; on a FATAL
    // path nothing further is delivered), then CLOSE the frame channel —
    // dropping child_tx ends the writer loop, which drops child stdin so the
    // native runtime cleans up. The out thread owns the pump's sender, so
    // child-stdout EOF closes the TLS output side; pump.finish gives the
    // graceful path a bounded window, then aborts and joins.
    if clean_peer_close && !frame.is_empty() {
        frame.push(b'\n');
        let _ = child_tx.try_send(Some(std::mem::take(&mut frame)));
    }
    drop(child_tx);
    let outcome = pump.finish(PUMP_DRAIN + Duration::from_secs(2));
    if let Outcome::Failed(m) = &outcome {
        eprintln!("[computer-host] remote pump ended with error: {m}");
    }
    // The in-thread is bounded by the child-stdin write; killing/reaping the
    // (owned) child first unblocks a full pipe before we join.
    let _ = wait_join(
        out_thread,
        Duration::from_secs(CHILD_EXIT_GRACE.as_secs() + 2),
    );
    let _ = wait_join(in_thread, THREAD_JOIN);
}

/// Assemble bytes into newline-terminated frames and forward them to the
/// child over the bounded queue. Returns false on a fatal condition (queue
/// full → explicit teardown, never a silent drop; frame over the cap).
fn feed_bytes(
    bytes: &[u8],
    frame: &mut Vec<u8>,
    child_tx: &std::sync::mpsc::SyncSender<Option<Vec<u8>>>,
) -> bool {
    for b in bytes {
        if *b == b'\n' {
            frame.push(b'\n');
            if child_tx.try_send(Some(std::mem::take(frame))).is_err() {
                eprintln!(
                    "[computer-host] remote inbound frame backlog full; \
                     tearing down the session (explicit, no silent drop)"
                );
                return false;
            }
        } else {
            frame.push(*b);
            if frame.len() > crate::mcp::jsonrpc::MAX_LINE_BYTES {
                eprintln!("[computer-host] remote frame exceeds cap; closing session");
                return false;
            }
        }
    }
    true
}

/// Reap ONLY the owned child: bounded grace after stdin EOF, then kill.
/// Err means the child could not be killed/reaped — the caller QUARANTINES
/// the listener rather than claiming it is safe to accept another session.
fn reap_child(child: &mut Child) -> Result<String, String> {
    let deadline = Instant::now() + CHILD_EXIT_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return classify_worker_exit(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    eprintln!(
                        "[computer-host] worker child did not exit within {CHILD_EXIT_GRACE:?} \
                         after stdin close; killing the owned child"
                    );
                    if let Err(e) = child.kill() {
                        return Err(format!("cannot kill owned child: {e}"));
                    }
                    let reap_deadline = Instant::now() + Duration::from_secs(2);
                    loop {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                return Err(format!(
                                    "forced worker kill ({status}); native cleanup unconfirmed"
                                ))
                            }
                            Ok(None) if Instant::now() < reap_deadline => {
                                std::thread::sleep(Duration::from_millis(25))
                            }
                            Ok(None) => return Err("killed but reap deadline exceeded".into()),
                            Err(e) => return Err(format!("killed but reap failed: {e}")),
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(format!("wait error: {e}")),
        }
    }
}

// MAX_OUT_LINE_BYTES is referenced by the (shared) child-output cap; keep the
// import meaningful even though chunking is now byte-oriented.

fn classify_worker_exit(status: std::process::ExitStatus) -> Result<String, String> {
    if status.code() == Some(7) {
        Err("worker shutdown_unknown; native cleanup unconfirmed, listener quarantined".into())
    } else {
        Ok(format!("exited with {status}"))
    }
}

#[cfg(test)]
mod feedback_tests {
    #[test]
    fn feedback_forwarding_is_argv_only_and_contains_no_tls_or_token_fields() {
        let feedback = crate::feedback::FeedbackConfig {
            executable: Some("C:/Program Files/Feedback/renderer.exe".into()),
            accent: Some("#2288aa".into()),
            label: Some("AI control".into()),
        };
        let mut cmd = std::process::Command::new("owned-child.exe");
        feedback.append_args(&mut cmd);
        let args: Vec<_> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--desktop-feedback",
                "C:/Program Files/Feedback/renderer.exe",
                "--feedback-accent",
                "#2288aa",
                "--feedback-label",
                "AI control"
            ]
        );
        assert!(!args
            .iter()
            .any(|a| a.contains("tls") || a.contains("token")));
    }
    #[cfg(windows)]
    #[test]
    fn shutdown_unknown_exit_quarantines_instead_of_regranting_remote_child() {
        use std::os::windows::process::ExitStatusExt;
        assert!(super::classify_worker_exit(std::process::ExitStatus::from_raw(7)).is_err());
        assert!(super::classify_worker_exit(std::process::ExitStatus::from_raw(0)).is_ok());
        assert!(super::classify_worker_exit(std::process::ExitStatus::from_raw(4)).is_ok());
    }
}
