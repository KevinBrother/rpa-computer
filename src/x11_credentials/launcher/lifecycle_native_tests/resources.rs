use super::super::native::lifecycle::{bit, ParentPoint, Probe};
use super::harness::*;
use crate::x11_credentials::{FailureKind, MAX_PENDING_CHILDREN};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub(super) fn run() {
    let file = Authority::new();
    assert_eq!(super::super::lifecycle_owner_view(0).0, 0);
    let mut held = Vec::new();
    for _ in 0..MAX_PENDING_CHILDREN {
        let probe = Probe::new(bit(ParentPoint::ReplyReceived as u32), 0);
        let worker = Worker::start(
            &file,
            probe.clone(),
            Instant::now() + Duration::from_secs(3),
        );
        parent(&probe, ParentPoint::ReplyReceived);
        // This is positive PRODUCT waitid evidence, not pidfd readability or a
        // synthetic Owner observation. The foreground still owns Native/reply.
        assert!(
            wait(Instant::now() + Duration::from_secs(1), || probe
                .raw()
                .positive_reaps
                .load(Ordering::Acquire)
                == 1),
            "LIFECYCLE_FIXTURE_INFRA: no actual product reap at held caller"
        );
        held.push((probe, worker));
    }
    let still_open = held
        .iter()
        .filter(|(p, _)| !closed(p.raw().parent_socket.load(Ordering::Acquire)))
        .count();
    // Descriptor count is diagnostic, not an implementation constraint: a
    // future fix may release IPC early, but these four callers still hold their
    // pre-parse replies at the real policy boundary.
    let product_reaps: Vec<_> = held
        .iter()
        .map(|(p, _)| p.raw().positive_reaps.load(Ordering::Acquire))
        .collect();
    let slots_while_held = super::super::lifecycle_owner_view(0).0;
    let fifth =
        crate::x11_credentials::acquire(file.request(), Instant::now() + Duration::from_secs(1));
    let rejected = matches!(fifth, Err(ref e) if e.kind == FailureKind::CapacityExhausted);
    let fifth_outcome = fifth.as_ref().map(|v| v.cleanup).map_err(|e| e.kind);
    for (probe, worker) in held {
        probe.release_parent(ParentPoint::ReplyReceived);
        let _ = worker.finish(); // Cleanup before the assertion, no pass inferred.
    }
    assert!(rejected,
        "LIFECYCLE_ASSERTION_RED: fifth acquisition not rejected at admission with four parked pre-parse callers after positive reaps; outcome={fifth_outcome:?}, actual_product_reaps={product_reaps:?}, tracker_slots={slots_while_held}, open_parent_sockets={still_open}");
}
