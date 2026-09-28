//! 1. Validation BEFORE any backend event is dispatched.

use crate::support::*;

#[test]
fn invalid_action_kinds_and_shapes_never_dispatch_input() {
    let cases: Vec<serde_json::Value> = vec![
        // unknown kind
        serde_json::json!({"kind": "teleport", "position": [10, 10]}),
        // missing kind
        serde_json::json!({"position": [10, 10]}),
        // click out of image bounds
        serde_json::json!({"kind": "click", "position": [10_000, 10]}),
        serde_json::json!({"kind": "click", "position": [10, 99_999]}),
        serde_json::json!({"kind": "click", "position": [-1, 10]}),
        // malformed position
        serde_json::json!({"kind": "click", "position": [10]}),
        serde_json::json!({"kind": "click", "position": ["10", 10]}),
        serde_json::json!({"kind": "click", "position": null}),
        // invalid button
        serde_json::json!({"kind": "click", "position": [10, 10], "button": "hyper"}),
        // click count out of 1..=3
        serde_json::json!({"kind": "click", "position": [10, 10], "count": 0}),
        serde_json::json!({"kind": "click", "position": [10, 10], "count": 4}),
        // move out of bounds
        serde_json::json!({"kind": "move", "position": [-5, 0]}),
        // drag path too short / too long / out of bounds
        serde_json::json!({"kind": "drag", "path": [[1, 1]]}),
        serde_json::json!({"kind": "drag", "path": vec![[1, 1]; 257]}),
        serde_json::json!({"kind": "drag", "path": [[1, 1], [-1, 2]]}),
        // drag duration above max 5000
        serde_json::json!({"kind": "drag", "path": [[1, 1], [2, 2]], "duration_ms": 5001}),
        // scroll beyond 100 wheel ticks
        serde_json::json!({"kind": "scroll", "position": [5, 5], "delta_x": 0, "delta_y": 101}),
        serde_json::json!({"kind": "scroll", "position": [5, 5], "delta_x": -101, "delta_y": 0}),
        serde_json::json!({"kind": "scroll", "position": [5, 5], "delta_x": 0, "delta_y": 1, "unit": "pixels"}),
        // text too long (>4096 chars)
        serde_json::json!({"kind": "text_input", "text": "x".repeat(4097)}),
        // key_chord with unknown key name
        serde_json::json!({"kind": "key_chord", "modifiers": [], "key": "not-a-key"}),
        serde_json::json!({"kind": "key_chord", "modifiers": ["ctrl"], "key": "nope"}),
        serde_json::json!({"kind": "key_chord", "modifiers": ["bogus-mod"], "key": "a"}),
        // key_chord with multi-char non-named key
        serde_json::json!({"kind": "key_chord", "modifiers": [], "key": "ab"}),
        // key_hold above 5000 ms
        serde_json::json!({"kind": "key_hold", "key": "a", "duration_ms": 5001}),
        // key_hold with invalid key
        serde_json::json!({"kind": "key_hold", "key": "no-such-key", "duration_ms": 10}),
    ];

    for (i, action) in cases.into_iter().enumerate() {
        let mut h = harness();
        let session = open_session(&mut h.runtime);
        let obs = observe(&mut h.runtime, &session);
        let obs_id = obs["observation_id"].as_str().unwrap().to_string();

        let reply = call(
            &mut h.runtime,
            "computer_step",
            step_args(
                &session,
                &format!("req-invalid-{i}"),
                &obs_id,
                action.clone(),
            ),
        );

        let code = error_code(&reply);
        assert!(
            code == "invalid_action" || code == "unsupported_action",
            "case {i} action {action:?}: expected invalid_action/unsupported_action, got {code}"
        );
        let mock = h.mock.lock().unwrap();
        assert_eq!(
            mock.inject_count(),
            0,
            "case {i} action {action:?}: invalid action must be rejected before ANY backend event; log: {:?}",
            mock.log
        );
    }
}

#[test]
fn validation_error_reports_not_started_and_no_image() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-not-started",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [-1, -1]}),
        ),
    );
    assert!(reply.is_error);
    assert_eq!(
        reply.data["input_outcome"]
            .as_str()
            .unwrap_or("not_started"),
        "not_started",
        "a rejected action must be distinguishable from a dispatched one: {:?}",
        reply.data
    );
    assert!(
        reply.image_png.is_none(),
        "a rejected action must not attach a post-action screenshot"
    );
}

#[test]
fn close_rejects_further_input() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let close = call(
        &mut h.runtime,
        "computer_close",
        serde_json::json!({"session_id": session}),
    );
    assert!(!close.is_error, "close must succeed: {:?}", close.data);

    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-after-close",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [10, 10]}),
        ),
    );
    assert!(reply.is_error, "step on closed session must fail");
    assert_eq!(h.mock.lock().unwrap().inject_count(), 0);
}
