//! stdio transport tests. These drive the REAL production loop
//! (`run_with_reader`) over loopback TCP streams — the same loop body,
//! channels, reader thread, ingress stamping, and overflow latch as
//! production; only the stdin/stdout streams are substituted (a portable
//! cross-platform in-process full-duplex byte channel; this is NOT
//! `mcp::tcp::run`). Nothing here re-implements a transport on copied
//! test-only channels.
//!
//! Determinism rules (reviewed):
//! - synchronization uses handshake flags (`capture_entered`), bounded
//!   reads, and `read_for_id` gates — never a sleep as proof;
//! - every gated backend carries an always-release guard created and
//!   dropped INSIDE the scope closure, so the gate is opened BEFORE the
//!   scoped threads implicitly join — a panicking test can never hang the
//!   test binary;
//! - no sync call waits behind an unreleased capture gate: the gate is
//!   ALWAYS opened before any blocked reply is awaited.
//!
//! Shared fixture helpers (backends, gate guard, `PipeHost`) live in the
//! `fixture` submodule to keep this file focused on scenarios.

mod fixture;

use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::mcp::ingress;
use fixture::{
    assert_image_result, gated_worker, is_tool_error, stream_pair, test_worker, FrameRead,
    PipeHost, ReleaseGuard,
};

/// Send `count` UNIQUE small requests (`id`s starting at `base`), stopping
/// early only if the peer provably closed the connection. Every id is
/// distinct so nothing is refused for a duplicate-id reason.
fn send_unique_pings(host: &mut PipeHost, base: i64, count: i64) {
    for id in base..base + count {
        let mut line =
            serde_json::to_vec(&json!({"jsonrpc": "2.0", "id": id, "method": "ping"})).unwrap();
        line.push(b'\n');
        if !host.try_send_line(&line) {
            return;
        }
    }
}

// ---- fixture regression units (deterministic, no production loop) ----------

/// Bare reader-pair for the fixture regression units: both directions get
/// an explicit 1s socket timeout so the units can never depend on the
/// platform default (which is NO timeout: the old units blocked forever in
/// `recvfrom`). This is the test-side counterpart of `PipeHost::spawn`,
/// which sets the same timeouts on its client streams.
fn raw_reader_pair() -> (TcpStream, TcpStream) {
    let (client_read, peer_write) = stream_pair();
    let t = Duration::from_secs(1);
    client_read
        .set_read_timeout(Some(t))
        .expect("set reader timeout");
    peer_write
        .set_write_timeout(Some(t))
        .expect("set writer timeout");
    (client_read, peer_write)
}

/// Wire order A then B; requesting B first must return B IMMEDIATELY from
/// the wire while A is stashed — and a later read must return A from the
/// stash without ever touching the wire again. Regression for the old
/// fixture where `read_for_id` popped the pending front inside
/// `next_frame`, pushed it back, and looped on the same frame until the
/// deadline without ever reading the wire.
#[test]
fn fixture_read_for_id_matches_later_id_and_keeps_earlier_frame() {
    let (client_read, mut peer_write) = raw_reader_pair();
    let mut host = PipeHost::raw(client_read);

    peer_write
        .write_all(b"{\"jsonrpc\": \"2.0\", \"id\": 100, \"result\": {}}\n")
        .unwrap();
    peer_write
        .write_all(b"{\"jsonrpc\": \"2.0\", \"id\": 200, \"result\": {}}\n")
        .unwrap();

    let b = host.read_for_id(&json!(200), Duration::from_millis(500));
    assert_eq!(b["id"], json!(200), "later id must be matched off the wire");
    // A is stashed: returned in order, with NO peer traffic left.
    let a = host.read_for_id(&json!(100), Duration::from_millis(500));
    assert_eq!(
        a["id"],
        json!(100),
        "earlier frame must be retained in the stash"
    );
}

