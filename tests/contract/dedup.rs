//! 4. Dedup after side effects / same id different payload.

use crate::support::*;

#[test]
fn replayed_request_id_returns_prior_result_without_repeating_input() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();
    let action = serde_json::json!({"kind": "click", "position": [42, 24]});

    let r1 = call(
        &mut h.runtime,
        "computer_step",
        step_args(&session, "req-dedup", &obs_id, action.clone()),
    );
    assert!(!r1.is_error, "first dispatch: {:?}", r1.data);
    let events_after_first = h.mock.lock().unwrap().inject_count();
    assert!(events_after_first > 0, "first call must dispatch input");

    // Exact replay: same request_id, same body — even though the observation
    // is now stale, dedup must win over stale validation and return the
    // recorded result WITHOUT any new input.
    let r2 = call(
        &mut h.runtime,
        "computer_step",
        step_args(&session, "req-dedup", &obs_id, action),
    );
    assert_eq!(
        h.mock.lock().unwrap().inject_count(),
        events_after_first,
        "dedup replay must never re-dispatch input"
    );
    assert_eq!(
        r2.is_error, r1.is_error,
        "replay must return the prior outcome verbatim; r1={:?} r2={:?}",
        r1.data, r2.data
    );
    assert_eq!(
        r2.data["input_outcome"], r1.data["input_outcome"],
        "replay must preserve the recorded input outcome"
    );
    assert_eq!(
        r2.data["request_id"].as_str().unwrap_or("req-dedup"),
        "req-dedup"
    );
}

#[test]
fn same_request_id_with_different_payload_is_conflict() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let r1 = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-conflict",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [1, 1]}),
        ),
    );
    assert!(!r1.is_error, "first: {:?}", r1.data);
    let events = h.mock.lock().unwrap().inject_count();

    let r2 = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-conflict",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [2, 2]}),
        ),
    );
    let code = error_code(&r2);
    assert!(
        code.contains("conflict") || code == "invalid_action" || code == "duplicate_request",
        "same id + different payload must be a conflict-style error, got {code}"
    );
    assert_eq!(
        h.mock.lock().unwrap().inject_count(),
        events,
        "conflicting replay must not dispatch input"
    );
}

#[test]
fn dedup_applies_even_when_first_attempt_failed_after_registration() {
    // The request is registered BEFORE the first input. If dispatch fails
    // mid-action, replaying the same request_id must not blindly retry.
    let mut mock = MockBackend::new(1280, 720);
    mock.inject_results
        .push_back(Err(err("input_error", "boom on first event")));
    mock.inject_results.push_back(Ok(()));
    let mut h = harness_with(mock);

    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();
    let action = serde_json::json!({"kind": "key_chord", "modifiers": ["ctrl"], "key": "a"});

    let r1 = call(
        &mut h.runtime,
        "computer_step",
        step_args(&session, "req-partial-dedup", &obs_id, action.clone()),
    );
    let events = h.mock.lock().unwrap().inject_count();
    let outcome1 = r1.data["input_outcome"].as_str().unwrap_or("?").to_string();
    assert!(
        outcome1 == "partial" || outcome1 == "unknown" || r1.is_error,
        "mid-action failure must surface as partial/unknown/error, got {outcome1}"
    );

    // Replay with the same body: must NOT re-dispatch (no blind retry).
    let r2 = call(
        &mut h.runtime,
        "computer_step",
        step_args(&session, "req-partial-dedup", &obs_id, action),
    );
    assert_eq!(
        h.mock.lock().unwrap().inject_count(),
        events,
        "no blind retry of a partially executed request"
    );
    assert_eq!(
        r2.data["input_outcome"].as_str().unwrap_or("?"),
        outcome1,
        "replay must return the recorded (failed) outcome"
    );
}

#[test]
fn get_step_returns_recorded_result_without_input() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let r1 = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-get",
            &obs_id,
            serde_json::json!({"kind": "move", "position": [7, 7]}),
        ),
    );
    assert!(!r1.is_error, "step: {:?}", r1.data);
    let events = h.mock.lock().unwrap().inject_count();

    let g = call(
        &mut h.runtime,
        "computer_get_step",
        serde_json::json!({"session_id": session, "request_id": "req-get"}),
    );
    assert!(
        !g.is_error,
        "get_step of known request must succeed: {:?}",
        g.data
    );
    assert_eq!(g.data["input_outcome"], r1.data["input_outcome"]);
    assert_eq!(
        h.mock.lock().unwrap().inject_count(),
        events,
        "get_step must never dispatch input"
    );

    // Unknown request id must be a structured error, not a fabricated result.
    let g2 = call(
        &mut h.runtime,
        "computer_get_step",
        serde_json::json!({"session_id": session, "request_id": "never-seen"}),
    );
    assert!(g2.is_error, "get_step of unknown request must fail");
}
