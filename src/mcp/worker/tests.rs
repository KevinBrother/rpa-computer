//! Worker tests: real two-thread workers with fake backends, deterministic
//! gating (flags, never sleeps-as-synchronization on the critical assertion),
//! injected deadlines, and always-release guards so a panicking test can
//! never hang the test binary.

use super::{Worker, WorkerError, MAX_QUEUED_COMMANDS};
use crate::backend::{Backend, BackendError, Capture, Direction, Geometry, InputEvent};
use crate::mcp::backend_factory::BackendFactory;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;
use std::time::Duration;

/// Minimal fake backend for transport tests. Test-only, constructed
/// exclusively through the explicit `BackendFactory::Test` gate.
struct FakeBackend {
    events: StdMutex<Vec<String>>,
    released: StdMutex<bool>,
    fail_capture: bool,
}

impl FakeBackend {
    fn new() -> Self {
        FakeBackend {
            events: StdMutex::new(Vec::new()),
            released: StdMutex::new(false),
            fail_capture: false,
        }
    }
}

impl Backend for FakeBackend {
    fn platform(&self) -> &'static str {
        "fake"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        Ok(Geometry {
            surface_id: "fake-surface".into(),
            input_origin: (0, 0),
            input_size: (800, 600),
            version: "v1".into(),
        })
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        if self.fail_capture {
            return Err(BackendError {
                code: "capture_error".into(),
                message: "injected capture failure".into(),
            });
        }
        // 64x64: at/above the runtime's MIN_IMAGE_DIM so the capture
        // survives geometry validation through the real observe path.
        let (w, h) = (64u32, 64u32);
        Ok(Capture {
            png: valid_test_png(w, h),
            width: w,
            height: h,
            geometry: self.geometry()?,
        })
    }
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        let desc = match event {
            InputEvent::Move { x, y } => format!("move {x},{y}"),
            InputEvent::Button {
                button,
                direction,
                ..
            } => format!(
                "button {button} {}",
                if *direction == Direction::Press {
                    "down"
                } else {
                    "up"
                }
            ),
            InputEvent::Key { key, direction } => format!(
                "key {key} {}",
                if *direction == Direction::Press {
                    "down"
                } else {
                    "up"
                }
            ),
            InputEvent::Text { text } => format!("text len={}", text.chars().count()),
            InputEvent::Scroll { x, y } => format!("scroll {x},{y}"),
        };
        self.events.lock().unwrap().push(desc);
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        *self.released.lock().unwrap() = true;
        Ok(())
    }
}

