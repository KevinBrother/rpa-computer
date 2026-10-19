use super::super::{coordinator_main, Command, Control, CoordShared, ShutdownStatus};
use super::*;
use crate::backend::{Capture, Geometry, InputEvent};
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{channel, sync_channel};
use std::time::Duration;

// Deliberately longer than the injected 120ms init budget. A gated
// constructor—not an OS call or sleep—is the source of the stalled init.
const INIT_OBSERVATION_BOUND: Duration = Duration::from_secs(3);

struct ProbeBackend {
    geometry_calls: Arc<AtomicUsize>,
    dropped: Sender<()>,
}
impl Backend for ProbeBackend {
    fn platform(&self) -> &'static str {
        "init-probe"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.geometry_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Geometry {
            surface_id: "init-probe".into(),
            input_origin: (0, 0),
            input_size: (800, 600),
            version: "v1".into(),
        })
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        panic!("initialization regression must not capture")
    }
    fn inject(&mut self, _: &InputEvent) -> Result<(), BackendError> {
        panic!("initialization regression must not inject input")
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}
impl Drop for ProbeBackend {
    fn drop(&mut self) {
        let _ = self.dropped.send(());
    }
}

struct ObservedInit {
    result: Result<Result<(), super::super::WorkerError>, std::sync::mpsc::RecvTimeoutError>,
    cancelled_at_deadline: bool,
    status_at_deadline: Option<ShutdownStatus>,
    late_geometry_calls: usize,
    queued_call_succeeded: bool,
}

fn gated_init(request_stop: bool) -> ObservedInit {
    let (entered_tx, entered_rx) = channel();
    let (release_tx, release_rx) = channel();
    let (dropped_tx, dropped_rx) = channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let backend_calls = calls.clone();
    let factory = BackendFactory::Test(Box::new(move || {
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        Ok(Box::new(ProbeBackend {
            geometry_calls: backend_calls,
            dropped: dropped_tx,
        }))
    }));
    let (tx, rx) = sync_channel(1);
    let (ctrl_tx, ctrl_rx) = channel();
    let (init_tx, init_rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let shutdown_status = Arc::new(Mutex::new(None));
    let shared = CoordShared {
        init_deadline: Instant::now() + Duration::from_millis(120),
        cancel: cancel.clone(),
        generation: RequestGeneration::new_for_test(),
        session_started: Arc::new(Mutex::new(None)),
        shutdown_status: shutdown_status.clone(),
        stash_len: Arc::new(AtomicUsize::new(0)),
        feedback: None,
    };
    let coordinator =
        std::thread::spawn(move || coordinator_main(factory, rx, ctrl_rx, init_tx, shared));
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (reply, reply_rx) = channel();
    tx.send(Command::Call {
        name: "computer_open".into(),
        args: serde_json::json!({}),
        reply,
        gen: 0,
    })
    .unwrap();
    if request_stop {
        ctrl_tx.send(Control::Quit).unwrap();
    }

    let result = init_rx.recv_timeout(INIT_OBSERVATION_BOUND);
    let cancelled_at_deadline = cancel.load(Ordering::SeqCst);
    let status_at_deadline = *shutdown_status.lock().unwrap();
    // Always release before any regression assertion, including RED, so
    // the validator never retains a stuck test thread after a failure.
    release_tx.send(()).unwrap();
    let queued_call_succeeded = reply_rx
        .recv_timeout(Duration::from_secs(1))
        .is_ok_and(|reply| !reply.is_error);
    let _ = ctrl_tx.send(Control::Quit);
    let until = Instant::now() + Duration::from_secs(4);
    while !coordinator.is_finished() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        coordinator.is_finished(),
        "fixture coordinator teardown exceeded bound"
    );
    coordinator.join().unwrap();
    dropped_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("late backend is disposed");
    ObservedInit {
        result,
        cancelled_at_deadline,
        status_at_deadline,
        late_geometry_calls: calls.load(Ordering::SeqCst),
        queued_call_succeeded,
    }
}

#[test]
fn native_initialization_has_a_deadline_and_reports_unknown_cleanup() {
    let observed = gated_init(false);
    let error = observed
        .result
        .expect("coordinator must bound native initialization")
        .expect_err("a blocked constructor cannot report readiness");
    assert!(
        matches!(
            error,
            super::super::WorkerError::InitializationUncertain {
                code: "initialization_timeout",
                ..
            }
        ),
        "expected an uncertain initialization timeout, got {error:?}"
    );
    assert!(
        observed.cancelled_at_deadline,
        "deadline must latch cancellation"
    );
    assert_eq!(observed.status_at_deadline, Some(ShutdownStatus::Unknown));
}

#[test]
fn late_initialization_never_replays_queued_open() {
    let observed = gated_init(false);
    assert_eq!(
        observed.late_geometry_calls, 0,
        "expired init executed queued backend work"
    );
    assert!(
        !observed.queued_call_succeeded,
        "queued open must not become a late success"
    );
}

#[test]
fn quit_during_initialization_is_bounded_and_not_clean() {
    let observed = gated_init(true);
    assert!(
        matches!(observed.result, Ok(Err(_))),
        "stop cannot wait for a stalled constructor"
    );
    assert!(observed.cancelled_at_deadline);
    assert_eq!(observed.status_at_deadline, Some(ShutdownStatus::Unknown));
    assert_eq!(observed.late_geometry_calls, 0);
}