/// A frame split across the socket read-timeout boundary is reassembled
/// whole: incomplete bytes persist across `read_wire` calls (regression
/// for the old dead-code partial handler that dropped the chunk).
#[test]
fn fixture_partial_frame_reassembled_across_socket_timeout() {
    let (client_read, mut peer_write) = raw_reader_pair();
    let mut host = PipeHost::raw(client_read);

    let frame = "{\"jsonrpc\": \"2.0\", \"id\": 42, \"result\": {}}\n";
    let split = frame.len() / 2;
    peer_write.write_all(&frame.as_bytes()[..split]).unwrap();
    // First read wakes on the socket timeout with only a partial frame.
    let first = host.read_wire(Duration::from_millis(200));
    assert!(
        matches!(first, FrameRead::Timeout),
        "partial frame alone must time out, not parse"
    );
    // Write the remainder: the very next read completes the whole frame.
    peer_write.write_all(&frame.as_bytes()[split..]).unwrap();
    match host.read_wire(Duration::from_secs(2)) {
        FrameRead::Frame(f) => assert_eq!(f["id"], json!(42), "split frame reassembled whole"),
        FrameRead::Timeout => panic!("expected the reassembled frame, got timeout"),
        FrameRead::Eof => panic!("expected the reassembled frame, got eof"),
    }
}

/// A silent peer: `read_wire` reports Timeout — NEVER EOF — and the
/// elapsed time stays within the budget plus scheduling slack. The stream
/// carries NO preset socket timeout: `read_wire` must apply the remaining
/// caller budget itself before every blocking read (regression: the old
/// helper relied on the stream's preset timeout, so a bare stream blocked
/// forever in `recvfrom` even though the caller asked for 200ms).
#[test]
fn fixture_read_wire_timeout_is_distinct_from_eof() {
    let (client_read, peer_write) = stream_pair(); // NO preset timeouts
    let mut host = PipeHost::raw(client_read);

    let start = Instant::now();
    let r = host.read_wire(Duration::from_millis(200));
    let elapsed = start.elapsed();
    assert!(
        matches!(r, FrameRead::Timeout),
        "a silent peer must report Timeout, never EOF"
    );
    assert!(
        elapsed < Duration::from_millis(900),
        "caller budget must bound the read within slack, took {elapsed:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(200),
        "the budget itself must elapse, took {elapsed:?}"
    );
    drop(peer_write); // NOW the peer is gone.
    assert!(
        matches!(host.read_wire(Duration::from_secs(2)), FrameRead::Eof),
        "a dropped peer must report EOF"
    );
}

/// A stale LONG socket timeout must never override the caller's budget:
/// the helper re-applies the remaining budget before every blocking read.
/// Regression for the old helper, which blocked for the full preset 5s
/// socket timeout even when the caller asked for 200ms.
#[test]
fn fixture_read_wire_overrides_stale_long_socket_timeout() {
    let (client_read, peer_write) = stream_pair();
    client_read
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a stale long timeout");
    let mut host = PipeHost::raw(client_read);

    let start = Instant::now();
    let r = host.read_wire(Duration::from_millis(200));
    let elapsed = start.elapsed();
    assert!(
        matches!(r, FrameRead::Timeout),
        "a silent peer must report Timeout even with a stale long timeout"
    );
    assert!(
        elapsed < Duration::from_millis(900),
        "the caller budget must override the stale socket timeout, took {elapsed:?}"
    );
    drop(peer_write);
}

/// A peer that TRICKLES one byte per 50ms can never stretch `read_wire`
/// past the total caller budget: each wake re-checks and re-applies the
/// remaining budget (regression for a helper that bounded only individual
/// socket reads, so a continuously trickling peer kept it blocked
/// forever).
#[test]
fn fixture_read_wire_trickling_peer_cannot_exceed_budget() {
    let (client_read, mut peer_write) = stream_pair();
    peer_write
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("set writer timeout");
    let mut host = PipeHost::raw(client_read);
    let stop = Arc::new(AtomicBool::new(false));
    let trickler = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                if peer_write.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        })
    };

    let start = Instant::now();
    let r = host.read_wire(Duration::from_millis(300));
    let elapsed = start.elapsed();
    stop.store(true, Ordering::SeqCst);
    trickler.join().unwrap();
    assert!(
        matches!(r, FrameRead::Timeout),
        "a trickling peer must still end in Timeout"
    );
    assert!(
        elapsed < Duration::from_millis(900),
        "trickled bytes must never extend the total budget, took {elapsed:?}"
    );
}

