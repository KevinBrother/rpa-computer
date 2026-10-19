use super::super::native::lifecycle::{bit, ChildPoint, ParentPoint, Probe};
use super::harness::*;
use crate::x11_credentials::FailureKind;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub(super) fn run(delay_write: bool) {
    let file = Authority::new();
    let hold = bit(ParentPoint::AfterPermissionWrite as u32)
        | if delay_write {
            bit(ParentPoint::BeforePermissionWrite as u32)
        } else {
            0
        };
    let probe = Probe::new(hold, bit(ChildPoint::BeforePermissionRead as u32));
    let deadline = Instant::now() + Duration::from_millis(800);
    let worker = Worker::start(&file, probe.clone(), deadline);
    child(&probe, ChildPoint::BeforePermissionRead);
    let point = if delay_write {
        ParentPoint::BeforePermissionWrite
    } else {
        ParentPoint::AfterPermissionWrite
    };
    parent(&probe, point);
    let observer = Observer::from_probe(&probe);
    assert!(
        Instant::now() < deadline,
        "LIFECYCLE_FIXTURE_INFRA: failed to establish pre-expiry barrier"
    );
    // Both threads are parked on explicit barriers. The wait only advances the
    // real absolute deadline; it is not used to guess an ordering/race.
    assert!(wait(
        deadline + Duration::from_millis(100),
        || Instant::now() >= deadline
    ));
    if delay_write {
        probe.release_parent(ParentPoint::BeforePermissionWrite);
        let progressed = wait(Instant::now() + Duration::from_secs(1), || {
            probe.raw().parent_reached.load(Ordering::Acquire)
                == ParentPoint::AfterPermissionWrite as u32
                || worker.finished()
        });
        assert!(
            progressed,
            "LIFECYCLE_FIXTURE_INFRA: neither write nor bounded pre-write rejection"
        );
        if worker.finished() {
            // A future implementation may reject at a repeated parent freshness
            // check before writing. This is valid, not a missing-fixture failure.
            let entered = probe.raw().path_entered.load(Ordering::Acquire);
            let result = worker.finish();
            drop(observer);
            assert_eq!(result.unwrap_err().kind, FailureKind::Timeout);
            assert_eq!(
                entered, 0,
                "expired rejected startup must not reach path work"
            );
            return;
        }
    }
    // Parent cannot perform its post-write check/cancellation until we release
    // it. Child gets the queued permission FIRST, after the original expiry.
    probe.release_child(ChildPoint::BeforePermissionRead);
    let progressed = wait(Instant::now() + Duration::from_millis(400), || {
        probe.raw().path_returned.load(Ordering::Acquire) != 0 || observer.exited()
    });
    let entered = probe.raw().path_entered.load(Ordering::Acquire);
    let returned = probe.raw().path_returned.load(Ordering::Acquire);
    probe.release_parent(ParentPoint::AfterPermissionWrite);
    let result = worker.finish();
    drop(observer);
    assert!(progressed, "LIFECYCLE_FIXTURE_INFRA: child neither exited nor returned from path syscall before parent cancellation");
    assert_eq!(
        result.unwrap_err().kind,
        FailureKind::Timeout,
        "parent late-result rejection remains required"
    );
    assert_eq!((entered, returned), (0, 0),
        "LIFECYCLE_ASSERTION_RED: expired startup reached actual path operation while parent was parked (delay_write={delay_write})");
}