/// A real, decodable PNG produced locally (no external fixture) at the
/// requested size. The previous hand-rolled byte constant had a wrong
/// IDAT CRC and made the runtime reject the capture; dimensions must be
/// at least the runtime's MIN_IMAGE_DIM so geometry validation accepts
/// them — regression coverage lives in
/// `observe_returns_image_through_worker`.
fn valid_test_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([7, 8, 9, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn test_worker() -> Worker {
    Worker::start(BackendFactory::Test(Box::new(|| {
        Ok(Box::new(FakeBackend::new()))
    })))
    .expect("test worker starts")
}

#[test]
fn worker_executes_describe() {
    let w = test_worker();
    let reply = w.call("computer_describe", serde_json::json!({})).unwrap();
    assert!(!reply.is_error, "describe failed: {:?}", reply.data);
    w.shutdown();
}

#[test]
fn unknown_tool_is_error_reply() {
    let w = test_worker();
    let reply = w.call("computer_nope", serde_json::json!({})).unwrap();
    assert!(reply.is_error);
    w.shutdown();
}

#[test]
fn cancel_handle_sets_flag_without_blocking() {
    let w = test_worker();
    let h = w.cancel_handle();
    assert!(!h.is_cancelled());
    h.cancel();
    assert!(h.is_cancelled());
    assert!(h.is_cancelled());
    w.shutdown();
}

#[test]
fn shutdown_is_idempotent() {
    let w = test_worker();
    let first = w.shutdown();
    let second = w.shutdown();
    assert_eq!(first, second);
}

#[test]
fn backend_error_at_start_is_reported() {
    let w = Worker::start(BackendFactory::Test(Box::new(|| {
        Err(BackendError {
            code: "desktop_unavailable".into(),
            message: "injected init failure".into(),
        })
    })));
    match w {
        Err(WorkerError::BackendUnavailable { code, .. }) => {
            assert_eq!(code, "desktop_unavailable")
        }
        other => panic!("expected backend failure, got {other:?}"),
    }
}

#[test]
fn session_marker_tracks_open_close() {
    let w = test_worker();
    assert!(!w.session_active());
    let open = w.call("computer_open", serde_json::json!({})).unwrap();
    assert!(!open.is_error, "open failed: {:?}", open.data);
    assert!(w.session_active());
    let sid = open.data["session_id"].as_str().unwrap().to_string();
    let close = w
        .call("computer_close", serde_json::json!({"session_id": sid}))
        .unwrap();
    assert!(!close.is_error, "close failed: {:?}", close.data);
    assert!(!w.session_active());
    w.shutdown();
}

#[test]
fn observe_returns_image_through_worker() {
    let w = test_worker();
    let open = w.call("computer_open", serde_json::json!({})).unwrap();
    let sid = open.data["session_id"].as_str().unwrap().to_string();
    let obs = w
        .call("computer_observe", serde_json::json!({"session_id": sid}))
        .unwrap();
    assert!(!obs.is_error, "observe failed: {:?}", obs.data);
    assert!(obs.image_png.is_some());
    assert_eq!(obs.data["width_px"], serde_json::json!(64));
    w.shutdown();
}

/// Release guard: even if the test panics mid-way, the gated native
/// call is always unblocked so worker shutdown in `Drop` can never hang
/// the test binary.
struct ReleaseGuard(std::sync::Arc<AtomicBool>);
impl Drop for ReleaseGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Gated fake backend: `capture` blocks until released; `capture_entered`
/// flips true the moment the gated capture starts, so tests can KNOW the
/// native thread is occupied (deterministic handshake, never a sleep).
struct GatedBackend {
    inner: FakeBackend,
    release: std::sync::Arc<AtomicBool>,
    capture_entered: std::sync::Arc<AtomicBool>,
}
impl Backend for GatedBackend {
    fn platform(&self) -> &'static str {
        "fake"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.inner.geometry()
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        // Announce the dispatch, then hold the native thread until the
        // test releases it.
        self.capture_entered.store(true, Ordering::SeqCst);
        while !self.release.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.inner.capture()
    }
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        self.inner.inject(event)
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        self.inner.release_all()
    }
}

/// A worker whose native `capture` is gated, plus the handshake flag that
/// proves a capture has been dispatched to the native thread.
fn gated_worker_with_probe(
    release: std::sync::Arc<AtomicBool>,
) -> (Worker, std::sync::Arc<AtomicBool>) {
    let capture_entered = std::sync::Arc::new(AtomicBool::new(false));
    let probe = std::sync::Arc::clone(&capture_entered);
    let w = Worker::start(BackendFactory::Test(Box::new(move || {
        Ok(Box::new(GatedBackend {
            inner: FakeBackend::new(),
            release,
            capture_entered,
        }))
    })))
    .expect("gated worker starts");
    (w, probe)
}

/// Bounded spin helper: waits until `cond` holds or panics after ~5s.
/// Never used as proof of an assertion — only to reach a deterministic
/// state before the real assertion runs.
fn spin_until(mut cond: impl FnMut() -> bool, what: &str) {
    let mut spins = 0;
    while !cond() {
        std::thread::sleep(Duration::from_millis(1));
        spins += 1;
        if spins > 5000 {
            panic!("timed out waiting for {what}");
        }
    }
}