/// A frame split MID-CODEPOINT across several writes (and socket wakes) is
/// reassembled whole: `read_wire` buffers raw BYTES, so an incomplete
/// UTF-8 sequence is never mangled by a string-decoding read (the old
/// `read_line` reader discarded bytes already consumed when a wake landed
/// between the lead and continuation bytes of a multi-byte character).
#[test]
fn fixture_partial_frame_reassembled_with_multibyte_utf8() {
    let (client_read, mut peer_write) = raw_reader_pair();
    let mut host = PipeHost::raw(client_read);

    let frame = "{\"jsonrpc\": \"2.0\", \"id\": 7, \"result\": {\"note\": \"héllo→世界\"}}\n";
    let bytes = frame.as_bytes();
    // Split INSIDE the multi-byte 'é' (0xC3 0xA9): after the 0xC3 lead byte.
    let split = frame.find('é').unwrap() + 1;
    assert!(
        split < bytes.len() && bytes[split] == 0xA9,
        "split must land mid-codepoint"
    );
    peer_write.write_all(&bytes[..split]).unwrap();
    let first = host.read_wire(Duration::from_millis(200));
    assert!(
        matches!(first, FrameRead::Timeout),
        "a mid-codepoint partial frame must time out, not parse or error"
    );
    peer_write.write_all(&bytes[split..]).unwrap();
    match host.read_wire(Duration::from_secs(2)) {
        FrameRead::Frame(f) => assert_eq!(
            f["result"]["note"],
            json!("héllo→世界"),
            "multi-byte frame reassembled whole"
        ),
        FrameRead::Timeout => panic!("expected the reassembled frame, got timeout"),
        FrameRead::Eof => panic!("expected the reassembled frame, got eof"),
    }
}

// ---- tests -----------------------------------------------------------------

/// Immediate queued-cancel: the cancel notification is pipelined directly
/// behind its request — the reader parses BOTH before the main loop can mark
/// the request in-flight. The request must still be refused with an exact
/// `error.code == "cancelled"` tool result, never run to a clean success
/// (the real harness race).
#[test]
fn stdio_immediate_queued_cancel_refuses_request() {
    let worker = test_worker();
    let cancel = worker.cancel_handle();
    std::thread::scope(|s| {
        let mut host = PipeHost::spawn(s, &worker);
        host.initialize();
        let session = host.open_session(2);

        // One write: request 3 + its cancel land together; the reader
        // processes both while the main loop is still on frame 3.
        let req = serde_json::to_string(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "computer_observe",
                       "arguments": {"session_id": session, "wait_ms": 2000}}
        }))
        .unwrap();
        let cancel_note = serde_json::to_string(&json!({
            "jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": 3}
        }))
        .unwrap();
        host.send_raw(format!("{req}\n{cancel_note}\n").as_bytes());

        let f = host.read_for_id(&json!(3), Duration::from_secs(10));
        assert!(
            is_tool_error(&f, "cancelled"),
            "request cancelled while queued must be refused with exact code, got: {f}"
        );

        // The host stays responsive; a fresh request proceeds normally.
        host.send(&json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}));
        let p = host.read_for_id(&json!(4), Duration::from_secs(5));
        assert!(p["result"].is_object(), "ping after cancel failed: {p}");

        host.close_stdin();
        assert_eq!(host.expect_exit("EOF after cancel"), "clean");
    });
    assert!(cancel.is_cancelled());
    worker.shutdown();
}

