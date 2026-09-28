//! Regression tests for native stop-epoch lifecycle rules:
//! 1. cancelling `computer_resume` while blocked inside `geometry()` keeps the
//!    flag set, surfaces `cancelled`, and a later deliberate resume succeeds;
//! 2. cancelling `computer_observe` while blocked inside `capture()` surfaces
//!    the exact `cancelled` error (never success/capture_error) and the very
//!    next deliberate resume succeeds on the first call;
//! 3. cancelling `computer_open` while blocked inside `geometry()` (where the
//!    runtime already created a session internally) rolls that session back:
//!    the caller sees `cancelled`, a fresh open succeeds on the first call,
//!    and `session_state` is never reported for the hidden session.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rpa_computer::backend::{Backend, BackendError, Capture, Geometry, InputEvent};
use rpa_computer::mcp::backend_factory::BackendFactory;
use rpa_computer::mcp::worker::Worker;
use serde_json::json;

struct Gate {
    armed: Arc<AtomicBool>,
    entered: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
}

fn test_geometry() -> Geometry {
    Geometry {
        surface_id: "s".to_string(),
        input_origin: (0, 0),
        input_size: (16, 16),
        version: "v1".to_string(),
    }
}

/// 16x16 solid PNG (matches the mock geometry input size).
fn test_png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([7, 8, 9, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

struct MockBackend {
    gate: Gate,
}

/// Block (with a safety bound) while the gate is armed. Consumes the arming
/// so exactly one backend call per arming gets stuck. Returns whether it
/// actually blocked; only the blocked call signals `entered`.
fn block_while_armed(gate: &Gate) -> bool {
    if !gate.armed.swap(false, Ordering::SeqCst) {
        return false;
    }
    gate.entered.store(true, Ordering::SeqCst);
    let start = Instant::now();
    while !gate.release.load(Ordering::SeqCst) {
        // Safety bound so a broken test can never hang forever.
        if start.elapsed() > Duration::from_secs(10) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    true
}

impl Backend for MockBackend {
    fn platform(&self) -> &'static str {
        "test"
    }

    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        block_while_armed(&self.gate);
        Ok(test_geometry())
    }

    fn capture(&mut self) -> Result<Capture, BackendError> {
        block_while_armed(&self.gate);
        Ok(Capture {
            png: test_png(),
            width: 16,
            height: 16,
            geometry: test_geometry(),
        })
    }

    fn inject(&mut self, _event: &InputEvent) -> Result<(), BackendError> {
        panic!("inject must not be called in this scenario");
    }

    fn release_all(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}

/// Releases the geometry gate when dropped. Created inside the scoped closure
/// so that, on panic/unwind, the blocked backend thread is released *before*
/// `std::thread::scope` performs its implicit join.
struct ReleaseOnDrop(Arc<AtomicBool>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn resume_cancelled_inside_geometry_keeps_cancel_flag() {
    let armed = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let gate = Gate {
        armed: armed.clone(),
        entered: entered.clone(),
        release: release.clone(),
    };
    let worker = gated_worker(&gate);

    // 1. Successful setup: open, then pause the session.
    let session_id = open_session(&worker);

    let pause = worker
        .call("computer_pause", json!({ "session_id": session_id }))
        .expect("pause transport");
    assert!(!pause.is_error, "computer_pause failed: {:?}", pause.data);

    // Arm the geometry gate so the next resume blocks inside geometry().
    armed.store(true, Ordering::SeqCst);

    // 2. Cancel the resume while it is definitely inside geometry().
    let (worker, resume_reply) = std::thread::scope(|s| {
        let _guard = ReleaseOnDrop(release.clone());
        let cancel_handle = worker.cancel_handle();
        let sid = session_id.clone();

        let handle = s.spawn(move || {
            let reply = worker.call("computer_resume", json!({ "session_id": sid }));
            (worker, reply)
        });

        wait_entered(&entered);

        // Cancel while definitely blocked inside geometry, then let it through.
        cancel_handle.cancel();
        release.store(true, Ordering::SeqCst);

        let (worker, reply) = handle.join().expect("resume thread panicked");

        // 3a. The cancel flag must still be set after the cancelled resume
        // completes (buggy Runtime.resume clears it after geometry()).
        assert!(
            cancel_handle.is_cancelled(),
            "cancel flag was cleared after a resume cancelled inside geometry()"
        );

        (worker, reply.expect("resume transport"))
    });

    // 3b. The cancelled resume must report cancellation, never clean ready.
    assert!(
        resume_reply.is_error,
        "cancelled resume must be an error, got: {:?}",
        resume_reply.data
    );
    assert_eq!(
        resume_reply.data["error"]["code"],
        json!("cancelled"),
        "resume error code must be 'cancelled': {:?}",
        resume_reply.data
    );

    // 3c. Observe must also report the cancellation, never a capture error.
    let observe = worker
        .call("computer_observe", json!({ "session_id": session_id }))
        .expect("observe transport");
    assert!(
        observe.is_error,
        "observe after cancellation must not succeed: {:?}",
        observe.data
    );
    assert_eq!(
        observe.data["error"]["code"],
        json!("cancelled"),
        "observe must report 'cancelled', not 'capture_error': {:?}",
        observe.data
    );

    // 3d. A new deliberate resume (gate now disarmed) must succeed on the
    // first call and require a fresh observation.
    let resume2 = worker
        .call("computer_resume", json!({ "session_id": session_id }))
        .expect("second resume transport");
    assert!(
        !resume2.is_error,
        "deliberate resume after gate disarmed must succeed: {:?}",
        resume2.data
    );
    assert_eq!(
        resume2.data["requires_fresh_observation"],
        json!(true),
        "successful resume must require fresh observation: {:?}",
        resume2.data
    );

    worker.shutdown();
}

/// Start a worker whose backend shares the given gate.
fn gated_worker(gate: &Gate) -> Worker {
    let factory_gate = Gate {
        armed: gate.armed.clone(),
        entered: gate.entered.clone(),
        release: gate.release.clone(),
    };
    Worker::start(BackendFactory::Test(Box::new(move || {
        Ok(Box::new(MockBackend {
            gate: Gate {
                armed: factory_gate.armed.clone(),
                entered: factory_gate.entered.clone(),
                release: factory_gate.release.clone(),
            },
        }))
    })))
    .expect("worker must start")
}

fn open_session(worker: &Worker) -> String {
    let open = worker
        .call("computer_open", json!({}))
        .expect("open transport");
    assert!(!open.is_error, "computer_open failed: {:?}", open.data);
    open.data["session_id"]
        .as_str()
        .expect("open must return session_id")
        .to_string()
}

/// Wait until the gated backend call was actually entered (with bound).
fn wait_entered(entered: &Arc<AtomicBool>) {
    let start = Instant::now();
    while !entered.load(Ordering::SeqCst) {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "gated backend call was not entered within 3s"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Requirement 2: a stop that lands INSIDE `backend.capture()` must turn the
/// observe's (valid) result into the exact `cancelled` error — never a
/// success and never a `capture_error` — and the very next deliberate resume
/// must succeed on the first call (the epoch guard must not rewrite a fresh
/// request stamped with the post-stop generation).
#[test]
fn observe_cancelled_inside_capture_reports_cancelled_not_capture_error() {
    let armed = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let gate = Gate {
        armed: armed.clone(),
        entered: entered.clone(),
        release: release.clone(),
    };
    let worker = gated_worker(&gate);

    // Setup: open, then pause so the follow-up resume validity is observable.
    let session_id = open_session(&worker);
    let pause = worker
        .call("computer_pause", json!({ "session_id": session_id }))
        .expect("pause transport");
    assert!(!pause.is_error, "computer_pause failed: {:?}", pause.data);
    let resume = worker
        .call("computer_resume", json!({ "session_id": session_id }))
        .expect("resume transport");
    assert!(!resume.is_error, "resume failed: {:?}", resume.data);

    // Arm the capture gate so the observe blocks inside capture().
    armed.store(true, Ordering::SeqCst);

    let (worker, observe_reply) = std::thread::scope(|s| {
        let _guard = ReleaseOnDrop(release.clone());
        let cancel_handle = worker.cancel_handle();
        let sid = session_id.clone();

        let handle = s.spawn(move || {
            let reply = worker.call("computer_observe", json!({ "session_id": sid }));
            (worker, reply)
        });

        wait_entered(&entered);

        // Cancel while definitely blocked inside capture, then let it through.
        cancel_handle.cancel();
        release.store(true, Ordering::SeqCst);

        let (worker, reply) = handle.join().expect("observe thread panicked");

        // The cancel flag must survive the observe (the guard re-latches).
        assert!(
            cancel_handle.is_cancelled(),
            "cancel flag was cleared after an observe cancelled inside capture()"
        );
        (worker, reply.expect("observe transport"))
    });

    // The observe's read-only result must be REPLACED by exact `cancelled`:
    // never a successful observation, never a capture_error.
    assert!(
        observe_reply.is_error,
        "cancelled observe must be an error, got: {:?}",
        observe_reply.data
    );
    assert_eq!(
        observe_reply.data["error"]["code"],
        json!("cancelled"),
        "observe error code must be 'cancelled': {:?}",
        observe_reply.data
    );

    // A fresh deliberate resume (stamped AFTER the stop) must succeed on the
    // first call: no stale-epoch rewrite may leak into new requests.
    let resume2 = worker
        .call("computer_resume", json!({ "session_id": session_id }))
        .expect("second resume transport");
    assert!(
        !resume2.is_error,
        "deliberate resume after capture-cancel must succeed: {:?}",
        resume2.data
    );
    assert_eq!(
        resume2.data["requires_fresh_observation"],
        json!(true),
        "successful resume must require fresh observation: {:?}",
        resume2.data
    );

    worker.shutdown();
}

/// Requirement 3: a stop that lands INSIDE `geometry()` during
/// `computer_open` — after the runtime internally created the session — must
/// roll that session back: the caller sees `cancelled`, the lifetime clock is
/// NOT armed, a fresh explicit open succeeds on the first call, and the
/// hidden session is gone (never a `session_state` error).
#[test]
fn open_cancelled_inside_geometry_rolls_back_new_session() {
    let armed = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let gate = Gate {
        armed: armed.clone(),
        entered: entered.clone(),
        release: release.clone(),
    };
    let worker = gated_worker(&gate);

    // Arm the geometry gate so the open blocks inside geometry().
    armed.store(true, Ordering::SeqCst);

    let (worker, open_reply) = std::thread::scope(|s| {
        let _guard = ReleaseOnDrop(release.clone());
        let cancel_handle = worker.cancel_handle();

        let handle = s.spawn(move || {
            let reply = worker.call("computer_open", json!({}));
            (worker, reply)
        });

        wait_entered(&entered);

        // Cancel while definitely blocked inside geometry, then let it through.
        cancel_handle.cancel();
        release.store(true, Ordering::SeqCst);

        let (worker, reply) = handle.join().expect("open thread panicked");

        assert!(
            cancel_handle.is_cancelled(),
            "cancel flag was cleared after an open cancelled inside geometry()"
        );
        (worker, reply.expect("open transport"))
    });

    // The cancelled open must report cancellation, never a ready session.
    assert!(
        open_reply.is_error,
        "cancelled open must be an error, got: {:?}",
        open_reply.data
    );
    assert_eq!(
        open_reply.data["error"]["code"],
        json!("cancelled"),
        "open error code must be 'cancelled': {:?}",
        open_reply.data
    );

    // The internally created session must be rolled back: a fresh explicit
    // open (gate disarmed) succeeds on the FIRST call — a hidden leftover
    // session would surface `session_state` here.
    let open2 = worker
        .call("computer_open", json!({}))
        .expect("second open transport");
    assert!(
        !open2.is_error,
        "fresh open after cancelled open must succeed: {:?}",
        open2.data
    );
    let session2 = open2.data["session_id"]
        .as_str()
        .expect("second open must return session_id")
        .to_string();

    // And it must be fully functional.
    let observe = worker
        .call("computer_observe", json!({ "session_id": session2 }))
        .expect("observe transport");
    assert!(
        !observe.is_error,
        "observe on the fresh session must succeed: {:?}",
        observe.data
    );

    worker.shutdown();
}
