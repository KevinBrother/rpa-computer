//! Controls for the admission FIXTURE, not credential product behavior. The
//! Original eleven assertions retained after independent RED; N1 adds a qualified
//! absence positive control, with no claim of separate historical RED for it.
use super::{harness, startup_budget};
use std::panic::{catch_unwind, UnwindSafe};
use std::time::{Duration, Instant};

fn must_be_infra(action: impl FnOnce() -> bool + UnwindSafe, reason: &str) {
    match catch_unwind(action) {
        Ok(value) => panic!(
            "ADMISSION_CONTROL_ASSERTION_RED: {reason} was classified as boolean {value}, not INFRA"
        ),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("");
            assert!(
                message.contains("ADMISSION_FIXTURE_INFRA"),
                "ADMISSION_CONTROL_ASSERTION_RED: unrelated panic is not qualification evidence: {message}"
            );
        }
    }
}

#[test]
fn expired_observation_with_true_predicate_is_not_timely_exit() {
    let until = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    must_be_infra(
        || harness::wait(until, || true),
        "first successful observation after cutoff (no earlier evidence)",
    );
}

#[test]
fn poll_syscall_error_is_infra_not_child_still_running() {
    must_be_infra(
        || harness::poll_exited(-1, 0, Some(libc::EIO)),
        "poll syscall EIO",
    );
}

#[test]
fn poll_nval_is_infra_not_child_still_running() {
    must_be_infra(
        || harness::poll_exited(1, libc::POLLNVAL, None),
        "poll invalid observer descriptor",
    );
}

#[test]
fn poll_error_event_is_infra_not_child_still_running() {
    must_be_infra(
        || harness::poll_exited(1, libc::POLLERR, None),
        "poll observer error event",
    );
}

#[test]
fn poll_nval_with_readiness_cannot_qualify_exit() {
    must_be_infra(
        || harness::poll_exited(1, libc::POLLNVAL | libc::POLLIN, None),
        "invalid descriptor even with readiness bit",
    );
}

#[test]
fn proc_permission_error_cannot_qualify_creator_death() {
    must_be_infra(
        || {
            harness::creator_gone(true, || {
                Err(std::io::Error::from_raw_os_error(libc::EACCES))
            })
        },
        "creator task lookup EACCES after pre-exit marker",
    );
}

#[test]
fn proc_io_error_cannot_qualify_creator_death() {
    must_be_infra(
        || harness::creator_gone(true, || Err(std::io::Error::from_raw_os_error(libc::EIO))),
        "creator task lookup EIO after pre-exit marker",
    );
}

// Positive controls prevent a later reject-all implementation of these local
// decisions. They are not native death/reuse or deadline evidence.
#[test]
fn timely_true_predicate_remains_observable() {
    assert!(harness::wait(
        Instant::now() + Duration::from_secs(1),
        || true
    ));
}

#[test]
fn valid_poll_readiness_and_not_ready_are_distinct() {
    assert!(harness::poll_exited(1, libc::POLLIN, None));
    assert!(!harness::poll_exited(0, 0, None));
    // Interrupted observation is not proof of exit or a terminal poll error.
    assert!(!harness::poll_exited(-1, 0, Some(libc::EINTR)));
}

#[test]
fn visible_task_or_missing_preexit_marker_does_not_qualify_death() {
    assert!(!harness::creator_gone(true, || Ok(true)));
    assert!(!harness::creator_gone(false, || panic!(
        "must short-circuit before lookup"
    )));
}

#[test]
fn delayed_start_schedule_excludes_a_fresh_full_budget() {
    // Same constants consumed by the actual child-barrier case. This is only a
    // schedule separation control, NOT a model replacing the real launcher run.
    let start = Instant::now();
    let original_cutoff = start + startup_budget::BUDGET + startup_budget::GRACE;
    let earliest_reset_exit = start + startup_budget::STARTUP_DELAY + startup_budget::BUDGET;
    assert!(earliest_reset_exit > original_cutoff + Duration::from_millis(300));
    assert!(start + startup_budget::STARTUP_DELAY < start + startup_budget::BUDGET);
}

