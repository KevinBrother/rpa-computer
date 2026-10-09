//! Client-side TLS stdio bridge for `computer-client`.
//!
//! stdin/stdout carry MCP JSON-RPC frames ONLY; the network side is TLS +
//! token + an explicit `AUTH_OK\n` acknowledgement. The token is sent
//! strictly AFTER the TLS handshake completes and is never logged or placed
//! in argv. After the token the client WAITS for the exact `AUTH_OK\n` line
//! within the remaining auth deadline BEFORE the stdin forwarder starts:
//! a missing/invalid/truncated ack (wrong token, stale server, confused
//! peer) is a NON-ZERO exit — never a silent bridge to nowhere. The ack is
//! consumed by the auth phase and NEVER forwarded to MCP stdout; any bytes
//! after the ack newline in the same TLS record (early MCP data) are
//! preserved and forwarded first. A truncated TLS session (socket EOF
//! without close_notify) is a non-zero exit — never a fake clean success.
//!
//! Threading: stdin→pump owns the UNIQUE pump sender (stdin EOF disconnects
//! the channel so the pump sends close_notify — a retained extra sender
//! would hide that EOF forever). pump→stdout is a dedicated thread over a
//! BOUNDED channel; a full stdout queue is an explicit cancel, not a block
//! of the main thread. Exit NEVER joins the stdin reader unboundedly (a
//! stuck terminal pipe cannot hang the process), and the final MCP frame is
//! flushed because the stdout thread IS joined (bounded) before exit.
//! `Outcome::JoinTimeout` is NEVER a success.

use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::time::{Duration, Instant};

use rustls::{ClientConnection, Connection};

use crate::mcp::jsonrpc::{MAX_LINE_BYTES, MAX_OUT_LINE_BYTES};
use crate::mcp::tcp::{load_token, AUTH_TIMEOUT};

use super::pump::{run_pump, wait_join, Outcome, PumpEvent, CHUNK_BYTES};

const INBOUND_CHUNKS: usize = 32;
const OUTBOUND_CHUNKS: usize = 64;
/// Bounded drain after stdin EOF: in-flight responses are still delivered.
const DRAIN: Duration = Duration::from_secs(10);
/// Bounded join for helper threads on exit.
const THREAD_JOIN: Duration = Duration::from_secs(3);

pub struct ClientArgs {
    pub connect: String,
    pub ca_cert: PathBuf,
    pub server_name: Option<String>,
    pub token_file: PathBuf,
}

