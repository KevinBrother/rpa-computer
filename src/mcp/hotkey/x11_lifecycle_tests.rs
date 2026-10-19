//! No sockets, GUI, timers, or detached threads. Independent CC executes these.
use super::x11_lifecycle::Control;
use super::HotkeyStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

#[test]
fn expiry_winning_while_caller_is_paused_prevents_late_activation() {
    let control = Arc::new(Control::default());
    let cancel = AtomicBool::new(false);
    let (checked_tx, checked_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let caller_control = Arc::clone(&control);
    let now = Instant::now();
    let caller = std::thread::spawn(move || {
        caller_control.accept_ready_at(now + Duration::from_secs(1), now, || {
            checked_tx.send(()).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        })
    });
    let reached = checked_rx.recv_timeout(Duration::from_secs(2));
    // Worker has decided that initialization expired; checked cleanup has not
    // returned. No wall-clock sleep or server is needed to order this race.
    control.begin_failed_cleanup("initialization expired before activation", &cancel);
    let published_before_cleanup = matches!(control.failure.get(), Some(HotkeyStatus::Failed(_)));
    let _ = resume_tx.send(());
    let accepted = caller.join().expect("caller checkpoint must complete");
    assert!(reached.is_ok(), "caller did not reach readiness checkpoint");
    assert!(!accepted, "expiry won but caller still returned Armed");
    assert!(!control.active.load(Ordering::SeqCst));
    assert!(
        published_before_cleanup,
        "failure must precede blocking cleanup"
    );
    assert_eq!(Arc::strong_count(&control), 1);
}

#[test]
fn preactive_failure_is_visible_before_any_cleanup_reply() {
    let control = Control::default();
    let cancel = AtomicBool::new(false);
    control.begin_failed_cleanup("checked grab failed", &cancel);
    assert!(matches!(
        control.failure.get(),
        Some(HotkeyStatus::Failed(_))
    ));
    assert!(
        !cancel.load(Ordering::SeqCst),
        "never-active arm has no session to cancel"
    );
}

#[test]
fn already_failed_registration_cannot_accept_queued_ready() {
    let control = Control::default();
    control
        .failure
        .set(HotkeyStatus::Failed("worker exited".into()))
        .unwrap();
    assert!(!control.accept_ready(Instant::now() + Duration::from_secs(1)));
}

#[test]
fn abandoned_registration_cannot_accept_queued_ready() {
    let control = Control::default();
    control.stop.store(true, Ordering::SeqCst);
    assert!(!control.accept_ready(Instant::now() + Duration::from_secs(1)));
}

#[test]
fn activation_winning_then_failure_publishes_and_cancels_before_cleanup() {
    let control = Control::default();
    let cancel = AtomicBool::new(false);
    assert!(control.accept_ready(Instant::now() + Duration::from_secs(1)));
    assert!(control.begin_failed_cleanup("event pump lost connection", &cancel));
    assert!(matches!(
        control.failure.get(),
        Some(HotkeyStatus::Failed(_))
    ));
    assert!(cancel.load(Ordering::SeqCst));
}

#[test]
fn expired_ready_is_not_accepted() {
    let control = Control::default();
    let now = Instant::now();
    assert!(!control.accept_ready_at(now, now, || panic!("expired path reached activation")));
}
