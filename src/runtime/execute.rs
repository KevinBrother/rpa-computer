//! Generic step executor: drives a [`Plan`] against the backend with
//! cancellation checks between every event, an absolute input-phase deadline,
//! and honest partial reporting — including when the deadline shortens a
//! requested hold.
//!
//! Release semantics: a press that was successfully injected is always
//! answered with its planned release on the partial/cancelled path (subject
//! to the same deadline), and `release_all` is additionally invoked whenever
//! the outcome is not `dispatched` — never before, so a completed
//! drag/click/chord is not reported as needing global cleanup.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::backend::Backend;
use crate::runtime::error::{codes, ToolError};
use crate::runtime::plan::{Plan, PlanEvent};
use crate::runtime::session::{
    sleep_cancellable, CleanupOutcome, InputOutcome, OBSERVE_PHASE_BUDGET, SETTLE_DELAY,
};

/// Extra wall-clock allowance for the best-effort release pass once the
/// input-phase deadline has expired. Releasing a key/button that was
/// physically pressed is mandatory cleanup, not optional input, so it gets a
/// small dedicated budget instead of being skipped outright when the input
/// budget ran out (e.g. a shortened hold that consumed the whole budget).
const RELEASE_PASS_ALLOWANCE: Duration = Duration::from_millis(250);

/// Outcome of running a plan's input phase plus cleanup.
pub struct ExecutionOutcome {
    pub input_outcome: InputOutcome,
    /// 1-based plan indexes of events injected successfully, in order.
    pub events_completed: Vec<usize>,
    /// Total input events in the plan (sleeps excluded).
    pub events_total: usize,
    pub cleanup_outcome: CleanupOutcome,
    pub cancelled: bool,
    /// Whether the post-input settle delay elapsed fully. When false, the
    /// post-step observation must not claim the screen was given time to
    /// settle.
    pub settled: bool,
    pub error: Option<ToolError>,
    pub pressed: Vec<usize>,
    /// Test-only cancellation seam: when set, the cancel flag is asserted
    /// after this many events have been injected successfully, making
    /// mid-dispatch cancellation deterministic without thread sleeps or
    /// wall-clock races. Compiled out of production builds.
    #[cfg(test)]
    pub cancel_after_injects: Option<usize>,
}

/// Configuration for one execution. Production code uses
/// `ExecutionConfig::production()`; tests can shrink the input budget to
/// exercise deadline truncation without multi-second sleeps.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionConfig {
    /// Absolute budget for the whole input phase (dispatch + releases).
    pub input_budget: Duration,
    /// Settle delay between input and the post-step observation.
    pub settle_delay: Duration,
    /// Test-only cancellation seam; see [`ExecutionOutcome::cancel_after_injects`].
    #[cfg(test)]
    pub cancel_after_injects: Option<usize>,
}

impl ExecutionConfig {
    pub fn production() -> Self {
        Self {
            input_budget: crate::runtime::session::INPUT_PHASE_BUDGET,
            settle_delay: SETTLE_DELAY,
            #[cfg(test)]
            cancel_after_injects: None,
        }
    }
}

