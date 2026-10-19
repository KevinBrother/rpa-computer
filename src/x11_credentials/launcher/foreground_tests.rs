//! Supplemental real-tracker assertions. Child observations are scripted here;
//! only the independent native fixtures can establish actual reaping/FD cleanup.
use super::policy::{Observation, SignalResult};
use super::tracker::{Identity, Ownership, Tracker};
use crate::x11_credentials::{Cleanup, FailureKind, MAX_PENDING_CHILDREN};

fn operation(tracker: &Tracker<()>, pid: i32) -> Identity {
    let ticket = tracker.reserve_foreground().unwrap();
    tracker.attach(ticket, pid, Some(())).unwrap()
}

#[test]
fn all_reaped_foregrounds_hold_capacity_then_normal_retirement_reuses_it() {
    let tracker = Tracker::new();
    let mut ids = Vec::new();
    for pid in 1..=MAX_PENDING_CHILDREN as i32 {
        let id = operation(&tracker, pid);
        assert_eq!(
            tracker.observe(id, Observation::Reaped),
            Ok(Cleanup::Reaped)
        );
        ids.push(id);
    }
    assert_eq!(tracker.occupied(), MAX_PENDING_CHILDREN);
    assert_eq!(
        tracker.reserve_foreground(),
        Err(FailureKind::CapacityExhausted)
    );
    assert!(tracker.children().iter().all(Option::is_none));
    // Cached successful reap cannot be downgraded while foreground still holds.
    assert_eq!(
        tracker.observe(ids[0], Observation::Echild),
        Ok(Cleanup::Reaped)
    );
    assert_eq!(
        tracker.quarantine(ids[0], SignalResult::Failed),
        Ok(Cleanup::Reaped)
    );
    for id in &ids {
        tracker.retire_foreground(id.ticket).unwrap();
    }
    assert_eq!(tracker.occupied(), 0);
    for old in ids {
        let new = operation(&tracker, old.pid);
        assert_ne!(new.ticket.generation, old.ticket.generation);
        tracker.observe(new, Observation::Reaped).unwrap();
        tracker.retire_foreground(new.ticket).unwrap();
    }
    assert_eq!(tracker.occupied(), 0);
}

#[test]
fn confirmed_no_child_does_not_free_unretired_foreground() {
    let tracker = Tracker::<()>::new();
    let ticket = tracker.reserve_foreground().unwrap();
    tracker.unstarted(ticket).unwrap();
    assert_eq!(tracker.occupied(), 1);
    assert_eq!(
        tracker.attach(ticket, 10, Some(())),
        Err(FailureKind::StaleIdentity)
    );
    tracker.retire_foreground(ticket).unwrap();
    assert_eq!(tracker.occupied(), 0);
}

#[test]
fn foreground_retirement_cannot_erase_pending_or_unknown_child() {
    for evidence in [Observation::NotReady, Observation::Echild] {
        let tracker = Tracker::new();
        let id = operation(&tracker, 10);
        tracker.observe(id, evidence).unwrap();
        tracker.retire_foreground(id.ticket).unwrap();
        assert_eq!(tracker.occupied(), 1);
        assert!(tracker.handle(id).unwrap().is_some());
        tracker.observe(id, Observation::Reaped).unwrap();
        assert_eq!(tracker.occupied(), 0);
    }
}

#[test]
fn reaped_foregrounds_and_pending_children_share_one_cap() {
    let tracker = Tracker::new();
    for pid in 1..=MAX_PENDING_CHILDREN as i32 {
        let id = operation(&tracker, pid);
        if pid % 2 == 0 {
            tracker.observe(id, Observation::Reaped).unwrap();
        } else {
            tracker.retire_foreground(id.ticket).unwrap();
        }
    }
    assert_eq!(tracker.occupied(), MAX_PENDING_CHILDREN);
    assert_eq!(
        tracker.reserve_foreground(),
        Err(FailureKind::CapacityExhausted)
    );
}

#[test]
fn stale_foreground_ticket_cannot_retire_a_replacement() {
    let tracker = Tracker::new();
    let old = operation(&tracker, 10);
    tracker.observe(old, Observation::Reaped).unwrap();
    tracker.retire_foreground(old.ticket).unwrap();
    let new = operation(&tracker, 10);
    tracker.observe(new, Observation::Reaped).unwrap();
    assert_eq!(
        tracker.retire_foreground(old.ticket),
        Err(FailureKind::StaleIdentity)
    );
    assert_eq!(tracker.occupied(), 1);
    tracker.retire_foreground(new.ticket).unwrap();
    assert_eq!(tracker.occupied(), 0);
}

#[test]
fn unpublished_launch_is_not_freed_by_foreground_retirement_alone() {
    let tracker = Tracker::<()>::new();
    let ticket = tracker.reserve_foreground().unwrap();
    tracker.retire_foreground(ticket).unwrap();
    // No explicit NoChild, no attached/reaped identity: keep the bounded slot.
    assert_eq!(tracker.occupied(), 1);
}
