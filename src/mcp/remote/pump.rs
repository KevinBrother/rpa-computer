//! Bounded, full-duplex, non-blocking TLS pump.
//!
//! ONE thread owns the `rustls::Connection` state machine and the
//! non-blocking socket — no Mutex around blocking reads, no second owner.
//! Application bytes cross via BOUNDED chunk channels in both directions:
//! a full inbound queue is EXPLICIT (the connection is torn down, never
//! silently dropping data); a full outbound queue applies backpressure by
//! stopping socket reads (the peer's send buffer then backs up naturally).
//!
//! Key invariants:
//! - EVERY outer-loop iteration flushes pending TLS bytes FIRST. The very
//!   first client handshake flight (ClientHello) is written unconditionally,
//!   so the handshake can never deadlock waiting for app data.
//! - `flush_socket` returns `Backpressured` to the outer loop instead of
//!   sleeping forever; TLS-internal plaintext state retains unsent bytes.
//! - A partial `conn.writer().write` is RETAINED across iterations in
//!   `pending` — never dropped, never re-fed (re-feeding corrupts framing).
//! - A peer `close_notify` is a HALF-close of INPUT: one `ClosedByPeer`
//!   event, reads stop, but caller output is still accepted and flushed
//!   until the caller's sender drops. Only then do we send our own
//!   close_notify, flush, and boundedly wait for the peer's.
//! - `Clean` is reported ONLY after the last outbound bytes were ACTUALLY
//!   flushed: no retained plaintext (`pending` empty) and no unwritten TLS
//!   records (`wants_write()` false). A peer close with an unsent buffer,
//!   or a drain-deadline expiry, is NEVER a clean exit — it is an explicit
//!   bounded abort (`Failed`/`Stopped`), so backpressure stalls can never
//!   masquerade as success.
//! - Raw socket EOF without close_notify is ALWAYS a failure (truncation),
//!   never a clean close.
//! - `finish()` joins via a bounded wait, then ABORTS (atomic flag + socket
//!   shutdown) and joins again — a stuck peer never leaks a detached pump.
//! - `JoinTimeout` is never treated as success by any caller.

use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::Duration;

use rustls::Connection;

use super::tls::TLS_BUFFER_LIMIT;

/// Max payload bytes per chunk between the pump and its caller.
pub const CHUNK_BYTES: usize = 64 * 1024;
/// Max outbound plaintext bytes retained in `pending` before the pump
/// declares a hard-stalled peer and tears the connection down explicitly.
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
/// Bounded iterations of `write_tls` per outer-loop pass.
const MAX_WRITES_PER_PASS: usize = 64;
/// Bounded iterations of `read_tls` per outer-loop pass.
const MAX_READS_PER_PASS: usize = 32;
/// Hard bound for the final join after abort in `PumpHandle::drop`.
const DROP_JOIN: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
pub enum PumpEvent {
    /// Handshake completed (before authentication on the host side).
    Connected,
    /// Decrypted application bytes from the peer.
    Data(Vec<u8>),
    /// Clean TLS close_notify from the peer (HALF-close of input only).
    ClosedByPeer,
    /// Fatal error: TLS failure, I/O failure, or truncation (EOF on the
    /// socket without close_notify — never reported as a clean close).
    Failed(String),
    /// The bounded inbound queue overflowed: the connection is dead.
    InboundOverflow,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Outcome {
    /// Internal default (only surfaced when a bounded join times out).
    #[default]
    JoinTimeout,
    /// Peer sent close_notify AND every outbound byte (retained plaintext
    /// plus TLS records) was verifiably flushed to the socket. NEVER
    /// reported while a send buffer still holds data or a deadline expired.
    Clean,
    /// Caller asked to stop (its writer side was dropped / abort requested),
    /// or a drain deadline expired before the peer closed cleanly. Bounded
    /// abort — NOT a clean success.
    Stopped,
    /// Fatal: TLS error, truncation, I/O error, or inbound overflow.
    Failed(String),
}

pub struct PumpHandle {
    /// The ONLY application writer. Take it with `take_sender()` and move it
    /// into exactly one producer thread so channel EOF is observable; do not
    /// hold a second sender while waiting for events.
    to_pump: Option<SyncSender<Vec<u8>>>,
    pub events: Receiver<PumpEvent>,
    thread: Option<std::thread::JoinHandle<Outcome>>,
    abort: Arc<AtomicBool>,
    /// Cloned socket used ONLY for `shutdown()` on abort.
    abort_sock: Option<TcpStream>,
}

impl PumpHandle {
    /// Take the unique outbound sender (once). Panics if already taken.
    pub fn take_sender(&mut self) -> SyncSender<Vec<u8>> {
        self.to_pump.take().expect("pump sender already taken")
    }