/// Active cancel: a genuinely in-flight (gated) request is cancelled by an
/// EXPLICIT id match; the cancel flag lands while the main thread is
/// blocked in the tool call. Unrelated and type-mismatched ids never
/// cancel; the unrelated in-flight work keeps running.
#[test]
fn stdio_active_cancel_matches_only_in_flight_id() {
    let release = Arc::new(AtomicBool::new(false));
    let (worker, capture_entered) = gated_worker(Arc::clone(&release));
    let cancel = worker.cancel_handle();
    std::thread::scope(|s| {
        // Guard INSIDE the scope: it drops (releasing the gate) BEFORE the
        // scope implicitly joins the host thread, so even a panicking test
        // leaves the host joinable.
        let _guard = ReleaseGuard(Arc::clone(&release));
        let mut host = PipeHost::spawn(s, &worker);
        host.initialize();
        let session = host.open_session(2);

        // Unrelated id BEFORE anything is in flight: never cancels.
        host.send(&json!({
            "jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": 99}
        }));
        host.send(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "computer_observe", "arguments": {"session_id": session}}
        }));
        // Handshake: the gated capture is genuinely running on the native
        // thread, so request 3 is the in-flight one.
        PipeHost::spin_until(
            || capture_entered.load(Ordering::SeqCst),
            "gated observe in flight",
        );
        assert!(
            !cancel.is_cancelled(),
            "unrelated requestId must never cancel"
        );

        // Unrelated id while 3 IS in flight: still no cancel.
        host.send(&json!({
            "jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": 300}
        }));
        // A type-mismatched twin of the in-flight id is not a match either.
        host.send(&json!({
            "jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": "3"}
        }));
        // Prove the reader processed those frames before asserting: pipeline
        // a matching cancel AFTER them; ordering in the reader is FIFO, so
        // once the match lands, the earlier ones were already handled.
        host.send(&json!({
            "jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": 3}
        }));
        PipeHost::spin_until(|| cancel.is_cancelled(), "matched cancel to land");
        assert!(cancel.is_cancelled(), "matching id must cancel directly");

        release.store(true, Ordering::SeqCst);
        let f = host.read_for_id(&json!(3), Duration::from_secs(10));
        // The observe was cancelled mid-flight: an exact `cancelled` tool
        // error, never a clean success and never a capture error.
        assert!(
            is_tool_error(&f, "cancelled"),
            "cancelled in-flight observe must report error.code=cancelled: {f}"
        );

        host.close_stdin();
        let _ = host.expect_exit("EOF after active cancel");
    });
    worker.shutdown();
}

/// EOF while a request is waiting: the reader-side direct EOF cancel lands
/// even though the main thread is blocked in the gated tool call. The gate
/// is opened as soon as the cancel is OBSERVED — before awaiting the exit —
/// so the native call can finish and the loop's exit is a genuine clean
/// stop, never a quarantine timeout.
#[test]
fn stdio_eof_cancels_while_main_blocked_in_tool_call() {
    let release = Arc::new(AtomicBool::new(false));
    let (worker, capture_entered) = gated_worker(Arc::clone(&release));
    let cancel = worker.cancel_handle();
    std::thread::scope(|s| {
        let _guard = ReleaseGuard(Arc::clone(&release));
        let mut host = PipeHost::spawn(s, &worker);
        host.initialize();
        let session = host.open_session(2);
        host.send(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "computer_observe", "arguments": {"session_id": session}}
        }));
        PipeHost::spin_until(
            || capture_entered.load(Ordering::SeqCst),
            "gated observe in flight",
        );
        host.close_stdin(); // EOF while the main loop is inside tools/call
        PipeHost::spin_until(|| cancel.is_cancelled(), "EOF direct cancel to land");
        // The cancel flag alone cannot finish the gated NATIVE call: release
        // the gate now, THEN require a bounded exit — the exit proves the
        // EOF path, not the 20s fault quarantine.
        release.store(true, Ordering::SeqCst);
        assert_eq!(
            host.expect_exit("loop exits on EOF with blocked main"),
            "clean",
            "EOF stop must be clean once the native call can finish"
        );
    });
    assert!(cancel.is_cancelled(), "EOF must cancel directly");
    worker.shutdown();
}

