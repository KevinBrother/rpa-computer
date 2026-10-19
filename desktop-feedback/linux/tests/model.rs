//! RED-intent behavior contracts. Not executed; skeleton methods are deliberately missing.
mod common;
use common::snapshot;
use rpa_desktop_feedback::protocol::{Cleanup, Phase, Pointer, PointerKind};
use rpa_desktop_feedback_linux::model::{FeedbackModel, Severity};
use std::time::Duration;

#[test]
fn initial_ui_is_waiting_not_executing_and_stop_is_disabled() {
    let model = FeedbackModel::new();
    assert_eq!(model.presentation().text, "Waiting for Host");
    assert!(!model.can_stop());
    assert!(model.ring(Duration::ZERO).is_none());
}

#[test]
fn stale_and_duplicate_sequences_do_not_rollback_current_session() {
    let mut model = FeedbackModel::new();
    assert!(model.apply(snapshot(8), Duration::ZERO).unwrap());
    let mut old = snapshot(7);
    old.phase = Phase::Executing;
    old.session.as_mut().unwrap().generation = 2;
    assert!(!model.apply(old, Duration::ZERO).unwrap());
    assert!(!model.apply(snapshot(8), Duration::ZERO).unwrap());
    assert_eq!(model.request_stop().unwrap().generation, 1);
}

#[test]
fn maximum_sequence_is_exact_and_cannot_wrap() {
    let mut model = FeedbackModel::new();
    model.apply(snapshot(u64::MAX), Duration::ZERO).unwrap();
    assert!(!model.apply(snapshot(0), Duration::ZERO).unwrap());
    assert_eq!(model.sequence(), Some(u64::MAX));
}

#[test]
fn stop_is_once_per_session_generation_even_when_phase_changes() {
    let mut model = FeedbackModel::new();
    model.apply(snapshot(1), Duration::ZERO).unwrap();
    assert_eq!(model.request_stop(), snapshot(1).session);
    assert!(model.request_stop().is_none());
    let mut next = snapshot(2);
    next.phase = Phase::Executing;
    model.apply(next, Duration::ZERO).unwrap();
    assert!(!model.can_stop());
    assert_eq!(model.presentation().text, "Stop requested · awaiting Host");
    assert!(!model.presentation().safe_completion);
}

#[test]
fn sessionless_snapshot_does_not_rearm_the_same_stopped_generation() {
    let mut model = FeedbackModel::new();
    model.apply(snapshot(1), Duration::ZERO).unwrap();
    model.request_stop().unwrap();
    let mut empty = snapshot(2);
    empty.session = None;
    model.apply(empty, Duration::ZERO).unwrap();
    assert!(!model.can_stop());
    model.apply(snapshot(3), Duration::ZERO).unwrap();
    assert!(model.request_stop().is_none());
}

#[test]
fn a_new_generation_or_session_reenables_stop() {
    let mut model = FeedbackModel::new();
    model.apply(snapshot(1), Duration::ZERO).unwrap();
    model.request_stop().unwrap();
    let mut next = snapshot(2);
    next.session.as_mut().unwrap().generation = u64::MAX;
    model.apply(next, Duration::ZERO).unwrap();
    assert_eq!(model.request_stop().unwrap().generation, u64::MAX);
    let mut other = snapshot(3);
    other.session.as_mut().unwrap().id = "session-b".into();
    model.apply(other, Duration::ZERO).unwrap();
    assert_eq!(model.request_stop().unwrap().id, "session-b");
}

#[test]
fn closed_and_sessionless_snapshots_disable_stop() {
    for no_session in [false, true] {
        let mut model = FeedbackModel::new();
        let mut next = snapshot(1);
        if no_session {
            next.session = None;
        } else {
            next.phase = Phase::Closed;
        }
        model.apply(next, Duration::ZERO).unwrap();
        assert!(model.request_stop().is_none());
    }
}