#[test]
fn qualified_absence_after_visible_target_allows_death_confirmation() {
    // Real filesystem lookups through the same TaskVisibility used by raw_exit.
    // Synthetic owned directories qualify the classification, NOT creator death
    // or TID reuse. No privileged /proc writes or product launcher involved.
    struct OwnedDirectory(std::path::PathBuf);
    impl Drop for OwnedDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let parent = std::env::temp_dir().join(format!(
        "credential-task-visibility-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&parent).expect("ADMISSION_FIXTURE_INFRA: owned parent directory");
    let parent = OwnedDirectory(parent);
    let target = parent.0.join("42");
    std::fs::create_dir(&target).expect("ADMISSION_FIXTURE_INFRA: owned visible target");
    let qualified = super::task_visibility::TaskVisibility::before_exit(&parent.0, 42)
        .expect("ADMISSION_FIXTURE_INFRA: target previsibility qualification");
    assert!(!harness::creator_gone(true, || qualified.exists()));
    std::fs::remove_dir(&target).expect("ADMISSION_FIXTURE_INFRA: remove owned target");
    assert!(harness::creator_gone(true, || qualified.exists()));
}

#[test]
fn in_window_false_returned_after_cutoff_cannot_establish_timeout() {
    // Exercise the ACTUAL shared wait, not a copied clock/poll/kernel model.
    // One thread explicitly orders the stale read, timely state transition and
    // delayed predicate return; no scheduler race is needed to obtain the case.
    let start = Instant::now();
    let cutoff = start + Duration::from_millis(200);
    let mut ready = false;
    let mut calls = 0;
    let mut saved_false_at = None;
    let mut became_ready_at = None;
    let mut stale_return_at = None;
    let outcome = catch_unwind(std::panic::AssertUnwindSafe(|| {
        harness::wait(cutoff, || {
            calls += 1;
            if calls != 1 {
                return ready; // A deliberate fresh sample sees the NEW state.
            }
            let saved = ready;
            saved_false_at = Some(Instant::now());
            ready = true;
            became_ready_at = Some(Instant::now());
            // A single short sleep deliberately spans the cutoff. The kernel
            // may overschedule it; that cannot create timely-success evidence.
            std::thread::sleep(
                (cutoff + Duration::from_millis(10)).saturating_duration_since(Instant::now()),
            );
            stale_return_at = Some(Instant::now());
            saved
        })
    }));

    // OUTSIDE catch_unwind: failure to establish the scenario cannot satisfy
    // the expected observation-INFRA result. No first call / slow setup is
    // control SETUP INFRA, never the intended assertion RED or a passing test.
    assert!(
        saved_false_at.is_some_and(|at| at < cutoff)
            && became_ready_at.is_some_and(|at| at < cutoff)
            && stale_return_at.is_some_and(|at| at >= cutoff),
        "ADMISSION_CONTROL_SETUP_INFRA: stale-false window not established"
    );
    println!(
        "ADMISSION_STALE_FALSE_CONTROL calls={calls} saved_us={} ready_us={} returned_us={} cutoff_us={}",
        saved_false_at.unwrap().duration_since(start).as_micros(),
        became_ready_at.unwrap().duration_since(start).as_micros(),
        stale_return_at.unwrap().duration_since(start).as_micros(),
        cutoff.duration_since(start).as_micros()
    );
    match outcome {
        Ok(value) => panic!(
            "ADMISSION_CONTROL_ASSERTION_RED: timely state transition hidden by stale false; wait returned ordinary {value} after cutoff"
        ),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("");
            assert!(
                message.contains("ADMISSION_FIXTURE_INFRA"),
                "ADMISSION_CONTROL_ASSERTION_RED: unrelated panic is not stale-sample qualification: {message}"
            );
        }
    }
}