#[test]
fn queued_open_after_cancel_is_refused() {
    // Approved contract (see module docs): an open/resume QUEUED BEFORE
    // a stop must be refused once the stop lands; a FRESH, deliberate
    // resume after the safe pause must succeed. Deterministic, single
    // thread, NO flood loop and NO deadline on the dependency path:
    // `enqueue` gives a synchronous ADMISSION decision plus a reply
    // Receiver, so a call is provably queued (native gate still closed)
    // without ever waiting for it.
    let release = std::sync::Arc::new(AtomicBool::new(false));
    let (w, capture_entered) = gated_worker_with_probe(std::sync::Arc::clone(&release));
    let _guard = ReleaseGuard(std::sync::Arc::clone(&release));
    let open = w.call("computer_open", json!({})).unwrap();
    assert!(!open.is_error, "open failed: {:?}", open.data);
    let sid = open.data["session_id"].as_str().unwrap().to_string();

    // 1. Occupy the native thread with a gated observe (admission only;
    //    the reply is collected after the gate opens).
    let observe_rx = enqueue_ok(&w, "computer_observe", json!({"session_id": sid}));
    spin_until(
        || capture_entered.load(Ordering::SeqCst),
        "gated observe to occupy the native thread",
    );

    // 2. Queue a resume behind it (pre-stop). Admission succeeding while
    //    the gate is closed PROVES it is queued; no stash peeking needed.
    let stale_resume_rx = enqueue_ok(&w, "computer_resume", json!({"session_id": sid}));

    // 3. Stop lands while the resume is still queued: the generation
    //    bumps, so the queued resume is now stamped with an old epoch.
    w.cancel_handle().cancel();
    let pause_rx = enqueue_ok(&w, "computer_pause", json!({"session_id": sid}));

    // 4. Open the gate BEFORE waiting for any reply: no dependency cycle.
    release.store(true, Ordering::SeqCst);
    let observe = recv_bounded(observe_rx, "observe");
    let stale = recv_bounded(stale_resume_rx, "queued resume");
    let pause = recv_bounded(pause_rx, "pause");

    assert!(
        !pause.is_error,
        "pause behind a busy worker must complete: {:?}",
        pause.data
    );
    let _ = observe; // may report cancelled; only its RETURN matters
    assert_eq!(
        stale.data["error"]["code"],
        json!("cancelled"),
        "resume queued before the stop must be refused (got {:?})",
        stale.data
    );

    // 5. A FRESH explicit resume after the safe pause must succeed.
    let fresh = w
        .call("computer_resume", json!({"session_id": sid}))
        .unwrap();
    assert!(
        !fresh.is_error,
        "a fresh deliberate resume after a safe pause must succeed: {:?}",
        fresh.data
    );
    w.shutdown();
}

/// Enqueue through the production admission path and require acceptance.
/// `Reply` has no Debug impl, so admission errors are surfaced with an
/// explicit match (test helper, not production unwrap).
fn enqueue_ok(
    w: &Worker,
    name: &str,
    args: serde_json::Value,
) -> std::sync::mpsc::Receiver<crate::runtime::Reply> {
    match w.enqueue(name, args) {
        Ok(rx) => rx,
        Err(err) => panic!(
            "{name} admission failed (is_error={}): {:?}",
            err.is_error, err.data
        ),
    }
}

/// Collect a queued reply with a generous bounded wait: the gate is
/// already open, so a genuine native call returns well within this.
fn recv_bounded(
    rx: std::sync::mpsc::Receiver<crate::runtime::Reply>,
    what: &str,
) -> crate::runtime::Reply {
    rx.recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|e| panic!("{what} reply never arrived: {e}"))
}

#[test]
fn invalid_close_does_not_clear_session_timer() {
    let w = test_worker();
    let open = w.call("computer_open", serde_json::json!({})).unwrap();
    assert!(!open.is_error);
    assert!(w.session_active());
    // Close with a WRONG session id fails and must not clear the timer.
    let bad = w
        .call("computer_close", serde_json::json!({"session_id": "bogus"}))
        .unwrap();
    assert!(bad.is_error);
    assert!(
        w.session_active(),
        "failed close must not clear the session clock"
    );
    w.shutdown();
}

