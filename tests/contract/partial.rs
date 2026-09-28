//! 6. Partial dispatch vs not_started.

use crate::support::*;

#[test]
fn partial_dispatch_is_distinguishable_from_not_started_and_dispatched() {
    // Fail on the SECOND event of a chord (press ctrl ok, next event fails).
    let mut mock = MockBackend::new(1280, 720);
    mock.inject_results.push_back(Ok(())); // press ctrl
    mock.inject_results
        .push_back(Err(err("input_error", "second press failed")));
    let mut h = harness_with(mock);

    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-partial",
            &obs_id,
            serde_json::json!({"kind": "key_chord", "modifiers": ["ctrl"], "key": "c"}),
        ),
    );
    let outcome = reply
        .data
        .get("input_outcome")
        .and_then(|o| o.as_str())
        .expect("step result must carry input_outcome");
    assert_ne!(
        outcome, "not_started",
        "one event was dispatched; outcome must not claim not_started"
    );
    assert_ne!(
        outcome, "dispatched",
        "the chord did not complete; outcome must not claim fully dispatched"
    );
    assert!(
        outcome == "partial" || outcome == "unknown",
        "expected partial/unknown, got {outcome}"
    );

    // CONTRACT §18: a failed inject may have partially applied side effects,
    // so the reported completed event range must never imply the failed event
    // had no effect. If the runtime reports per-event completion metadata, it
    // must not include the failed event index as definitively applied — the
    // boundary between "known applied" and "unknown" must be at or before the
    // failing event, or the event must be marked unknown.
    if let Some(completed) = reply
        .data
        .get("events_completed")
        .and_then(|e| e.as_array())
    {
        let total = reply
            .data
            .get("events_total")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        assert!(
            (completed.len() as u64) < total.max(1),
            "events_completed must not claim ALL events applied when one inject failed: {:?}",
            reply.data
        );
    }
}

#[test]
fn outcome_fields_present_on_step_result() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-fields",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [5, 5]}),
        ),
    );
    assert!(!reply.is_error, "{:?}", reply.data);
    for field in [
        "request_id",
        "input_outcome",
        "observation_outcome",
        "cleanup_outcome",
    ] {
        assert!(
            reply.data.get(field).is_some(),
            "step result must contain {field}: {:?}",
            reply.data
        );
    }
    assert_eq!(reply.data["request_id"].as_str().unwrap(), "req-fields");
    // cleanup_outcome must be a known enum value, never an invented one.
    let cleanup = reply.data["cleanup_outcome"].as_str().unwrap();
    assert!(
        ["not_needed", "released", "failed", "unknown"].contains(&cleanup),
        "cleanup_outcome must be a contract enum value, got {cleanup}"
    );
}
