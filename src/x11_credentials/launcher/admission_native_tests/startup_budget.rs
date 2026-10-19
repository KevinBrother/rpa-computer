//! Additive R1 scenario. Original no_go.rs baseline and all assertions retained.
//! Shared observation qualification is repaired separately from product no-GO;
//! independent runtime validation is still required.
use super::super::native::lifecycle::{bit, ChildPoint, ParentPoint, Probe};
use super::harness::*;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub(super) const BUDGET: Duration = Duration::from_secs(2);
pub(super) const STARTUP_DELAY: Duration = Duration::from_millis(900);
pub(super) const GRACE: Duration = Duration::from_millis(500);

pub(super) fn run() {
    let file = Authority::new();
    let exits = ExitObserver::install();
    let point = ParentPoint::BeforePermissionWrite;
    let child_point = ChildPoint::BeforePrctl;
    let probe = Probe::new(bit(point as u32), bit(child_point as u32));
    let start = Instant::now();
    let deadline = start + BUDGET;
    let worker = Worker::start(&file, probe.clone(), deadline);
    child(&probe, child_point, start + Duration::from_millis(600));
    // Consume the original budget in the EXISTING raw child barrier, before
    // prctl/ARMED. No controller-sent GO, new child hook, or product timeout edit.
    let release_not_before = start + STARTUP_DELAY;
    while Instant::now() < release_not_before {
        std::thread::sleep(Duration::from_millis(1));
    }
    let released_at = Instant::now();
    assert!(
        released_at + Duration::from_millis(400) < deadline,
        "ADMISSION_FIXTURE_INFRA: startup delay left insufficient original budget"
    );
    assert_eq!(path_ops(&probe), (0, 0));
    probe.release_child(child_point);
    parent(&probe, point, deadline);
    let armed_at = Instant::now();
    assert!(
        armed_at < deadline,
        "ADMISSION_FIXTURE_INFRA: ARMED too late"
    );
    let cutoff = deadline + GRACE; // Never reset from release or ARMED.
                                   // Release timestamp is a lower bound on actual ARMED, unlike the later
                                   // controller observation of the parent's ARMED barrier.
    assert!(
        released_at + BUDGET > cutoff + Duration::from_millis(300),
        "ADMISSION_FIXTURE_INFRA: child release cannot distinguish reset budget"
    );
    assert!(
        armed_at + BUDGET > cutoff + Duration::from_millis(300),
        "ADMISSION_FIXTURE_INFRA: no separation from reset-full-budget counterexample"
    );
    let observer = Observer::from_probe(&probe);
    let pid = probe.raw().child_pid.load(Ordering::Acquire);
    let tid = probe.raw().creator_tid.load(Ordering::Acquire);
    assert!(pid > 0 && tid > 0 && tid != std::process::id() as i32);
    let exited = wait(cutoff, || observer.exited());
    let observed_at = Instant::now();
    if exited {
        assert!(
            wait(Instant::now() + Duration::from_millis(200), || exits
                .for_pid(pid)
                .is_some()),
            "ADMISSION_FIXTURE_INFRA: missing kernel termination cause"
        );
    }
    // Sample before releasing the actual creator or performing fixture cleanup.
    let event = exits.for_pid(pid);
    let path = path_ops(&probe);
    let held = still_held(&probe, point) && !worker.finished();
    let reaps = probe.raw().positive_reaps.load(Ordering::Acquire);
    println!("ADMISSION_OBSERVATION delayed-no-go actual_creator_tid={tid} child_pid={pid} budget_ms={} grace_ms={} released_ms={} armed_ms={} observed_ms={} cutoff_ms={} reset_earliest_ms={} exited={exited} event={event:?} parent_held={held} path_ops={path:?} product_reaps={reaps}",
        BUDGET.as_millis(), GRACE.as_millis(), released_at.duration_since(start).as_millis(),
        armed_at.duration_since(start).as_millis(), observed_at.duration_since(start).as_millis(),
        cutoff.duration_since(start).as_millis(), (released_at + BUDGET).duration_since(start).as_millis());
    probe.release_parent(point);
    let result = worker.finish();
    drop(observer);
    assert!(held, "ADMISSION_FIXTURE_INFRA: actual creator was not held");
    assert!(
        result.is_err(),
        "no-GO expiry cannot return a usable credential"
    );
    assert_eq!(
        path,
        (0, 0),
        "ADMISSION_ASSERTION_RED: delayed no-GO accessed authority"
    );
    assert!(exited && event.is_some_and(|e| e.pid == pid && e.raw_rejection()),
        "ADMISSION_ASSERTION_RED: startup consumed budget but no-GO child did not self-reject by original cutoff; exited={exited}, event={event:?}");
}