/// Execute `plan` against `backend`. `map` converts image-space positions to
/// native coordinates. Never panics; every failure path yields a truthful
/// outcome.
pub fn execute_plan(
    plan: &Plan,
    backend: &mut dyn Backend,
    map: impl Fn([i32; 2]) -> (i32, i32),
    cancel: &Arc<AtomicBool>,
    config: &ExecutionConfig,
) -> ExecutionOutcome {
    let events_total = plan
        .events
        .iter()
        .filter(|e| !matches!(e, PlanEvent::Sleep(_)))
        .count();
    let mut out = ExecutionOutcome {
        input_outcome: InputOutcome::NotStarted,
        events_completed: Vec::new(),
        events_total,
        cleanup_outcome: CleanupOutcome::NotNeeded,
        cancelled: false,
        settled: true,
        error: None,
        pressed: Vec::new(),
        #[cfg(test)]
        cancel_after_injects: config.cancel_after_injects,
    };
    if plan.events.is_empty() {
        return out;
    }

    let deadline = Instant::now() + config.input_budget;
    let mut dispatched_all = true;
    let mut first_failure = false;

    let mut index = 0;
    while index < plan.events.len() {
        if cancel.load(Ordering::SeqCst) {
            out.cancelled = true;
            dispatched_all = false;
            break;
        }
        let in_hold = plan
            .hold_press
            .map(|p| index >= p && index < plan.hold_end)
            .unwrap_or(false);
        match &plan.events[index] {
            PlanEvent::Sleep(ms) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let want = Duration::from_millis(*ms);
                if !in_hold && want > remaining {
                    out.error = Some(ToolError::new(
                        codes::DEADLINE_EXCEEDED,
                        "input phase exceeded its time budget before a required wait",
                    ));
                    dispatched_all = false;
                    break;
                }
                // Inside a hold window a shortened sleep means the hold was
                // shortened: sleep what is left, then report partial honestly.
                let slice = want.min(remaining);
                if sleep_cancellable(slice, cancel).is_err() {
                    out.cancelled = true;
                    dispatched_all = false;
                    break;
                }
                if want > slice {
                    out.error = Some(ToolError::new(
                        codes::DEADLINE_EXCEEDED,
                        "requested hold was cut short by the input-phase time budget; \
                         the hold is reported as partial, not dispatched",
                    ));
                    dispatched_all = false;
                    // Fall through to the release pass for the hold window.
                    do_release_pass(
                        plan,
                        backend,
                        &map,
                        index + 1,
                        plan.hold_end,
                        &deadline,
                        &mut out,
                    );
                    break;
                }
            }
            _ => {
                if Instant::now() > deadline {
                    if out.error.is_none() {
                        out.error = Some(ToolError::new(
                            codes::DEADLINE_EXCEEDED,
                            "input phase exceeded its time budget",
                        ));
                    }
                    dispatched_all = false;
                    break;
                }
                let be = plan
                    .backend_event(index, &map)
                    .expect("non-sleep events always convert");
                match backend.inject(&be) {
                    Ok(()) => {
                        out.events_completed.push(index);
                        if plan.is_press_event(index) {
                            out.pressed.push(index);
                        }
                        // Test seam: deterministic mid-dispatch cancellation.
                        // Compiled out of production; the flag takes effect at
                        // the next event boundary, exactly like a real
                        // transport-set cancel.
                        #[cfg(test)]
                        if let Some(threshold) = out.cancel_after_injects {
                            if out.events_completed.len() >= threshold {
                                cancel.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                    Err(e) => {
                        if out.error.is_none() {
                            out.error = Some(ToolError::from(e));
                        }
                        first_failure = true;
                        dispatched_all = false;
                        // Honor only planned releases that answer a press we
                        // actually injected; releasing a never-pressed key or
                        // button can unlock UI state the user owns.
                        do_release_pass(
                            plan,
                            backend,
                            &map,
                            index + 1,
                            plan.events.len(),
                            &deadline,
                            &mut out,
                        );
                        break;
                    }
                }
            }
        }
        index += 1;
    }

    // --- precise outcome ----------------------------------------------------
    // If the first failure happened at the very first input event, that call
    // may still have had partial side effects: report partial, not
    // not_started. Cancellation before any injection is honestly not_started.
    out.input_outcome = if dispatched_all {
        InputOutcome::Dispatched
    } else if out.events_completed.is_empty() && !first_failure && out.cancelled {
        InputOutcome::NotStarted
    } else {
        InputOutcome::Partial
    };

    // --- cleanup --------------------------------------------------------------
    let completed_all = out.events_completed.len() == out.events_total && dispatched_all;
    if !completed_all && (!out.events_completed.is_empty() || first_failure) {
        // Something may still be physically held (or the failed call may have
        // pressed something): global best-effort release, honestly reported.
        match backend.release_all() {
            Ok(()) => out.cleanup_outcome = CleanupOutcome::Released,
            Err(e) => {
                out.cleanup_outcome = CleanupOutcome::Failed;
                if out.error.is_none() {
                    out.error = Some(ToolError::from(e));
                }
            }
        }
    } else if out.events_completed.is_empty() {
        out.cleanup_outcome = CleanupOutcome::NotNeeded;
    } else {
        // Fully dispatched: every press was answered by its planned release.
        // No global release was required — do not claim one happened.
        out.cleanup_outcome = CleanupOutcome::NotNeeded;
    }

    // --- settle --------------------------------------------------------------
    if !out.cancelled {
        let settle = config.settle_delay.min(OBSERVE_PHASE_BUDGET);
        if sleep_cancellable(settle, cancel).is_err() {
            out.cancelled = true;
            out.settled = false;
        }
    } else {
        out.settled = false;
    }
    if cancel.load(Ordering::SeqCst) && !out.cancelled {
        out.cancelled = true;
    }
    out
}

/// Best-effort pass over planned releases in `[from, to)` that answer a
/// press which was actually injected (tracked in `out.pressed`). Releases
/// answering a failed or never-attempted press are skipped — injecting them
/// could unlock UI state (window drag, menu, modifier) the user owns.
///
/// Each answered release is attempted once, bounded by the cleanup budget:
/// releasing a physically held key is mandatory cleanup, so when the
/// input-phase deadline has already expired the pass gets a small extra
/// wall-clock allowance instead of skipping releases outright. Failures are
/// recorded but do not stop the remaining releases. Successful releases are
/// added to `events_completed` so cleanup attribution stays exact.
fn do_release_pass(
    plan: &Plan,
    backend: &mut dyn Backend,
    map: &impl Fn([i32; 2]) -> (i32, i32),
    from: usize,
    to: usize,
    deadline: &Instant,
    out: &mut ExecutionOutcome,
) {
    let cleanup_deadline = (*deadline).max(Instant::now() + RELEASE_PASS_ALLOWANCE);
    let pressed: Vec<usize> = out.pressed.clone();
    for index in from..to.min(plan.events.len()) {
        if !plan.is_answered_release(index, &pressed) {
            continue;
        }
        if Instant::now() > cleanup_deadline {
            return; // remaining releases are covered by release_all
        }
        if let Some(be) = plan.backend_event(index, map) {
            match backend.inject(&be) {
                Ok(()) => out.events_completed.push(index),
                Err(e) => {
                    if out.error.is_none() {
                        out.error = Some(ToolError::from(e));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::actions::Action;
    use crate::runtime::plan::compile_plan;
    use crate::runtime::testutil::{be, FakeBackend};

    fn run(
        action: Action,
        backend: &mut FakeBackend,
        cancel: &Arc<AtomicBool>,
        config: &ExecutionConfig,
    ) -> ExecutionOutcome {
        let plan = compile_plan(&action, 3);
        execute_plan(&plan, backend, |p| (p[0], p[1]), cancel, config)
    }

    #[test]
    fn fully_dispatched_reports_no_cleanup_needed() {
        let mut b = FakeBackend::new(1600, 900);
        let c = Arc::new(AtomicBool::new(false));
        let out = run(
            Action::KeyHold {
                key: "shift".into(),
                duration_ms: 20,
            },
            &mut b,
            &c,
            &ExecutionConfig {
                settle_delay: Duration::from_millis(5),
                ..ExecutionConfig::production()
            },
        );
        assert_eq!(out.input_outcome, InputOutcome::Dispatched);
        assert_eq!(out.cleanup_outcome, CleanupOutcome::NotNeeded);
        assert_eq!(b.release_all_calls(), 0);
        assert!(out.settled);
    }

    #[test]
    fn mid_text_cancellation_reports_partial_and_releases() {
        // Deterministic: the cancel flag is asserted by the test seam after
        // exactly 3 chunks are injected, with no thread sleeps or wall-clock
        // races. The flag is honored at the next event boundary, exactly like
        // a transport-set cancel.
        let mut b = FakeBackend::new(1600, 900);
        let c = Arc::new(AtomicBool::new(false));
        let text = "x".repeat(64 * 10); // 10 text events
        let out = run(
            Action::TextInput { text },
            &mut b,
            &c,
            &ExecutionConfig {
                cancel_after_injects: Some(3),
                input_budget: Duration::from_secs(5),
                settle_delay: Duration::from_millis(200),
            },
        );
        assert!(out.cancelled);
        assert_eq!(out.input_outcome, InputOutcome::Partial);
        assert_eq!(out.events_completed.len(), 3);
        assert_eq!(out.events_total, 10);
        assert!(b.events().len() >= 3);
        assert_eq!(out.cleanup_outcome, CleanupOutcome::Released);
        assert!(!out.settled);
    }

    #[test]
    fn cancel_before_first_event_is_not_started() {
        let mut b = FakeBackend::new(1600, 900);
        let c = Arc::new(AtomicBool::new(true));
        let out = run(
            Action::TextInput { text: "abc".into() },
            &mut b,
            &c,
            &ExecutionConfig::production(),
        );
        assert_eq!(out.input_outcome, InputOutcome::NotStarted);
        assert_eq!(out.cleanup_outcome, CleanupOutcome::NotNeeded);
        assert!(b.events().is_empty());
    }

    #[test]
    fn deadline_truncates_max_hold_and_reports_partial() {
        let mut b = FakeBackend::new(1600, 900);
        let c = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        let out = run(
            Action::KeyHold {
                key: "shift".into(),
                duration_ms: 5000, // the advertised maximum
            },
            &mut b,
            &c,
            &ExecutionConfig {
                input_budget: Duration::from_millis(150),
                settle_delay: Duration::from_millis(10),
                ..ExecutionConfig::production()
            },
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "must not actually wait the full hold"
        );
        assert_eq!(out.input_outcome, InputOutcome::Partial);
        assert_eq!(out.error.as_ref().unwrap().code, codes::DEADLINE_EXCEEDED);
        assert_eq!(out.cleanup_outcome, CleanupOutcome::Released);
        assert!(b.release_all_calls() >= 1);
        // The planned release for the pressed key was attempted.
        use crate::backend::{Direction, InputEvent};
        assert!(b.events().iter().any(|e| matches!(
            e,
            InputEvent::Key {
                key,
                direction: Direction::Release
            } if key == "shift"
        )));
    }

    #[test]
    fn first_event_failure_reports_partial_with_possible_side_effects() {
        let mut b = FakeBackend::new(1600, 900);
        b.fail_inject_at.push_back((0, be("input_error", "flaky")));
        let c = Arc::new(AtomicBool::new(false));
        let out = run(
            Action::Move { position: [3, 4] },
            &mut b,
            &c,
            &ExecutionConfig {
                settle_delay: Duration::from_millis(5),
                ..ExecutionConfig::production()
            },
        );
        assert_eq!(out.input_outcome, InputOutcome::Partial);
        assert_eq!(out.error.as_ref().unwrap().code, "input_error");
        assert!(out.events_completed.is_empty());
    }

    #[test]
    fn cleanup_failure_is_reported_not_hidden() {
        let plan = compile_plan(
            &Action::KeyChord {
                modifiers: vec!["ctrl".into()],
                key: "s".into(),
            },
            1,
        );
        let mut backend = FakeBackend::new(1600, 900);
        // First press lands, second event fails, release_all fails too.
        backend
            .fail_inject_at
            .push_back((1, be("input_error", "boom")));
        backend.fail_release_all = Some(be("cleanup_error", "stuck"));
        let c = Arc::new(AtomicBool::new(false));
        let out = execute_plan(
            &plan,
            &mut backend,
            |p| (p[0], p[1]),
            &c,
            &ExecutionConfig {
                settle_delay: Duration::from_millis(5),
                ..ExecutionConfig::production()
            },
        );
        assert_eq!(out.input_outcome, InputOutcome::Partial);
        assert_eq!(out.cleanup_outcome, CleanupOutcome::Failed);
        assert!(out.error.is_some());
        // The release pass attempted the planned releases before release_all.
        use crate::backend::{Direction, InputEvent};
        assert!(backend.events().iter().any(|e| matches!(
            e,
            InputEvent::Key {
                direction: Direction::Release,
                ..
            }
        )));
    }
}
