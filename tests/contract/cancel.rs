//! 8. Cancellation: the cancel flag stops new input; pause blocks input and resume invalidates pre-pause observations.

use std::sync::atomic::Ordering;

use crate::support::*;

#[test]
fn cancel_flag_prevents_new_input_and_marks_cancelled() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    // Transport sets cancel=true immediately on stop; runtime must refuse to
    // start new input.
    h.cancel.store(true, Ordering::SeqCst);

    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-cancelled",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [10, 10]}),
        ),
    );
    let outcome = reply
        .data
        .get("input_outcome")
        .and_then(|o| o.as_str())
        .unwrap_or("not_started");
    assert_eq!(
        h.mock.lock().unwrap().inject_count(),
        0,
        "cancelled runtime must not dispatch new input"
    );
    if reply.is_error {
        let code = error_code(&reply);
        assert_eq!(code, "cancelled", "expected cancelled, got {code}");
    }
    assert_ne!(
        outcome, "dispatched",
        "a cancelled step must never report dispatched"
    );
}

#[test]
fn pause_blocks_input_and_resume_requires_reobserve() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let p = call(
        &mut h.runtime,
        "computer_pause",
        serde_json::json!({"session_id": session}),
    );
    assert!(!p.is_error, "pause must succeed: {:?}", p.data);

    let blocked = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-paused",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [10, 10]}),
        ),
    );
    assert!(blocked.is_error, "input while paused must be rejected");
    assert_eq!(
        h.mock.lock().unwrap().inject_count(),
        0,
        "no input may be dispatched while paused"
    );

    let r = call(
        &mut h.runtime,
        "computer_resume",
        serde_json::json!({"session_id": session}),
    );
    assert!(!r.is_error, "resume must succeed: {:?}", r.data);

    // After resume, the pre-pause observation is stale: decisions made while
    // paused must be discarded and re-observed.
    let stale = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-after-resume-stale",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [10, 10]}),
        ),
    );
    if !stale.is_error {
        // The contract says "resume ... may reset only after safe cleanup/state
        // validation" and the design requires re-observe; prefer rejection.
        panic!(
            "step based on pre-pause observation after resume must be rejected (design: resume invalidates pending decisions); got success: {:?}",
            stale.data
        );
    } else {
        let code = error_code(&stale);
        assert!(
            code == "stale_observation" || code == "cancelled" || code == "invalid_action",
            "unexpected error code for pre-pause observation after resume: {code}"
        );
    }
    assert_eq!(h.mock.lock().unwrap().inject_count(), 0);
}

#[test]
fn cancelled_long_action_does_not_report_dispatched() {
    // Cancel between registration and dispatch of a multi-event action:
    // with cancel set before the call, the action must not start at all and
    // cleanup must still be attempted (held-state safety).
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    h.cancel.store(true, Ordering::SeqCst);
    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-cancelled-chord",
            &obs_id,
            serde_json::json!({"kind": "key_chord", "modifiers": ["ctrl", "shift"], "key": "x"}),
        ),
    );
    assert_eq!(
        h.mock.lock().unwrap().inject_count(),
        0,
        "cancelled chord must not dispatch any event"
    );
    let outcome = reply
        .data
        .get("input_outcome")
        .and_then(|o| o.as_str())
        .unwrap_or("not_started");
    assert!(
        outcome == "not_started" || outcome == "cancelled",
        "cancelled chord outcome must be not_started/cancelled, got {outcome}"
    );
}