/// Exit codes: 0 clean; 2 arg/usage; 3 connect; 4 TLS; 5 auth/config; 6 I/O.
pub fn run(args: ClientArgs) -> u8 {
    let token = match load_token(&args.token_file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[computer-client] {e}");
            return 5;
        }
    };
    let config = match super::tls::client_config(&args.ca_cert) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[computer-client] {e}");
            return 4;
        }
    };
    let name = match &args.server_name {
        Some(n) => n.clone(),
        None => {
            // Default: the DNS name from --connect (must be a DNS name, not
            // a bare IP — for IPs the operator passes --server-name).
            match args.connect.rsplit_once(':') {
                Some((host, _)) => host.to_string(),
                None => {
                    eprintln!("[computer-client] --connect must be HOST:PORT");
                    return 2;
                }
            }
        }
    };
    let server_name = match super::tls::parse_server_name(&name) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[computer-client] {e}");
            return 2;
        }
    };

    let addr = match args
        .connect
        .to_socket_addrs()
        .ok()
        .and_then(|mut it| it.next())
    {
        Some(a) => a,
        None => {
            eprintln!("[computer-client] cannot resolve {}", args.connect);
            return 3;
        }
    };
    let sock = match TcpStream::connect_timeout(&addr, AUTH_TIMEOUT) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[computer-client] connect to {} failed: {e}", args.connect);
            return 3;
        }
    };
    let client = match ClientConnection::new(config, server_name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[computer-client] cannot create TLS client state: {e}");
            return 4;
        }
    };

    let mut pump = run_pump(
        Connection::Client(client),
        sock,
        INBOUND_CHUNKS,
        OUTBOUND_CHUNKS,
        DRAIN,
    );

    // Wait for the TLS handshake BEFORE sending the token. This deadline is
    // NOT restarted: the auth ack shares the same absolute AUTH_TIMEOUT
    // window that started here ("the remaining auth deadline").
    let auth_deadline = Instant::now() + AUTH_TIMEOUT;
    loop {
        let now = Instant::now();
        if now >= auth_deadline {
            eprintln!("[computer-client] TLS handshake timeout");
            pump.finish(THREAD_JOIN);
            return 4;
        }
        match pump.events.recv_timeout(auth_deadline - now) {
            Ok(PumpEvent::Connected) => break,
            Ok(PumpEvent::Failed(m)) => {
                eprintln!("[computer-client] TLS handshake failed: {m}");
                pump.finish(THREAD_JOIN);
                return 4;
            }
            Ok(PumpEvent::ClosedByPeer) => {
                eprintln!("[computer-client] server closed during TLS handshake");
                pump.finish(THREAD_JOIN);
                return 4;
            }
            Ok(_) => {}
            Err(_) => {
                eprintln!("[computer-client] TLS handshake timeout");
                pump.finish(THREAD_JOIN);
                return 4;
            }
        }
    }

    // Take the UNIQUE sender: the auth line goes first, then stdin owns it.
    // Channel EOF at stdin-end is exactly what tells the pump to send its
    // close_notify — a second retained sender would hide that EOF.
    let to_pump = pump.take_sender();

    // Authenticate: token line is the FIRST application bytes.
    let mut auth = token;
    auth.push(b'\n');
    if to_pump.send(auth).is_err() {
        eprintln!("[computer-client] cannot send authentication");
        pump.finish(THREAD_JOIN);
        return 5;
    }

    // Wait for the EXPLICIT acknowledgement BEFORE stdin forwarding starts:
    // the host answers the literal `AUTH_OK\n` line before any MCP data; a
    // rejected token closes with NO ack. A missing/invalid/truncated ack is
    // a non-zero exit (distinct diagnostics; the token is never echoed).
    // Any bytes AFTER the ack newline in the same chunk are early MCP data
    // and are preserved for the stdout bridge.
    let prefix: Vec<u8> = match await_auth_ok(&pump.events, auth_deadline) {
        Ok(p) => p,
        Err(code) => {
            pump.finish(THREAD_JOIN);
            return code;
        }
    };

    // stdin → pump (frames pass through unchanged). The unique sender moves
    // in; when stdin hits EOF the thread returns and the sender DROPS, which
    // disconnects the channel → pump sends close_notify and drains.
    std::thread::Builder::new()
        .name("computer-client-stdin".into())
        .spawn(move || {
            use std::io::Read;
            let stdin = std::io::stdin();
            let mut lock = stdin.lock();
            let mut buf = vec![0u8; CHUNK_BYTES];
            loop {
                match lock.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        if to_pump.send(chunk).is_err() {
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }
            // Unique sender drops here: pump sees EOF → close_notify → drain.
        })
        .expect("spawn client stdin");

    // pump → stdout (line-framed). Bounded queue; joined (bounded) before
    // exit so the FINAL MCP frame is actually flushed to the terminal/pipe.
    let (line_tx, line_rx) = sync_channel::<Vec<u8>>(OUTBOUND_CHUNKS);
    let stdout_thread = std::thread::Builder::new()
        .name("computer-client-stdout".into())
        .spawn(move || {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            drain_to_writer(&line_rx, &mut lock)
            // Channel closed: loop ends, lock drops, stdout is flushed.
        })
        .expect("spawn client stdout");

    // Main: assemble network bytes into lines and forward to stdout.
    let exit = bridge_events(&pump.events, &line_tx, prefix);
    drop(line_tx); // close the queue → stdout thread flushes the tail and exits

    // Wait (bounded) for the pump: close_notify + drain of in-flight
    // responses after stdin EOF. JoinTimeout is NOT a success.
    let outcome = pump.finish(DRAIN + Duration::from_secs(2));
    // Bounded join of the stdout writer so the final frame reaches the pipe.
    // ONLY Some(Ok(())) is success: a write/flush failure on the FINAL frame
    // (queued before the peer closed) or a join timeout is exit 6, never 0.
    let mut exit = exit;
    match wait_join(stdout_thread, THREAD_JOIN) {
        Some(Ok(())) => {}
        Some(Err(e)) => {
            eprintln!("[computer-client] stdout write failed: {e}");
            if exit == 0 {
                exit = 6;
            }
        }
        None => {
            eprintln!("[computer-client] stdout writer did not finish within the bound");
            if exit == 0 {
                exit = 6;
            }
        }
    }
    match (exit, outcome) {
        (0, Outcome::Clean) => 0,
        (0, Outcome::Stopped) => {
            // Stopped covers forced aborts and drain timeouts: an explicit
            // cancellation can NEVER claim final delivery.
            eprintln!("[computer-client] transport stopped before a clean close");
            6
        }
        (0, Outcome::Failed(m)) => {
            eprintln!("[computer-client] transport ended with error: {m}");
            6
        }
        (0, Outcome::JoinTimeout) => {
            eprintln!("[computer-client] pump did not finish within the bound");
            6
        }
        (code, _) => code,
    }
}

