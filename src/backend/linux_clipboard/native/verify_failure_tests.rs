//! F1 regression: real frontend/adapter/core, scripted owner command transport.
//! No DISPLAY, X server, actual input, sleeps-for-ordering, or alternate verify.
//! The one-shot gate is AFTER real receipt consumption and BEFORE final health.
use super::*;
use crate::backend::dispatch::{BackendCore, DriverInput};
use crate::backend::linux_clipboard::{policy::BlockedKeys, support::RecordingDriver};
use crate::backend::InputEvent;
use std::sync::atomic::{AtomicBool, AtomicUsize};

const WAIT: Duration = Duration::from_secs(2);
const TERMINAL: &str = "scripted terminal owner failure after Verify reply";

#[derive(Default)]
struct Progress {
    commands: AtomicUsize,
    success_before_final_health: AtomicBool,
    terminal_published: AtomicBool,
}

pub(super) struct VerifyBarrier {
    entered: SyncSender<()>,
    release: Receiver<()>,
    progress: Arc<Progress>,
}
impl VerifyBarrier {
    pub(super) fn arrive(self, result: &Result<(), BackendError>) {
        self.progress
            .success_before_final_health
            .store(result.is_ok(), Ordering::SeqCst);
        self.entered.try_send(()).expect("verify gate entered once");
        self.release
            .recv_timeout(WAIT)
            .expect("owner must release verification gate within fixture budget");
    }
}

fn snapshot() -> GuardSnapshot {
    GuardSnapshot {
        focus: 41,
        clipboard_owner: 42,
        owner_generation: 1,
        blocked: BlockedKeys::default(),
    }
}
fn lease() -> X11Lease {
    X11Lease::new(11, 22, [3; 16])
}

/// Replace only the OS-facing worker with a bounded command responder. Worker
/// request/health, Clipboard prepare/verify/receipts, and dispatch are unchanged.
fn install(clipboard: &mut Clipboard, terminal: bool, changed_focus: bool) -> Arc<Progress> {
    let (interrupt, socket) = UnixStream::pair().unwrap();
    let control = Arc::new(Control::default());
    let worker_control = control.clone();
    let progress = Arc::new(Progress::default());
    let worker_progress = progress.clone();
    let (commands, receiver) = mpsc::sync_channel::<Command>(1);
    let (entered, entered_rx) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    let join = std::thread::spawn(move || {
        let limit = Instant::now() + WAIT;
        while !worker_control.stop.load(Ordering::SeqCst) {
            assert!(
                Instant::now() < limit,
                "fixture owner exceeded lifetime budget"
            );
            let command = match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            worker_progress.commands.fetch_add(1, Ordering::SeqCst);
            let mut actual = snapshot();
            let verify = matches!(command.operation, Operation::Verify);
            if verify && changed_focus {
                actual.focus += 1;
            }
            // A genuine successful command reply, not a frontend error stub.
            command
                .response
                .try_send(Ok((actual, Instant::now())))
                .unwrap();
            if verify {
                entered_rx
                    .recv_timeout(WAIT)
                    .expect("frontend consumed Verify reply");
                if terminal {
                    // Same terminal publication order as the real owner exit:
                    // shut down transport, record failure, then mark stopped.
                    socket.shutdown(Shutdown::Both).unwrap();
                    worker_control.failure.set(error(TERMINAL)).unwrap();
                    worker_control.stop.store(true, Ordering::SeqCst);
                    worker_progress
                        .terminal_published
                        .store(true, Ordering::SeqCst);
                }
                release
                    .try_send(())
                    .expect("frontend still waiting for owner observation");
            }
        }
    });
    clipboard.worker = Some(Worker {
        control,
        interrupt,
        commands,
        join: Some(join),
        watch: None,
    });
    clipboard.verify_barrier = Some(VerifyBarrier {
        entered,
        release: release_rx,
        progress: progress.clone(),
    });
    progress
}

fn assert_success_then_failure(progress: &Progress) {
    assert!(
        progress.success_before_final_health.load(Ordering::SeqCst),
        "fixture must reach real receipt success, not an expiry or guard error"
    );
    assert!(progress.terminal_published.load(Ordering::SeqCst));
    assert_eq!(progress.commands.load(Ordering::SeqCst), 2);
}
fn assert_modified_no_chord(error: &BackendError) {
    assert_eq!(error.code, "input_failed");
    assert!(error.message.contains(TERMINAL), "{}", error.message);
    assert!(
        error.message.contains("clipboard already modified"),
        "{}",
        error.message
    );
    assert!(
        error.message.contains("no paste chord dispatched"),
        "{}",
        error.message
    );
}