/// Normal backlog overflow: while the native thread (and thus the main
/// loop) is provably blocked, MORE than 64 unique small requests fill the
/// bounded frame queue. The reader must overflow EXPLICITLY: the cancel
/// flag lands, ONE protocol refusal is written, and the loop exits — no
/// infinite drain behind a dead main loop.
#[test]
fn stdio_overflow_refuses_once_and_exits() {
    let release = Arc::new(AtomicBool::new(false));
    let (worker, capture_entered) = gated_worker(Arc::clone(&release));
    let cancel = worker.cancel_handle();
    std::thread::scope(|s| {
        let _guard = ReleaseGuard(Arc::clone(&release));
        let mut host = PipeHost::spawn(s, &worker);
        host.initialize();
        let session = host.open_session(2);
        // Occupy the native thread and the main loop.
        host.send(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "computer_observe", "arguments": {"session_id": session}}
        }));
        PipeHost::spin_until(
            || capture_entered.load(Ordering::SeqCst),
            "gated observe to block the main loop",
        );
        // 64 UNIQUE small requests fill the bounded queue (64 slots) while
        // the main loop is blocked; unique ids keep every frame inside the
        // registered-id bound so nothing is refused for the wrong reason.
        // Writes are timeout-bounded; the loop stops early only if the peer
        // already closed (refusal raced ahead).
        send_unique_pings(&mut host, 1000, 64);
        // Overflow is explicit control: the 65th unique frame trips the
        // one-shot latch and the cancel flag lands even though the main
        // loop never moved.
        send_unique_pings(&mut host, 2000, 1);
        PipeHost::spin_until(|| cancel.is_cancelled(), "overflow to cancel directly");
        // The blocked native call can never finish while the gate is held;
        // release BEFORE awaiting refusal/exit so the exit is the explicit
        // refusal path, not a quarantine timeout.
        release.store(true, Ordering::SeqCst);

        // Exactly one protocol-level refusal is visible on the wire — even
        // if the loop thread already exited and buffered it — then the wire
        // reaches EOF. The observed answer for the gated observe (id 3)
        // does not count as a refusal.
        let errors = host.drain_errors_until_eof();
        assert_eq!(
            errors.len(),
            1,
            "overflow must post exactly one protocol refusal, got: {errors:?}"
        );
        assert_eq!(
            errors[0]["error"]["code"],
            json!(-32000),
            "overflow must be refused with server_busy: {}",
            errors[0]
        );
        assert_eq!(
            host.expect_exit("overflow exits the transport"),
            "clean",
            "overflow refusal is an explicit, non-fatal stop"
        );
        // The reader must terminate too: with the loop gone, further writes
        // eventually fail (no infinite drain).
        host.close_stdin();
    });
    worker.shutdown();
}