/// Consume pump events until the literal `AUTH_OK\n` acknowledgement line.
/// Returns the bytes AFTER the ack newline in the same chunk (early MCP
/// data — preserved, never dropped). Errors are the intended exit code with
/// a diagnostic already printed; no presented/expected token bytes are ever
/// logged or echoed.
fn await_auth_ok(events: &Receiver<PumpEvent>, deadline: Instant) -> Result<Vec<u8>, u8> {
    let mut line: Vec<u8> = Vec::with_capacity(16);
    loop {
        let now = Instant::now();
        if now >= deadline {
            eprintln!(
                "[computer-client] authentication failed: the server sent no \
                 AUTH_OK acknowledgement within the deadline"
            );
            return Err(5);
        }
        match events.recv_timeout(deadline - now) {
            Ok(PumpEvent::Connected) => {} // duplicate announce; keep waiting
            Ok(PumpEvent::Data(chunk)) => {
                for (i, b) in chunk.iter().enumerate() {
                    line.push(*b);
                    if *b == b'\n' {
                        if line == super::AUTH_ACK {
                            // Preserve everything after the ack newline.
                            return Ok(chunk[i + 1..].to_vec());
                        }
                        eprintln!(
                            "[computer-client] authentication failed: invalid \
                             acknowledgement from the server"
                        );
                        return Err(5);
                    }
                    if line.len() > super::AUTH_ACK.len() {
                        eprintln!(
                            "[computer-client] authentication failed: invalid \
                             acknowledgement from the server"
                        );
                        return Err(5);
                    }
                }
            }
            Ok(PumpEvent::ClosedByPeer) => {
                eprintln!(
                    "[computer-client] authentication failed: the server closed \
                     without an acknowledgement"
                );
                return Err(5);
            }
            Ok(PumpEvent::Failed(m)) => {
                eprintln!("[computer-client] transport failed during authentication: {m}");
                return Err(5);
            }
            Ok(PumpEvent::InboundOverflow) => {
                eprintln!("[computer-client] inbound overflow during authentication");
                return Err(5);
            }
            Err(_) => {
                eprintln!(
                    "[computer-client] authentication failed: the server sent no \
                     AUTH_OK acknowledgement within the deadline"
                );
                return Err(5);
            }
        }
    }
}

/// Consume pump events → stdout lines. `prefix` is any early MCP data that
/// arrived in the same TLS record as the auth ack (fed FIRST). Returns the
/// intended exit code. A FULL stdout queue is an explicit cancel (never
/// block the main thread forever behind a stuck pipe); a DEAD stdout writer
/// (closed pipe) is an I/O failure — the peer's bytes could not be
/// delivered, so the exit is non-zero.
fn bridge_events(
    events: &Receiver<PumpEvent>,
    line_tx: &SyncSender<Vec<u8>>,
    prefix: Vec<u8>,
) -> u8 {
    let mut frame: Vec<u8> = Vec::with_capacity(4096);
    // Post-ack bytes from the auth chunk are data like any other: feed them
    // first so a pipelined server response is never lost.
    if let Some(code) = feed_frame(&prefix, &mut frame, line_tx) {
        return code;
    }
    loop {
        match events.recv() {
            Ok(PumpEvent::Connected) => {}
            Ok(PumpEvent::Data(chunk)) => {
                if let Some(code) = feed_frame(&chunk, &mut frame, line_tx) {
                    return code;
                }
            }
            Ok(PumpEvent::ClosedByPeer) => {
                // Clean close: deliver any final partial line, terminated.
                if !frame.is_empty() {
                    frame.push(b'\n');
                    match line_tx.try_send(std::mem::take(&mut frame)) {
                        Ok(()) => {}
                        Err(std::sync::mpsc::TrySendError::Full(_)) => {
                            eprintln!(
                                "[computer-client] stdout queue full; final frame not delivered"
                            );
                            return 6;
                        }
                        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                            eprintln!("[computer-client] stdout pipe closed before delivery");
                            return 6;
                        }
                    }
                }
                return 0;
            }
            Ok(PumpEvent::Failed(m)) => {
                eprintln!("[computer-client] transport failed: {m}");
                return 6;
            }
            Ok(PumpEvent::InboundOverflow) => {
                eprintln!("[computer-client] inbound overflow; closing");
                return 6;
            }
            Err(_) => return 6, // pump died without a final event
        }
    }
}

