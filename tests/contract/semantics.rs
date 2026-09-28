//! 10. Event-shape semantics and misc protocol hygiene.

use crate::support::*;

#[test]
fn unknown_tool_name_is_structured_error() {
    let mut h = harness();
    let reply = call(&mut h.runtime, "computer_nuke", serde_json::json!({}));
    assert!(reply.is_error);
    let code = error_code(&reply);
    assert!(
        code == "unsupported_action" || code == "unknown_tool" || code == "invalid_action",
        "unknown tool must be a structured error, got {code}"
    );
}

#[test]
fn step_requires_session_id_and_valid_session() {
    let mut h = harness();
    let _ = open_session(&mut h.runtime);
    let reply = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            "no-such-session",
            "req-x",
            "obs-x",
            serde_json::json!({"kind": "move", "position": [1, 1]}),
        ),
    );
    assert!(reply.is_error, "unknown session must be rejected");
    assert_eq!(h.mock.lock().unwrap().inject_count(), 0);
}

#[test]
fn key_chord_uses_key_events_not_text() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let r = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-chord",
            &obs_id,
            serde_json::json!({"kind": "key_chord", "modifiers": ["ctrl"], "key": "c"}),
        ),
    );
    assert!(!r.is_error, "valid chord must dispatch: {:?}", r.data);
    let mock = h.mock.lock().unwrap();
    let events = mock.events();
    assert!(
        events.iter().all(|e| e.contains("Key")),
        "chords must use Key press/release events, never Text: {:?}",
        events
    );
    assert!(
        events.iter().any(|e| e.contains("Press")),
        "chord must include press events: {:?}",
        events
    );
    assert!(
        events.iter().any(|e| e.contains("Release")),
        "chord must include release events: {:?}",
        events
    );
}

#[test]
fn scroll_uses_wheel_ticks_with_contract_sign() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let r = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-scroll",
            &obs_id,
            serde_json::json!({
                "kind": "scroll",
                "position": [100, 100],
                "delta_x": 0,
                "delta_y": 3,
                "unit": "wheel_ticks"
            }),
        ),
    );
    assert!(!r.is_error, "valid scroll: {:?}", r.data);
    let mock = h.mock.lock().unwrap();
    let scrolls: Vec<&str> = mock
        .events()
        .into_iter()
        .filter(|e| e.contains("Scroll"))
        .collect();
    assert_eq!(
        scrolls.len(),
        1,
        "one Scroll event expected: {:?}",
        mock.events()
    );
    // CONTRACT: positive delta_y = down. The runtime passes wheel ticks
    // through; the backend owns OS sign mapping. Assert the runtime didn't
    // silently flip or scale.
    assert!(
        scrolls[0].contains("y: 3"),
        "runtime must pass delta_y=3 wheel ticks through unscaled/unflipped, got {}",
        scrolls[0]
    );
}

#[test]
fn key_hold_dispatches_press_then_release_within_limit() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let r = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-hold",
            &obs_id,
            serde_json::json!({"kind": "key_hold", "key": "shift", "duration_ms": 50}),
        ),
    );
    assert!(!r.is_error, "valid hold: {:?}", r.data);
    let mock = h.mock.lock().unwrap();
    let events = mock.events();
    let press = events.iter().position(|e| e.contains("Press"));
    let release = events.iter().rposition(|e| e.contains("Release"));
    assert!(
        press.is_some() && release.is_some(),
        "hold needs press+release: {:?}",
        events
    );
    assert!(
        press.unwrap() < release.unwrap(),
        "press must precede release"
    );
}

#[test]
fn text_input_uses_text_event_and_accepts_unicode() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let r = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-text",
            &obs_id,
            serde_json::json!({"kind": "text_input", "text": "你好，世界"}),
        ),
    );
    assert!(!r.is_error, "unicode text input: {:?}", r.data);
    let mock = h.mock.lock().unwrap();
    let events = mock.events();
    assert!(
        events.iter().any(|e| e.contains("Text")),
        "text_input must use a Text event (not per-char chords): {:?}",
        events
    );
}