/// Oversized frames share the ONE-SHOT overflow latch: with the queue
/// provably full (64 queued small requests behind a blocked main loop), a
/// stream of oversized frames fails to enter the queue and must post
/// exactly ONE refusal — never a stream of markers onto the control
/// channel. The combined normal+oversized refusal stays one-shot.
#[test]
fn stdio_oversized_overflow_uses_same_one_shot_latch() {
    let release = Arc::new(AtomicBool::new(false));
    let (worker, capture_entered) = gated_worker(Arc::clone(&release));
    let cancel = worker.cancel_handle();
    std::thread::scope(|s| {
        let _guard = ReleaseGuard(Arc::clone(&release));
        let mut host = PipeHost::spawn(s, &worker);
        host.initialize();
        let session = host.open_session(2);
        host.send(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "computer_observe", "arguments": {"session_id": session}}
        }));
        PipeHost::spin_until(
            || capture_entered.load(Ordering::SeqCst),
            "gated observe to block the main loop",
        );
        // PREFILL the queue with 64 unique small requests: now the queue is
        // provably full BEFORE any oversized frame arrives, so each
        // oversized frame exceeds capacity and exercises the SHARED latch —
        // the old test sent only 4 oversized frames into an empty queue and
        // they were all enqueued as ordinary markers (no overflow at all).
        // The outcome/cancel state is NOT a stop condition here: a refusal
        // racing ahead once does not let the remaining small frames escape
        // the queue bound.
        send_unique_pings(&mut host, 1000, 64);
        // Oversized line (> 1 MiB) repeated: every one of them fails to
        // enter the full queue, but only ONE refusal may ever be posted
        // (normal+oversized share the latch).
        let big = vec![b'x'; ingress::MAX_LINE_BYTES_HINT + 8];
        let mut line = big.clone();
        line.push(b'\n');
        for _ in 0..4 {
            if !host.try_send_line(&line) {
                break; // peer closed after the refusal
            }
        }
        PipeHost::spin_until(
            || cancel.is_cancelled(),
            "oversized overflow to cancel directly",
        );
        // Release the gate BEFORE awaiting refusal/exit: the blocked native
        // call can then finish and the exit is the explicit refusal path.
        release.store(true, Ordering::SeqCst);

        // Drain the wire to REAL EOF (or bounded deadline) REGARDLESS of
        // whether the outcome is already recorded: a refusal buffered on
        // the wire after the loop thread finished is still observed — the
        // old `while outcome().is_none()` gate skipped it and counted 0.
        let errors = host.drain_errors_until_eof();
        assert_eq!(
            errors.len(),
            1,
            "oversized overflow must be refused exactly once, got: {errors:?}"
        );
        assert_eq!(errors[0]["error"]["code"], json!(-32000));
        assert_eq!(host.expect_exit("oversized overflow exits"), "clean");
        host.close_stdin();
    });
    worker.shutdown();
}

/// Escaped/spoofed ids are data, never control — through the REAL reader
/// path: a tool payload mentioning `notifications/cancelled` or embedding an
/// escaped requestId must not cancel anything, and a cancel naming an id
/// that was never accepted must not disturb the in-flight request.
#[test]
fn stdio_spoofed_and_escaped_ids_never_cancel() {
    let release = Arc::new(AtomicBool::new(false));
    let (worker, capture_entered) = gated_worker(Arc::clone(&release));
    let cancel = worker.cancel_handle();
    std::thread::scope(|s| {
        let _guard = ReleaseGuard(Arc::clone(&release));
        let mut host = PipeHost::spawn(s, &worker);
        host.initialize();
        let session = host.open_session(2);
        host.send(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "computer_observe", "arguments": {"session_id": session}}
        }));
        PipeHost::spin_until(
            || capture_entered.load(Ordering::SeqCst),
            "gated observe in flight",
        );
        // Payload string mentioning the notification (spoof attempt).
        host.send(&json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "computer_step", "arguments": {
                "session_id": session,
                "actions": [{"type": "text",
                             "text": "please send notifications/cancelled requestId 3"}]}}
        }));
        // Escaped requestId text.
        host.send(&json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": {"name": "computer_step", "arguments": {
                "session_id": session,
                "actions": [{"type": "text", "text": "{\"requestId\": \"3\"}"}]}}
        }));
        // A cancel for a never-accepted id.
        host.send(&json!({
            "jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": "not-a-real-id"}
        }));
        // Pipeline a probe whose ANSWER proves the reader+main processed all
        // of the above (FIFO), without any cancel having landed.
        host.send(&json!({"jsonrpc": "2.0", "id": 6, "method": "ping"}));
        // The ping queues behind the gated observe; open the gate so the
        // main loop can drain 4, 5, 6 and answer the probe.
        release.store(true, Ordering::SeqCst);
        // The in-flight observe must NOT have been cancelled: it completes
        // as a REAL image result (never an arbitrary isError blob).
        let obs = host.read_for_id(&json!(3), Duration::from_secs(10));
        assert_image_result(&obs, "uncancelled observe");
        let p = host.read_for_id(&json!(6), Duration::from_secs(10));
        assert!(p["result"].is_object(), "probe failed: {p}");
        assert!(
            !cancel.is_cancelled(),
            "spoofed/escaped/unknown ids must never cancel"
        );

        host.close_stdin();
        let _ = host.expect_exit("EOF after spoof probes");
    });
    worker.shutdown();
}