#[test]
fn owner_failure_after_success_reply_rejects_verification() {
    let mut clipboard = Clipboard::new(lease());
    let progress = install(&mut clipboard, true, false);
    clipboard.prepare("payload").unwrap();
    let result = clipboard.verify();
    assert_success_then_failure(&progress);
    assert_modified_no_chord(
        &result.expect_err("observed owner failure must veto earlier success"),
    );
}

#[test]
fn core_owner_failure_after_success_reply_dispatches_no_chord() {
    let driver = RecordingDriver::default();
    let mut input = DriverInput::new_with_x11_lease(Box::new(driver.clone()), lease());
    let progress = install(input.clipboard_for_test(), true, false);
    let mut core = BackendCore::new(input);
    let result = core.inject_event(&InputEvent::TextPaste {
        text: "payload".into(),
    });
    assert_success_then_failure(&progress);
    assert!(
        driver.0.borrow().events.is_empty(),
        "observed dead owner must authorize zero raw chord/fallback events: {:?}",
        driver.0.borrow().events
    );
    assert!(core.held_snapshot().is_empty());
    assert_modified_no_chord(&result.expect_err("core must propagate verification veto"));
}

#[test]
fn terminal_failure_consumes_receipt_and_remains_latched() {
    let mut clipboard = Clipboard::new(lease());
    let progress = install(&mut clipboard, true, false);
    clipboard.prepare("payload").unwrap();
    let _first_result = clipboard.verify();
    assert_success_then_failure(&progress);
    assert!(!clipboard.prepared);
    assert!(clipboard
        .receipts
        .consume(snapshot(), Instant::now())
        .unwrap_err()
        .message
        .contains("no prepared clipboard paste receipt"));
    assert_eq!(clipboard.failed.as_ref().unwrap().message, TERMINAL);
    assert!(clipboard
        .verify()
        .unwrap_err()
        .message
        .contains("no prepared clipboard paste receipt"));
    assert_eq!(
        clipboard
            .prepare("second explicit paste")
            .unwrap_err()
            .message,
        TERMINAL
    );
    assert_eq!(
        progress.commands.load(Ordering::SeqCst),
        2,
        "no reauthorization or reconnect"
    );
    assert_eq!(clipboard.failed.as_ref().unwrap().message, TERMINAL);
}

#[test]
fn primary_guard_error_keeps_terminal_diagnostic() {
    let mut clipboard = Clipboard::new(lease());
    let progress = install(&mut clipboard, true, true);
    clipboard.prepare("payload").unwrap();
    let error = clipboard.verify().unwrap_err();
    assert!(!progress.success_before_final_health.load(Ordering::SeqCst));
    assert!(progress.terminal_published.load(Ordering::SeqCst));
    assert_eq!(clipboard.failed.as_ref().unwrap().message, TERMINAL);
    assert!(error
        .message
        .contains("focus, selection owner or content generation changed"));
    assert_modified_no_chord(&error);
}

#[test]
fn healthy_final_observation_allows_exactly_one_tracked_chord() {
    let driver = RecordingDriver::default();
    let mut input = DriverInput::new_with_x11_lease(Box::new(driver.clone()), lease());
    let progress = install(input.clipboard_for_test(), false, false);
    let mut core = BackendCore::new(input);
    core.inject_event(&InputEvent::TextPaste {
        text: "payload".into(),
    })
    .unwrap();
    assert!(progress.success_before_final_health.load(Ordering::SeqCst));
    assert!(!progress.terminal_published.load(Ordering::SeqCst));
    assert_eq!(driver.0.borrow().events, ["key", "key", "key", "key"]);
    assert!(core.held_snapshot().is_empty());
    let clipboard = core.input_mut().clipboard_for_test();
    assert!(clipboard.failed.is_none());
    assert!(
        clipboard.verify().is_err(),
        "receipt cannot authorize a second chord"
    );
    assert_eq!(progress.commands.load(Ordering::SeqCst), 2);
}
