use super::super::native::lifecycle::{bit, ChildPoint, ParentPoint, Probe};
use super::harness::*;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub(super) fn run(point: ParentPoint) {
    let file = Authority::new();
    assert_eq!(super::super::lifecycle_owner_view(0).0, 0);
    let probe = Probe::new(
        bit(point as u32),
        bit(ChildPoint::BeforePermissionRead as u32),
    );
    let worker = Worker::start(
        &file,
        probe.clone(),
        Instant::now() + Duration::from_secs(3),
    );
    parent(&probe, point);
    // Child has installed PDEATHSIG and completed FD containment, but has not
    // received startup permission. This isolates handoff loss from pre-prctl.
    child(&probe, ChildPoint::BeforePermissionRead);
    let observer = Observer::from_probe(&probe);
    let pid = probe.raw().child_pid.load(Ordering::Acquire);
    let tid = probe.raw().creator_tid.load(Ordering::Acquire);
    let descriptors = [
        probe.raw().parent_socket.load(Ordering::Acquire),
        probe.raw().child_socket.load(Ordering::Acquire),
        probe.raw().kernel_pidfd.load(Ordering::Acquire),
    ];
    assert!(pid > 0 && tid > 0 && tid != std::process::id() as i32);
    worker.creator_will_exit_without_unwind();
    probe
        .raw()
        .exit_parent
        .store(point as u32, Ordering::Release);
    assert!(
        wait(Instant::now() + Duration::from_secs(1), || probe
            .raw()
            .creator_exiting
            .load(Ordering::Acquire)
            == 1
            && !std::path::Path::new(&format!("/proc/self/task/{tid}"))
                .exists()),
        "LIFECYCLE_FIXTURE_INFRA: creator-only raw exit not observed"
    );
    assert!(
        wait(Instant::now() + Duration::from_secs(1), || observer
            .exited()),
        "LIFECYCLE_ASSERTION_RED: creator death did not produce observed child exit"
    );
    let recovered = wait(Instant::now() + Duration::from_millis(700), || {
        probe.raw().positive_reaps.load(Ordering::Acquire) == 1
            && super::super::lifecycle_owner_view(pid).0 == 0
            && descriptors.iter().all(|&fd| closed(fd))
    });
    // Evidence is captured before the observer's cleanup can reap anything.
    let view = super::super::lifecycle_owner_view(pid);
    let reaps = probe.raw().positive_reaps.load(Ordering::Acquire);
    let closed: Vec<_> = descriptors.iter().map(|&fd| closed(fd)).collect();
    let path_ops = probe.raw().path_entered.load(Ordering::Acquire);
    drop(observer);
    assert!(recovered,
        "LIFECYCLE_ASSERTION_RED: creator {point:?} lost recoverable helper ownership/resources: slots={}, tracked_child={}, actual_product_reaps={reaps}, original_fds_closed={closed:?}, path_ops={path_ops}", view.0, view.1);
    assert_eq!(
        path_ops, 0,
        "creator death before startup must not reach pathname work"
    );
}