    /// Abort the pump thread (idempotent): the atomic flag unblocks channel
    /// waits; the socket shutdown unblocks a stuck socket op.
    fn request_abort(&self) {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(s) = &self.abort_sock {
            let _ = s.shutdown(Shutdown::Both);
        }
    }

    /// Signal end of outbound data (any retained sender here is dropped),
    /// then join with a hard bound: a graceful window first, then an abort
    /// and a second bounded join. The abort guarantees the thread exits.
    pub fn finish(mut self, timeout: Duration) -> Outcome {
        drop(self.to_pump.take());
        let thread = self.thread.take().expect("pump thread already joined");
        // Graceful window: most of the bound, so a normal drain is never cut
        // short; only the tail is reserved for the abort + final join. A
        // forced abort here can never be labelled successful by the caller.
        let reserve = Duration::from_millis(500);
        let graceful = timeout.saturating_sub(reserve).max(reserve);
        if wait_finished(&thread, graceful) {
            return thread.join().unwrap_or_default();
        }
        self.request_abort();
        if wait_finished(&thread, timeout.saturating_sub(graceful).max(reserve)) {
            thread.join().unwrap_or_default()
        } else {
            Outcome::JoinTimeout
        }
    }
}

impl Drop for PumpHandle {
    fn drop(&mut self) {
        drop(self.to_pump.take());
        if let Some(t) = self.thread.take() {
            self.request_abort();
            let _ = wait_join(t, DROP_JOIN);
        }
    }
}

/// Poll a JoinHandle for completion with a wall-clock bound (non-consuming).
fn wait_finished<T>(handle: &std::thread::JoinHandle<T>, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while !handle.is_finished() {
        if start.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    true
}

/// Wait for a JoinHandle with a wall-clock bound. On timeout None is
/// returned — the caller treats that as fatal (never a silent pass).
pub fn wait_join<T: Send + 'static>(
    handle: std::thread::JoinHandle<T>,
    timeout: Duration,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(handle.join());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(v)) => Some(v),
        _ => None,
    }
}

/// Drive one TLS connection until close. `conn` is the single-owned rustls
/// state machine; `sock` is switched to non-blocking. `inbound_bound` caps
/// queued decrypted chunks; `outbound_bound` caps queued plaintext waiting
/// to be encrypted. Drains remaining peer data after the caller's outbound
/// side closes, bounded by `drain_timeout`.
pub fn run_pump(
    mut conn: Connection,
    mut sock: TcpStream,
    inbound_bound: usize,
    outbound_bound: usize,
    drain_timeout: Duration,
) -> PumpHandle {
    let (to_pump_tx, to_pump_rx) = sync_channel::<Vec<u8>>(outbound_bound);
    let (event_tx, event_rx) = sync_channel::<PumpEvent>(inbound_bound + 4);
    let abort = Arc::new(AtomicBool::new(false));
    let abort_thread = Arc::clone(&abort);
    let abort_sock = sock.try_clone().ok();
    let thread = std::thread::Builder::new()
        .name("computer-tls-pump".into())
        .spawn(move || {
            pump_loop(
                &mut conn,
                &mut sock,
                &to_pump_rx,
                &event_tx,
                drain_timeout,
                &abort_thread,
            )
        })
        .expect("spawn tls pump");
    PumpHandle {
        to_pump: Some(to_pump_tx),
        events: event_rx,
        thread: Some(thread),
        abort,
        abort_sock,
    }
}

fn send_event(tx: &SyncSender<PumpEvent>, ev: PumpEvent) -> bool {
    match tx.try_send(ev) {
        Ok(()) => true,
        Err(TrySendError::Full(PumpEvent::Data(_))) => {
            // Queue full of DATA: explicit overflow, tear the connection down.
            let _ = tx.try_send(PumpEvent::InboundOverflow);
            false
        }
        Err(TrySendError::Full(ev)) => {
            // Control events must land: bounded retries; failure is explicit
            // (the caller treats a false return as fatal).
            let mut attempts = 0;
            let mut cur = ev;
            loop {
                std::thread::sleep(Duration::from_millis(10));
                attempts += 1;
                match tx.try_send(cur) {
                    Ok(()) => return true,
                    Err(TrySendError::Full(back)) if attempts < 100 => cur = back,
                    Err(_) => return false,
                }
            }
        }
        Err(TrySendError::Disconnected(_)) => false,
    }
}

enum Flush {
    /// Everything rustls wanted to write is on the socket.
    Done,
    /// Socket buffer is full; the outer loop must retry next pass.
    Backpressured,
    Fatal(String),
}