#[test]
fn failed_open_does_not_arm_session_timer() {
    let w = test_worker();
    // An invalid open (bad args) must not arm the clock.
    let bad = w
        .call("computer_open", serde_json::json!({"max_width": 0}))
        .unwrap();
    assert!(bad.is_error);
    assert!(!w.session_active(), "failed open must not arm the clock");
    w.shutdown();
}

#[test]
fn active_request_validated_cancel() {
    let w = test_worker();
    let active = w.active_request();
    let cancel = w.cancel_handle();
    active.set(Some(serde_json::json!(7)));
    assert!(active.matches(&serde_json::json!(7)));
    assert!(!active.matches(&serde_json::json!(8)));
    assert!(!cancel.is_cancelled()); // matching alone never sets the flag
    active.set(None);
    assert!(!active.matches(&serde_json::json!(7)));
    w.shutdown();
}

#[test]
fn call_queue_is_bounded() {
    // The worker must refuse with server_busy instead of growing an
    // unbounded backlog. Deterministic, single thread, NO flood and NO
    // reply deadline on the dependency path: `enqueue` gives the
    // ADMISSION decision synchronously, so the pending population can be
    // filled exactly to the cap and overflow proven while the gated
    // native thread provably holds the in-flight slot. Quarantine is a
    // separate concern and never a premise here: the gate opens BEFORE
    // any deadline could fire, every accepted call completes genuinely,
    // and the worker stays healthy.
    let release = std::sync::Arc::new(AtomicBool::new(false));
    let (w, capture_entered) = gated_worker_with_probe(std::sync::Arc::clone(&release));
    let _guard = ReleaseGuard(std::sync::Arc::clone(&release));
    let open = w.call("computer_open", json!({})).unwrap();
    assert!(!open.is_error, "open failed: {:?}", open.data);
    let sid = open.data["session_id"].as_str().unwrap().to_string();

    // Occupy the native thread (admission only, no wait).
    let observe_rx = enqueue_ok(&w, "computer_observe", json!({"session_id": sid}));
    spin_until(
        || capture_entered.load(Ordering::SeqCst),
        "gated observe to occupy the native thread",
    );

    // Fill the REAL pending population to exactly the cap. Each accepted
    // call's Receiver is retained so nothing is dropped/refused early.
    let mut pending = Vec::with_capacity(MAX_QUEUED_COMMANDS);
    for i in 0..MAX_QUEUED_COMMANDS {
        pending.push(enqueue_ok(&w, "computer_describe", json!({})));
        assert_eq!(pending.len(), i + 1);
    }

    // The very next admissions overflow the REAL pending population:
    // immediate server_busy from the admission path (not a silent queue,
    // not a timeout), and refusals persist while nothing drains.
    for probe in ["first overflow", "second overflow"] {
        match w.enqueue("computer_describe", json!({})) {
            Err(err) => assert_eq!(
                err.data["error"]["code"],
                json!("server_busy"),
                "{probe}: call beyond the pending cap must be server_busy, got {:?}",
                err.data
            ),
            Ok(_) => panic!("{probe}: admission beyond the cap was accepted"),
        }
    }

    // Open the gate BEFORE collecting anything: no deadline ever fires,
    // no fault, every accepted call completes genuinely.
    release.store(true, Ordering::SeqCst);
    let observe = recv_bounded(observe_rx, "observe");
    assert!(
        !observe.is_error,
        "gated observe must complete after release: {:?}",
        observe.data
    );
    for (i, rx) in pending.into_iter().enumerate() {
        let reply = recv_bounded(rx, "queued describe");
        assert!(
            !reply.is_error,
            "accepted describe #{i} must complete genuinely: {:?}",
            reply.data
        );
    }
    assert!(
        !w.is_faulted(),
        "bounded-queue proof must never quarantine the worker"
    );
    w.shutdown();
}