/// Assemble bytes into newline-terminated frames and queue them for the
/// stdout writer. Returns `Some(exit_code)` when forwarding must stop.
fn feed_frame(bytes: &[u8], frame: &mut Vec<u8>, line_tx: &SyncSender<Vec<u8>>) -> Option<u8> {
    for b in bytes {
        frame.push(*b);
        if *b == b'\n' {
            match line_tx.try_send(std::mem::take(frame)) {
                Ok(()) => {}
                Err(std::sync::mpsc::TrySendError::Full(_)) => {
                    eprintln!("[computer-client] stdout queue full; cancelling");
                    return Some(6);
                }
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    // The stdout writer is gone: delivery FAILED. This is an
                    // I/O error, not a clean close.
                    eprintln!("[computer-client] stdout pipe closed before delivery");
                    return Some(6);
                }
            }
        } else if frame.len() > MAX_OUT_LINE_BYTES {
            eprintln!("[computer-client] peer frame exceeds cap; closing");
            return Some(6);
        }
    }
    None
}

/// Drain queued lines into `w`. Returns Err on the FIRST write/flush failure:
/// a frame that was queued (e.g. the final response just before the peer
/// closed) but could not be delivered must surface as an I/O error, never as
/// a silent success. Generic over `Write` so tests can inject failing writers.
fn drain_to_writer<W: Write>(rx: &Receiver<Vec<u8>>, w: &mut W) -> std::io::Result<()> {
    while let Ok(line) = rx.recv() {
        w.write_all(&line)?;
        w.flush()?;
    }
    Ok(())
}

/// Frame cap used for OUR outbound (stdin) lines matches the host's.
pub const CLIENT_IN_CAP: usize = MAX_LINE_BYTES;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    /// Writer that fails with BrokenPipe once its byte budget is spent —
    /// models a consumer pipe that closes mid-delivery.
    struct FailingWriter {
        budget: usize,
    }

    impl Write for FailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.budget == 0 {
                return Err(Error::new(ErrorKind::BrokenPipe, "pipe closed"));
            }
            let n = buf.len().min(self.budget);
            self.budget -= n;
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct FlushFailWriter;

    impl Write for FlushFailWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(Error::new(ErrorKind::BrokenPipe, "flush failed"))
        }
    }

    #[test]
    fn full_drain_reports_success() {
        let (tx, rx) = sync_channel::<Vec<u8>>(4);
        tx.send(b"a\n".to_vec()).unwrap();
        tx.send(b"b\n".to_vec()).unwrap();
        drop(tx);
        let mut w: Vec<u8> = Vec::new();
        drain_to_writer(&rx, &mut w).expect("clean drain");
        assert_eq!(w, b"a\nb\n");
    }

    #[test]
    fn final_frame_write_failure_is_an_error() {
        // Regression: the final response is queued, the peer then closes; the
        // write of that final frame fails and MUST NOT look like success.
        let (tx, rx) = sync_channel::<Vec<u8>>(4);
        tx.send(b"first\n".to_vec()).unwrap();
        tx.send(b"final\n".to_vec()).unwrap();
        drop(tx);
        let mut w = FailingWriter { budget: 6 }; // first line fits, final fails
        assert!(drain_to_writer(&rx, &mut w).is_err());
    }

    #[test]
    fn flush_failure_is_an_error() {
        let (tx, rx) = sync_channel::<Vec<u8>>(4);
        tx.send(b"frame\n".to_vec()).unwrap();
        drop(tx);
        let mut w = FlushFailWriter;
        assert!(drain_to_writer(&rx, &mut w).is_err());
    }
}
