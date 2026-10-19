use super::super::native::lifecycle::{bit, ParentPoint, Probe};
use super::harness::*;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub(super) fn run() {
    let file = Authority::new();
    let exits = ExitObserver::install();
    let point = ParentPoint::BeforePermissionWrite;
    let probe = Probe::new(bit(point as u32), 0);
    let deadline = Instant::now() + Duration::from_millis(800);
    let worker = Worker::start(&file, probe.clone(), deadline);
    // This point is reached by the actual clone thread only after transport::byte
    // received the child stage1 marker, after prctl and FD closure in the leaf.
    parent(&probe, point, deadline);
    assert!(
        Instant::now() < deadline,
        "ADMISSION_FIXTURE_INFRA: barrier not established before expiry"
    );
    let observer = Observer::from_probe(&probe);
    let pid = probe.raw().child_pid.load(Ordering::Acquire);
    let tid = probe.raw().creator_tid.load(Ordering::Acquire);
    assert!(pid > 0 && tid > 0 && tid != std::process::id() as i32);
    let exited = wait(deadline + Duration::from_millis(500), || observer.exited());
    if exited {
        assert!(
            wait(Instant::now() + Duration::from_millis(200), || exits
                .for_pid(pid)
                .is_some()),
            "ADMISSION_FIXTURE_INFRA: kernel termination cause not captured; cannot attribute exit"
        );
    }
    // Capture BEFORE any parent release, cancellation or fixture observer kill.
    let event = exits.for_pid(pid);
    let path = path_ops(&probe);
    let held = still_held(&probe, point) && !worker.finished();
    let reaps = probe.raw().positive_reaps.load(Ordering::Acquire);
    println!("ADMISSION_OBSERVATION no-go actual_creator_tid={tid} child_pid={pid} exited={exited} event={event:?} parent_held={held} path_ops={path:?} product_reaps={reaps}");
    probe.release_parent(point);
    let result = worker.finish();
    drop(observer); // Cleanup cannot alter the captured evidence above.
    assert!(
        held,
        "ADMISSION_FIXTURE_INFRA: parent no longer held before GO/cancellation"
    );
    assert!(
        result.is_err(),
        "no-GO expiry cannot return a usable credential"
    );
    assert_eq!(
        path,
        (0, 0),
        "ADMISSION_ASSERTION_RED: no-GO helper accessed authority"
    );
    assert!(exited && event.is_some_and(|e| e.pid == pid && e.raw_rejection()),
        "ADMISSION_ASSERTION_RED: no-GO child did not self-reject by original deadline independently of parent cancellation; exited={exited}, event={event:?}");
}