#[test]
fn cleanup_warnings_override_every_phase_and_stop_request() {
    for (cleanup, text, severity) in [
        (
            Cleanup::Failed,
            "Cleanup failed · inputs may remain held",
            Severity::Error,
        ),
        (
            Cleanup::Unknown,
            "Cleanup unknown · not confirmed",
            Severity::Warning,
        ),
        (
            Cleanup::Pending,
            "Stopping · cleanup pending",
            Severity::Warning,
        ),
    ] {
        for phase in [
            Phase::Starting,
            Phase::Idle,
            Phase::Observing,
            Phase::Executing,
            Phase::Paused,
            Phase::Stopping,
            Phase::Faulted,
            Phase::Closed,
        ] {
            let mut model = FeedbackModel::new();
            model.apply(snapshot(1), Duration::ZERO).unwrap();
            model.request_stop().unwrap();
            let mut next = snapshot(2);
            next.phase = phase;
            next.cleanup = cleanup;
            model.apply(next, Duration::ZERO).unwrap();
            let view = model.presentation();
            assert_eq!((view.text, view.severity), (text, severity));
            assert!(!view.safe_completion);
        }
    }
}

#[test]
fn only_host_closed_with_released_or_not_needed_is_safe_completion() {
    for cleanup in [Cleanup::Released, Cleanup::NotNeeded] {
        let mut model = FeedbackModel::new();
        let mut next = snapshot(1);
        next.cleanup = cleanup;
        model.apply(next.clone(), Duration::ZERO).unwrap();
        assert!(!model.presentation().safe_completion);
        next.sequence = 2;
        next.phase = Phase::Closed;
        model.apply(next, Duration::ZERO).unwrap();
        assert!(model.presentation().safe_completion);
    }
}

#[test]
fn ring_uses_confirmed_native_coordinates_and_exact_expiry() {
    for (kind, lifetime) in [
        (PointerKind::Move, 350),
        (PointerKind::Drag, 350),
        (PointerKind::Click, 650),
    ] {
        let mut model = FeedbackModel::new();
        let mut next = snapshot(1);
        let pointer = Pointer {
            x: 143.5,
            y: 80.25,
            kind,
        };
        next.pointer = Some(pointer.clone());
        model.apply(next, Duration::from_millis(100)).unwrap();
        assert_eq!(
            model.ring(Duration::from_millis(100 + lifetime - 1)),
            Some(pointer)
        );
        assert!(model.ring(Duration::from_millis(100 + lifetime)).is_none());
    }
}

#[test]
fn duplicate_snapshot_and_timer_poll_do_not_extend_ring() {
    let mut model = FeedbackModel::new();
    let mut next = snapshot(1);
    next.pointer = Some(Pointer {
        x: 1.,
        y: 2.,
        kind: PointerKind::Move,
    });
    model.apply(next.clone(), Duration::ZERO).unwrap();
    assert!(model.ring(Duration::from_millis(200)).is_some());
    assert!(!model.apply(next, Duration::from_millis(300)).unwrap());
    assert!(model.ring(Duration::from_millis(350)).is_none());
}

#[test]
fn topology_invalidation_clears_ring_without_rearming_stop() {
    let mut model = FeedbackModel::new();
    let mut next = snapshot(1);
    next.pointer = Some(Pointer {
        x: 1.,
        y: 2.,
        kind: PointerKind::Move,
    });
    model.apply(next, Duration::ZERO).unwrap();
    model.request_stop().unwrap();
    model.invalidate_geometry();
    assert!(model.ring(Duration::from_millis(1)).is_none());
    assert!(!model.can_stop());
}

#[test]
fn sessionless_or_closed_states_never_draw_a_pointer() {
    for no_session in [false, true] {
        let mut model = FeedbackModel::new();
        let mut next = snapshot(1);
        next.pointer = Some(Pointer {
            x: 1.,
            y: 2.,
            kind: PointerKind::Move,
        });
        if no_session {
            next.session = None;
        } else {
            next.phase = Phase::Closed;
        }
        model.apply(next, Duration::ZERO).unwrap();
        assert!(model.ring(Duration::ZERO).is_none());
    }
}