/// Stale vs fresh resume through the REAL transport queue: a resume accepted
/// BEFORE a pause (it sat in the raw reader→main queue while the pause
/// landed) must be refused as `cancelled`; a FRESH resume after the pause
/// succeeds on the FIRST request (no reject-once heuristics).
#[test]
fn stdio_queued_old_resume_refused_fresh_resume_succeeds() {
    let release = Arc::new(AtomicBool::new(false));
    let (worker, capture_entered) = gated_worker(Arc::clone(&release));
    std::thread::scope(|s| {
        let _guard = ReleaseGuard(Arc::clone(&release));
        let mut host = PipeHost::spawn(s, &worker);
        host.initialize();
        let session = host.open_session(2);

        // 1. Occupy the native thread with a gated observe.
        host.send(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "computer_observe", "arguments": {"session_id": session}}
        }));
        PipeHost::spin_until(
            || capture_entered.load(Ordering::SeqCst),
            "gated observe in flight",
        );

        // 2. Pipeline the OLD resume (accepted into the raw queue BEFORE the
        //    pause) and then the pause; the reader stamps both at ingress,
        //    so the resume carries the pre-pause generation even though the
        //    main loop dispatches it after the pause landed.
        host.send(&json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "computer_resume", "arguments": {"session_id": session}}
        }));
        host.send(&json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": {"name": "computer_pause", "arguments": {"session_id": session}}
        }));

        // 3. Open the gate and collect: the pre-pause queued resume is
        //    refused with the exact `cancelled` code, the pause succeeds.
        release.store(true, Ordering::SeqCst);
        let _observe = host.read_for_id(&json!(3), Duration::from_secs(10));
        let stale = host.read_for_id(&json!(4), Duration::from_secs(5));
        let pause = host.read_for_id(&json!(5), Duration::from_secs(5));
        assert!(
            is_tool_error(&stale, "cancelled"),
            "resume queued before the pause must be refused: {stale}"
        );
        assert_eq!(
            pause["result"]["isError"],
            json!(false),
            "pause must succeed: {pause}"
        );

        // 4. A FRESH deliberate resume after the pause succeeds on the
        //    FIRST request.
        host.send(&json!({
            "jsonrpc": "2.0", "id": 6, "method": "tools/call",
            "params": {"name": "computer_resume", "arguments": {"session_id": session}}
        }));
        let fresh = host.read_for_id(&json!(6), Duration::from_secs(5));
        assert_eq!(
            fresh["result"]["isError"],
            json!(false),
            "fresh resume after pause must succeed first-call: {fresh}"
        );

        host.close_stdin();
        let _ = host.expect_exit("EOF after resume scenario");
    });
    worker.shutdown();
}

