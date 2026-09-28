//! 3. Stale sequence / based_on rejection and geometry faults.

use crate::support::*;

#[test]
fn step_with_unknown_observation_is_rejected_before_input() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-unknown-obs",
            "observation-that-never-existed",
            serde_json::json!({"kind": "click", "position": [10, 10]}),
        ),
    );
    let code = error_code(&reply);
    assert_eq!(
        code, "stale_observation",
        "unknown based_on must be stale_observation, got {code}"
    );
    assert_eq!(h.mock.lock().unwrap().inject_count(), 0);
}

#[test]
fn input_invalidates_prior_observation() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs1 = observe(&mut h.runtime, &session);
    let obs1_id = obs1["observation_id"].as_str().unwrap().to_string();

    // First step succeeds.
    let r1 = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-first",
            &obs1_id,
            serde_json::json!({"kind": "click", "position": [10, 10]}),
        ),
    );
    assert!(!r1.is_error, "first step must dispatch: {:?}", r1.data);
    assert_eq!(r1.data["input_outcome"].as_str().unwrap(), "dispatched");

    // Second step based on the now-stale observation must be rejected.
    let r2 = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-second",
            &obs1_id,
            serde_json::json!({"kind": "click", "position": [20, 20]}),
        ),
    );
    let code = error_code(&r2);
    assert_eq!(
        code, "stale_observation",
        "step based on a pre-input observation must be stale_observation, got {code}"
    );
    // Exactly the first click's events were dispatched; the stale step must
    // have added no further input events.
    let mock = h.mock.lock().unwrap();
    let after_first = mock.inject_count();
    assert!(after_first > 0);
    let events = mock.events();
    let r2_marker = events.iter().filter(|e| e.contains("x: 20")).count();
    assert_eq!(
        r2_marker, 0,
        "stale step must not dispatch its (20,20) click; log: {:?}",
        mock.log
    );
}

#[test]
fn read_only_observe_does_not_invalidate_based_on() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs1 = observe(&mut h.runtime, &session);
    let seq1 = obs1["input_sequence"].as_u64().expect("input_sequence");
    let obs2 = observe(&mut h.runtime, &session);
    let seq2 = obs2["input_sequence"].as_u64().expect("input_sequence");
    assert_eq!(
        seq1, seq2,
        "read-only observes must share the input sequence"
    );
    // A step based on the latest observation (same sequence) is accepted.
    let r = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-after-observes",
            obs2["observation_id"].as_str().unwrap(),
            serde_json::json!({"kind": "move", "position": [100, 100]}),
        ),
    );
    assert!(
        !r.is_error,
        "step based on current sequence must run: {:?}",
        r.data
    );
}

#[test]
fn geometry_change_rejects_old_actions_and_faults() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();
    let geom_v = obs["geometry_version"].as_str().unwrap().to_string();
    assert_eq!(geom_v, "geom-v1");

    // Simulate a monitor/DPI change between observe and step.
    {
        let mut m = h.mock.lock().unwrap();
        m.geometry.version = "geom-v2".into();
        m.geometry.input_size = (2560, 1440);
        m.default_capture.geometry = m.geometry.clone();
    }

    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-geom-change",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [10, 10]}),
        ),
    );
    let code = error_code(&reply);
    assert!(
        code == "geometry_changed" || code == "stale_observation",
        "geometry change must reject old actions, got {code}"
    );
    assert_eq!(
        h.mock.lock().unwrap().inject_count(),
        0,
        "no input may be dispatched against changed geometry"
    );
}

#[test]
fn observation_carries_required_contract_fields() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    for field in [
        "observation_id",
        "session_id",
        "surface_id",
        "geometry_version",
        "input_sequence",
        "width_px",
        "height_px",
    ] {
        assert!(
            obs.get(field).is_some(),
            "observation must contain {field}: {obs:?}"
        );
    }
    assert_eq!(obs["session_id"].as_str().unwrap(), session);
    assert_eq!(obs["surface_id"].as_str().unwrap(), "mock-surface-0");
}