/// Bounded socket flush: never sleeps inside. WouldBlock hands control back
/// to the outer loop so deadlines and the abort flag stay live.
fn flush_socket(conn: &mut Connection, sock: &mut TcpStream) -> Flush {
    let mut n = 0;
    while conn.wants_write() {
        match conn.write_tls(sock) {
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Flush::Backpressured,
            Err(e) => return Flush::Fatal(format!("socket write: {e}")),
        }
        n += 1;
        if n >= MAX_WRITES_PER_PASS {
            return Flush::Backpressured;
        }
    }
    Flush::Done
}

/// Push retained plaintext into the TLS writer. `true` = all consumed.
fn write_pending(
    conn: &mut Connection,
    pending: &mut Vec<u8>,
    off: &mut usize,
) -> Result<bool, String> {
    while *off < pending.len() {
        match conn.writer().write(&pending[*off..]) {
            // A full rustls output buffer can accept zero plaintext bytes.
            // Yield to the caller's flush without losing the retained offset.
            // With no TLS output to flush, keep zero progress fatal instead
            // of introducing an endless retry with nothing to drain.
            Ok(0) if conn.wants_write() => return Ok(false),
            Ok(0) => return Err("tls writer accepted 0 bytes".into()),
            Ok(n) => *off += n,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(false),
            Err(e) => return Err(format!("tls write: {e}")),
        }
    }
    pending.clear();
    *off = 0;
    Ok(true)
}

