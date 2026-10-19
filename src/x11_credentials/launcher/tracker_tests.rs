//! Real tracker boundary, scripted completion evidence. No real process reap is
//! implied; native fixtures independently establish OS containment/termination.
use super::policy::{Observation, SignalResult};
use super::tracker::{Identity, Ownership, Tracker};
use crate::x11_credentials::{Cleanup, FailureKind, MAX_PENDING_CHILDREN};

#[derive(Debug)]
struct Handle(u64);
fn attach(t: &Tracker<Handle>, pid: i32, handle: Option<Handle>) -> Identity {
    let ticket = t
        .reserve()
        .expect("tracker must reserve bounded ownership before launch");
    t.attach(ticket, pid, handle)
        .expect("reserve->attach retains the same generation")
}
#[test]
fn starting_and_quarantined_helpers_share_the_four_slot_cap() {
    let t = Tracker::<Handle>::new();
    for i in 0..MAX_PENDING_CHILDREN {
        let id = attach(&t, 100 + i as i32, Some(Handle(i as u64 + 1)));
        let signal = if i % 2 == 0 {
            SignalResult::Requested
        } else {
            SignalResult::Failed
        };
        assert_eq!(
            t.quarantine(id, signal).unwrap(),
            if i % 2 == 0 {
                Cleanup::Pending
            } else {
                Cleanup::Unknown
            }
        );
    }
    assert_eq!(t.occupied(), 4);
    assert_eq!(t.reserve(), Err(FailureKind::CapacityExhausted));
}
#[test]
fn timeout_or_successful_signal_does_not_release_capacity() {
    let t = Tracker::new();
    let id = attach(&t, 123, Some(Handle(1)));
    assert_eq!(
        t.quarantine(id, SignalResult::Requested).unwrap(),
        Cleanup::Pending
    );
    assert_eq!(
        t.observe(id, Observation::NotReady).unwrap(),
        Cleanup::Pending
    );
    assert_eq!(t.occupied(), 1);
    assert_eq!(t.handle(id).unwrap().unwrap().0, 1);
}
#[test]
fn echild_or_wait_failure_is_unknown_not_successful_cleanup() {
    for observation in [Observation::Echild, Observation::Failed] {
        let t = Tracker::new();
        let id = attach(&t, 123, Some(Handle(1)));
        assert_eq!(t.observe(id, observation).unwrap(), Cleanup::Unknown);
        assert_eq!(t.occupied(), 1);
    }
}
#[test]
fn positively_reaped_child_releases_exactly_one_slot() {
    let t = Tracker::new();
    let ids: Vec<_> = (0..4)
        .map(|i| attach(&t, 123 + i, Some(Handle(i as u64))))
        .collect();
    assert_eq!(
        t.observe(ids[1], Observation::Reaped).unwrap(),
        Cleanup::Reaped
    );
    assert_eq!(t.occupied(), 3);
    assert!(t.reserve().is_ok());
    assert_eq!(t.reserve(), Err(FailureKind::CapacityExhausted));
}
#[test]
fn reused_pid_has_new_generation_and_old_identity_cannot_release_it() {
    let t = Tracker::new();
    let old = attach(&t, 123, Some(Handle(1)));
    t.observe(old, Observation::Reaped).unwrap();
    let new = attach(&t, 123, Some(Handle(2)));
    assert_ne!(old.ticket, new.ticket);
    assert!(matches!(t.handle(old), Err(FailureKind::StaleIdentity)));
    assert_eq!(
        t.observe(old, Observation::Reaped),
        Err(FailureKind::StaleIdentity)
    );
    assert_eq!(t.occupied(), 1);
    assert_eq!(t.handle(new).unwrap().unwrap().0, 2);
}
#[test]
fn successful_pid_without_pidfd_remains_owned_unknown() {
    let t = Tracker::<Handle>::new();
    let id = attach(&t, 123, None);
    assert_eq!(
        t.quarantine(id, SignalResult::Failed).unwrap(),
        Cleanup::Unknown
    );
    assert!(t.handle(id).unwrap().is_none());
    assert_eq!(t.occupied(), 1);
    assert_eq!(
        t.observe(id, Observation::Echild).unwrap(),
        Cleanup::Unknown
    );
}
#[test]
fn only_unstarted_reservation_can_be_released_without_reap() {
    let t = Tracker::<Handle>::new();
    let ticket = t.reserve().unwrap();
    t.unstarted(ticket).unwrap();
    assert_eq!(t.occupied(), 0);
    assert_eq!(t.unstarted(ticket), Err(FailureKind::StaleIdentity));
    let id = attach(&t, 123, Some(Handle(1)));
    assert_eq!(t.unstarted(id.ticket), Err(FailureKind::StaleIdentity));
    assert_eq!(t.occupied(), 1);
}

#[test]
fn reservations_alone_exhaust_capacity_before_any_child_is_created() {
    let t = Tracker::<Handle>::new();
    let tickets: Vec<_> = (0..4)
        .map(|_| t.reserve().expect("starting slots count toward cap"))
        .collect();
    assert_eq!(t.occupied(), 4);
    assert_eq!(t.reserve(), Err(FailureKind::CapacityExhausted));
    t.unstarted(tickets[0]).unwrap();
    assert!(t.reserve().is_ok());
    assert_eq!(t.reserve(), Err(FailureKind::CapacityExhausted));
}
#[test]
fn unknown_is_not_downgraded_by_a_noncompletion_observation() {
    let t = Tracker::new();
    let id = attach(&t, 123, Some(Handle(1)));
    assert_eq!(
        t.quarantine(id, SignalResult::Failed).unwrap(),
        Cleanup::Unknown
    );
    assert_eq!(
        t.observe(id, Observation::NotReady).unwrap(),
        Cleanup::Unknown
    );
    assert_eq!(t.occupied(), 1);
    assert_eq!(t.observe(id, Observation::Reaped).unwrap(), Cleanup::Reaped);
    assert_eq!(t.occupied(), 0);
}
