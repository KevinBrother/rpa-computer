//! C2 positive control: actual leaf waits without GO, then the original creator
//! authorizes within the original budget. No child barrier or replacement clock.
use super::super::native::lifecycle::{bit, ChildPoint, ParentPoint, Probe};
use super::harness::*;
use crate::x11_credentials::Cleanup;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_secs(2);
const HOLD: Duration = Duration::from_millis(80);
const SAMPLE_INTERVAL: Duration = Duration::from_millis(2);
const MAX_SAMPLE_GAP: Duration = Duration::from_millis(20);
const COMPLETION_RESERVE: Duration = Duration::from_millis(500);

pub(super) fn run() {
    let file = Authority::new();
    let exits = ExitObserver::install();
    let before_go = ParentPoint::BeforePermissionWrite;
    let after_go = ParentPoint::AfterPermissionWrite;
    let probe = Probe::new(bit(before_go as u32) | bit(after_go as u32), 0);
    let started = Instant::now();
    let deadline = started + BUDGET;
    let qualification_cutoff = deadline - COMPLETION_RESERVE;
    let worker = Worker::start(&file, probe.clone(), deadline);
    // The real creator has consumed the ARMED stage1 marker. No GO can be
    // queued yet. Also observe the leaf entering its permission routine with
    // child_hold=0, so the fixture never parks the child on its futex seam.
    parent(&probe, before_go, qualification_cutoff);
    child(
        &probe,
        ChildPoint::BeforePermissionRead,
        qualification_cutoff,
    );
    let observer = Observer::from_probe(&probe);
    let pid = probe.raw().child_pid.load(Ordering::Acquire);
    let creator = probe.raw().creator_tid.load(Ordering::Acquire);
    assert!(
        pid > 0 && creator > 0 && creator != std::process::id() as i32,
        "ADMISSION_FIXTURE_INFRA: actual child/creator identity unavailable"
    );

    let hold_started = Instant::now();
    let hold_until = hold_started + HOLD;
    assert!(
        hold_until + MAX_SAMPLE_GAP < qualification_cutoff,
        "ADMISSION_FIXTURE_INFRA: insufficient original budget to qualify wait and timely GO"
    );
    let mut previous_sample_end = hold_started;
    let mut samples = 0;
    loop {
        let sample_start = Instant::now();
        let alive = !observer.exited(); // Fresh kernel pidfd poll each sample.
        let held = still_held(&probe, before_go) && !worker.finished();
        let path = path_ops(&probe);
        let stable_identity = probe.raw().child_pid.load(Ordering::Acquire) == pid
            && probe.raw().creator_tid.load(Ordering::Acquire) == creator;
        let no_child_hold = probe.raw().child_hold.load(Ordering::Acquire) == 0;
        let permission_entry = probe.raw().child_reached.load(Ordering::Acquire)
            == ChildPoint::BeforePermissionRead as u32;
        let no_reap = probe.raw().positive_reaps.load(Ordering::Acquire) == 0;
        let sample_end = Instant::now();
        // Reject scheduling/observation gaps instead of treating one stale
        // pre-hold sample and a late sample as an observed waiting interval.
        assert!(
            sample_end.duration_since(previous_sample_end) <= MAX_SAMPLE_GAP
                && sample_end <= hold_until + MAX_SAMPLE_GAP
                && sample_end < qualification_cutoff,
            "ADMISSION_FIXTURE_INFRA: waiting samples have an unqualified scheduling gap/cutoff"
        );
        assert!(
            alive && held && path == (0, 0) && stable_identity
                && no_child_hold && permission_entry && no_reap,
            "ADMISSION_FIXTURE_INFRA: wait not qualified; child_pid={pid} creator_tid={creator} alive={alive} parent_held={held} path_ops={path:?} stable_identity={stable_identity} child_unheld={no_child_hold} permission_entry={permission_entry} no_reap={no_reap}"
        );
        println!(
            "ADMISSION_PERMISSION_WAIT_SAMPLE index={samples} child_pid={pid} creator_tid={creator} start_elapsed_us={} end_elapsed_us={} alive={alive} parent_held={held} path_ops={path:?}",
            sample_start.duration_since(hold_started).as_micros(),
            sample_end.duration_since(hold_started).as_micros(),
        );
        samples += 1;
        previous_sample_end = sample_end;
        if sample_start >= hold_until {
            break; // Final fresh sample begins after the full requested hold.
        }
        std::thread::sleep(SAMPLE_INTERVAL);
    }
    assert!(
        samples >= 3 && previous_sample_end.duration_since(hold_started) >= HOLD,
        "ADMISSION_FIXTURE_INFRA: no qualified waiting interval"
    );
    let release_at = Instant::now();
    assert!(
        release_at < qualification_cutoff && still_held(&probe, before_go),
        "ADMISSION_FIXTURE_INFRA: original creator not held with timely GO budget"
    );
    probe.release_parent(before_go);
    // This existing seam is called only after the actual socket write returned
    // Ok(1). The controller never writes permission or changes the deadline.
    parent(&probe, after_go, qualification_cutoff);
    let sent_at = Instant::now();
    assert!(
        sent_at < qualification_cutoff
            && still_held(&probe, after_go)
            && !worker.finished()
            && probe.raw().creator_tid.load(Ordering::Acquire) == creator
            && probe.raw().child_pid.load(Ordering::Acquire) == pid,
        "ADMISSION_FIXTURE_INFRA: original creator's timely GO write not established"
    );
    println!(
        "ADMISSION_PERMISSION_WAIT_GO child_pid={pid} creator_tid={creator} samples={samples} held_us={} release_remaining_us={} sent_remaining_us={}",
        previous_sample_end.duration_since(hold_started).as_micros(),
        deadline.duration_since(release_at).as_micros(),
        deadline.duration_since(sent_at).as_micros(),
    );
    probe.release_parent(after_go);
    let result = worker.finish();
    assert!(
        Instant::now() < deadline,
        "ADMISSION_FIXTURE_INFRA: caller result observed after original deadline"
    );
    let acquired = result.expect(
        "ADMISSION_ASSERTION_RED: qualified permission wait followed by timely original GO must succeed",
    );
    assert_eq!(acquired.cleanup, Cleanup::Reaped);
    // This is the exact synthetic fixture value in harness::Authority. Boolean
    // comparison deliberately avoids formatting either credential on failure.
    assert!(
        acquired
            .cookie
            .as_ref()
            .is_some_and(|cookie| cookie.as_bytes() == &[0x79; 16]),
        "ADMISSION_ASSERTION_RED: expected synthetic credential missing or incorrect"
    );
    assert_eq!(
        path_ops(&probe),
        (1, 1),
        "synthetic authority path must actually be read"
    );
    assert_eq!(probe.raw().child_pid.load(Ordering::Acquire), pid);
    assert_eq!(probe.raw().positive_reaps.load(Ordering::Acquire), 1,
        "ADMISSION_ASSERTION_RED: require this probe's positive product waitid, not arbitrary reap activity");
    assert!(
        observer.exited(),
        "ADMISSION_ASSERTION_RED: actual pidfd must report child exit"
    );
    assert!(
        wait(deadline, || exits.for_pid(pid).is_some()),
        "ADMISSION_FIXTURE_INFRA: kernel exit event for actual child pid unavailable"
    );
    let event = exits.for_pid(pid).unwrap();
    assert!(
        event.pid == pid && event.code == libc::CLD_EXITED && event.status == 0,
        "ADMISSION_ASSERTION_RED: actual child did not exit successfully: {event:?}"
    );
    // Non-consuming check of the REAL pid, before Observer::drop can signal or
    // reap anything. Counter + successful exact-pid event + ECHILD tie completion
    // to this helper; WNOWAIT ensures the fixture cannot do the positive reap.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    let errno = if rc == -1 {
        std::io::Error::last_os_error().raw_os_error()
    } else {
        None
    };
    assert!(
        rc == 0 || (rc == -1 && errno == Some(libc::ECHILD)),
        "ADMISSION_FIXTURE_INFRA: exact-pid non-consuming waitid unavailable; rc={rc} errno={errno:?}"
    );
    assert!(
        rc == -1 && errno == Some(libc::ECHILD),
        "ADMISSION_ASSERTION_RED: actual child pid not already reaped by product; pid={pid} rc={rc} errno={errno:?}"
    );
    println!(
        "ADMISSION_OBSERVATION permission-wait actual_creator_tid={creator} child_pid={pid} samples={samples} path_ops={:?} product_reaps=1 exact_pid_echild=true event={event:?} credential_matches=true",
        path_ops(&probe),
    );
    drop(acquired);
    drop(observer); // Every success/exit/reap assertion above precedes cleanup.
}