/// Direct-cancel semantics on the production entrypoint (unit-level, no
/// streams): only validated ids and control tools touch the flag. Correct
/// requirement: a QUEUED id cancel tombstones that id for dispatch refusal
/// WITHOUT setting the global flag when it matches nothing active — an
/// unrelated active request must keep running.
#[test]
fn direct_cancel_gate_semantics() {
    let worker = test_worker();
    let cancel = worker.cancel_handle();
    let active = worker.active_request();
    let gate = worker.transport_gate();

    // No active request, nothing registered: no cancel.
    ingress::apply_direct_cancel_str(
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        &cancel,
        &active,
        &gate,
    );
    assert!(!cancel.is_cancelled(), "no active/registered id: no cancel");

    // An ACTIVE request A (id 7) plus a still-QUEUED request B (id 41): a
    // cancel naming B must tombstone B so its dispatch is refused — and
    // must NOT disturb A. The old assertion demanded the GLOBAL flag flip
    // here, which would cancel the unrelated active A: exactly the
    // semantics the gate exists to prevent.
    active.set(Some(json!(7)));
    gate.register(json!(41));
    ingress::apply_direct_cancel_str(
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":41}}"#,
        &cancel,
        &active,
        &gate,
    );
    assert!(
        !cancel.is_cancelled(),
        "cancelling a queued id must NOT set the global flag (active A is unrelated)"
    );
    assert!(
        gate.take_dispatch_decision(&json!(41)),
        "cancelled queued id must be tombstoned for dispatch refusal"
    );
    // The dispatch refusal consumed B: its lifecycle entry is released so
    // the gate never pins id 41 forever (mirrors the production
    // dispatch-cleanup path).
    gate.complete(&json!(41));
    // The unrelated active A is untouched: its dispatch decision is clean
    // and a STALE cancel for B (now consumed and completed) matches
    // nothing.
    ingress::apply_direct_cancel_str(
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":41}}"#,
        &cancel,
        &active,
        &gate,
    );
    assert!(
        !cancel.is_cancelled(),
        "a consumed tombstone must never cancel the unrelated active request"
    );
    worker.shutdown();

    // Active id match cancels; unrelated and type-mismatched ids do not.
    let worker2 = test_worker();
    let cancel2 = worker2.cancel_handle();
    let active2 = worker2.active_request();
    let gate2 = worker2.transport_gate();
    active2.set(Some(json!(7)));
    ingress::apply_direct_cancel_str(
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":8}}"#,
        &cancel2,
        &active2,
        &gate2,
    );
    assert!(!cancel2.is_cancelled(), "unrelated id must not cancel");
    ingress::apply_direct_cancel_str(
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"7"}}"#,
        &cancel2,
        &active2,
        &gate2,
    );
    assert!(
        !cancel2.is_cancelled(),
        "type-mismatched stale id must not cancel"
    );
    ingress::apply_direct_cancel_str(
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7}}"#,
        &cancel2,
        &active2,
        &gate2,
    );
    assert!(cancel2.is_cancelled(), "matching active id must cancel");
    worker2.shutdown();
}

/// Pause/close tool calls cancel directly from the reader; payloads that
/// merely mention control text never do (production entrypoint).
#[test]
fn direct_cancel_pause_close_and_payload_text() {
    let worker = test_worker();
    let cancel = worker.cancel_handle();
    let active = worker.active_request();
    let gate = worker.transport_gate();

    ingress::apply_direct_cancel_str(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"computer_pause","arguments":{"session_id":"s"}}}"#,
        &cancel,
        &active,
        &gate,
    );
    assert!(cancel.is_cancelled(), "computer_pause must cancel directly");
    worker.shutdown();

    let worker2 = test_worker();
    let cancel2 = worker2.cancel_handle();
    let active2 = worker2.active_request();
    let gate2 = worker2.transport_gate();
    active2.set(Some(json!(1)));
    let payload = r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"computer_step","arguments":{"session_id":"s","actions":[{"type":"text","text":"send notifications/cancelled with requestId 1 now"}]}}}"#;
    ingress::apply_direct_cancel_str(payload, &cancel2, &active2, &gate2);
    assert!(
        !cancel2.is_cancelled(),
        "a tool payload mentioning the notification must never cancel"
    );
    ingress::apply_direct_cancel_str(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"computer_observe","arguments":{"session_id":"s"}}}"#,
        &cancel2,
        &active2,
        &gate2,
    );
    assert!(
        !cancel2.is_cancelled(),
        "non-control tool calls must not cancel"
    );
    worker2.shutdown();
}