fn pump_loop(
    conn: &mut Connection,
    sock: &mut TcpStream,
    to_pump: &Receiver<Vec<u8>>,
    events: &SyncSender<PumpEvent>,
    drain_timeout: Duration,
    abort: &AtomicBool,
) -> Outcome {
    conn.set_buffer_limit(Some(TLS_BUFFER_LIMIT));
    if sock.set_nonblocking(true).is_err() {
        let _ = send_event(
            events,
            PumpEvent::Failed("cannot set socket non-blocking".into()),
        );
        return Outcome::Failed("cannot set socket non-blocking".into());
    }

    let mut announced = false;
    let mut read_open = true;
    let mut peer_clean_close = false;
    let mut peer_close_announced = false;
    let mut close_notify_sent = false;
    let mut out_open = true; // caller's sender still connected
    let mut drain_deadline: Option<std::time::Instant> = None;
    // Outbound plaintext retained across iterations: NEVER re-fed after a
    // partial write (re-feeding would corrupt the TLS stream).
    let mut pending: Vec<u8> = Vec::new();
    let mut pending_off = 0usize;
    let mut app_buf = vec![0u8; CHUNK_BYTES];

    loop {
        if abort.load(Ordering::SeqCst) {
            return Outcome::Stopped;
        }

        // 0. Flush pending TLS bytes FIRST, every iteration. This is what
        //    makes the initial handshake flight (ClientHello / flight
        //    responses) reach the wire without waiting for app data.
        match flush_socket(conn, sock) {
            Flush::Done | Flush::Backpressured => {}
            Flush::Fatal(m) => return fail(events, m),
        }

        // 1. Outbound: drain retained plaintext, then take caller chunks.
        match write_pending(conn, &mut pending, &mut pending_off) {
            Err(m) => return fail(events, m),
            Ok(false) => match flush_socket(conn, sock) {
                Flush::Done | Flush::Backpressured => {}
                Flush::Fatal(m) => return fail(events, m),
            },
            Ok(true) => {
                if out_open {
                    match to_pump.try_recv() {
                        Ok(chunk) => {
                            if chunk.len() > MAX_PENDING_BYTES {
                                return fail(events, "outbound chunk exceeds bound".into());
                            }
                            pending = chunk;
                            pending_off = 0;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {}
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            out_open = false;
                            // Caller done sending: our own close_notify ONCE,
                            // after everything already queued is flushed.
                            if !close_notify_sent {
                                conn.send_close_notify();
                                close_notify_sent = true;
                                match flush_socket(conn, sock) {
                                    Flush::Done | Flush::Backpressured => {}
                                    Flush::Fatal(m) => return fail(events, m),
                                }
                            }
                            if drain_deadline.is_none() {
                                drain_deadline = Some(std::time::Instant::now() + drain_timeout);
                            }
                            if !read_open {
                                // Clean ONLY when the actual pending flush is
                                // verifiably complete: no retained plaintext
                                // and no unwritten TLS records.
                                if peer_clean_close && pending.is_empty() && !conn.wants_write() {
                                    return Outcome::Clean;
                                }
                                return if peer_clean_close {
                                    Outcome::Failed(
                                        "peer closed with outbound bytes still unflushed".into(),
                                    )
                                } else {
                                    Outcome::Stopped
                                };
                            }
                        }
                    }
                }
            }
        }

        // 2. Handshake announcement.
        if !announced && !conn.is_handshaking() {
            announced = true;
            if !send_event(events, PumpEvent::Connected) {
                return Outcome::Failed("event queue closed during handshake".into());
            }
        }

        // 3. Inbound: socket → TLS → decrypted chunks. A peer close_notify
        //    HALF-closes input (one event) but output stays live until the
        //    caller's sender drops.
        if read_open {
            let mut reads = 0;
            loop {
                match conn.read_tls(sock) {
                    Ok(0) => {
                        // Socket EOF. Only valid AFTER a close_notify; a raw
                        // EOF is truncation and ALWAYS a failure.
                        if peer_clean_close {
                            read_open = false;
                        } else {
                            return fail(
                                events,
                                "connection truncated: socket EOF without TLS close_notify".into(),
                            );
                        }
                        break;
                    }
                    Ok(_) => {
                        match conn.process_new_packets() {
                            Ok(io) => {
                                if io.peer_has_closed() {
                                    peer_clean_close = true;
                                }
                            }
                            Err(e) => {
                                let _ = flush_socket(conn, sock); // best-effort alert
                                return fail(events, format!("tls error: {e}"));
                            }
                        }
                        match flush_socket(conn, sock) {
                            Flush::Done | Flush::Backpressured => {}
                            Flush::Fatal(m) => return fail(events, m),
                        }
                        loop {
                            match conn.reader().read(&mut app_buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    if !send_event(events, PumpEvent::Data(app_buf[..n].to_vec())) {
                                        return Outcome::Failed(
                                            "inbound queue overflow; connection torn down".into(),
                                        );
                                    }
                                }
                                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                                Err(e) => return fail(events, format!("tls read: {e}")),
                            }
                        }
                        if peer_clean_close && !peer_close_announced {
                            peer_close_announced = true;
                            let _ = send_event(events, PumpEvent::ClosedByPeer);
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => return fail(events, format!("socket read: {e}")),
                }
                reads += 1;
                if reads >= MAX_READS_PER_PASS || peer_clean_close {
                    break;
                }
            }
            if peer_clean_close {
                read_open = false;
            }
        }

        // 4. Termination / deadlines.
        if let Some(d) = drain_deadline {
            if std::time::Instant::now() >= d {
                // A deadline expiry is an explicit BOUNDED ABORT, never a
                // clean exit: the peer did not finish its half of the close
                // within the bound (or bytes were still in flight).
                return if peer_clean_close {
                    Outcome::Failed("drain deadline expired after peer close".into())
                } else {
                    Outcome::Stopped
                };
            }
        }
        if !out_open && !read_open {
            // Both directions closed. Clean ONLY if the outbound side was
            // ACTUALLY flushed (no retained plaintext, no pending TLS
            // records); a peer close that raced an unsent buffer is a
            // failure, not a clean exit.
            if peer_clean_close && pending.is_empty() && !conn.wants_write() {
                return Outcome::Clean;
            }
            return if peer_clean_close {
                Outcome::Failed("peer closed with outbound bytes still unflushed".into())
            } else {
                Outcome::Stopped
            };
        }

        // 5. Bounded park ONLY when there is genuinely nothing to do
        //    (no pending TLS write, no retained plaintext). Otherwise loop
        //    immediately to flush/read again.
        if !conn.wants_write() && pending.is_empty() {
            if out_open {
                match to_pump.recv_timeout(Duration::from_millis(5)) {
                    Ok(chunk) => {
                        if chunk.len() > MAX_PENDING_BYTES {
                            return fail(events, "outbound chunk exceeds bound".into());
                        }
                        pending = chunk;
                        pending_off = 0;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        out_open = false;
                        if !close_notify_sent {
                            conn.send_close_notify();
                            close_notify_sent = true;
                            let _ = flush_socket(conn, sock);
                        }
                        if drain_deadline.is_none() {
                            drain_deadline = Some(std::time::Instant::now() + drain_timeout);
                        }
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

fn fail(events: &SyncSender<PumpEvent>, message: String) -> Outcome {
    let _ = send_event(events, PumpEvent::Failed(message.clone()));
    Outcome::Failed(message)
}

// The normal Cargo integration target includes this production module and
// calls the private helper through a test-only bridge. No algorithm is copied.
#[cfg(test)]
#[allow(dead_code)] // The library unit-test target does not use this bridge.
pub(crate) fn write_pending_for_regression(
    conn: &mut Connection,
    pending: &mut Vec<u8>,
    off: &mut usize,
) -> Result<bool, String> {
    write_pending(conn, pending, off)
}
