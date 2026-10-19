use super::*;
use crate::mcp::worker::{Worker, WorkerError};
use std::time::Duration;

struct RetireProbe {
    inner: crate::mcp::mock_backend::MockBackend,
    dropped: Sender<()>,
}
impl Backend for RetireProbe {
    fn platform(&self) -> &'static str {
        self.inner.platform()
    }
    fn geometry(&mut self) -> Result<crate::backend::Geometry, BackendError> {
        self.inner.geometry()
    }
    fn capture(&mut self) -> Result<crate::backend::Capture, BackendError> {
        panic!("expired constructor must not capture")
    }
    fn inject(&mut self, _: &crate::backend::InputEvent) -> Result<(), BackendError> {
        panic!("expired constructor must not inject")
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        self.inner.release_all()
    }
}
impl Drop for RetireProbe {
    fn drop(&mut self) {
        let _ = self.dropped.send(());
    }
}

#[test]
fn outer_start_is_bounded_and_late_constructor_is_disposed() {
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    let start = std::thread::spawn(move || {
        let result = Worker::start_config_with_init_budget(
            BackendFactory::Test(Box::new(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Box::new(RetireProbe {
                    inner: crate::mcp::mock_backend::MockBackend::new(),
                    dropped: dropped_tx,
                }))
            })),
            crate::feedback::FeedbackConfig::default(),
            None,
            Duration::from_millis(120),
        );
        result_tx.send(result).unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let result = result_rx.recv_timeout(Duration::from_secs(2));
    // Unblock before assertions, including the regression's failure path.
    release_tx.send(()).unwrap();
    dropped_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("late constructor must dispose backend");
    start.join().unwrap();
    let error = result
        .expect("outer Worker::start must not wait for the native constructor")
        .expect_err("unconfirmed init must not publish a Worker");
    assert!(matches!(error, WorkerError::InitializationUncertain { .. }));
    assert!(!error.initialization_no_input());
}

#[test]
fn setup_error_with_unretired_native_thread_stays_unknown() {
    let (init_tx, init_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (_ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
    let mut join = Some(std::thread::spawn(move || {
        init_tx
            .send(Err(BackendError::new("no_display", "fixture")))
            .unwrap();
        // Models a native destructor still running after error publication.
        release_rx.recv().unwrap();
    }));
    let result = await_initialization(
        init_rx,
        &mut join,
        &ctrl_rx,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_millis(120),
    );
    release_tx.send(()).unwrap();
    if let Some(join) = join {
        join.join().unwrap();
    }
    let error = result.expect_err("publishing an error does not prove cleanup finished");
    assert!(matches!(
        error,
        WorkerError::InitializationUncertain {
            code: "initialization_cleanup_unknown",
            ..
        }
    ));
    assert!(!error.initialization_no_input());
}

#[test]
fn completed_setup_error_is_distinct_from_constructor_panic() {
    let fail = Worker::start_config_with_init_budget(
        BackendFactory::Test(Box::new(|| Err(BackendError::new("no_display", "fixture")))),
        crate::feedback::FeedbackConfig::default(),
        None,
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(fail.initialization_no_input());
    let panic = Worker::start_config_with_init_budget(
        BackendFactory::Test(Box::new(|| panic!("injected constructor panic"))),
        crate::feedback::FeedbackConfig::default(),
        None,
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(
        !panic.initialization_no_input(),
        "panic must never emit the pre-ready no-input exit"
    );
    assert!(matches!(panic, WorkerError::InitializationUncertain { .. }));
}
