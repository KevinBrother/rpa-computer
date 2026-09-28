//! 9. Registry / cache boundaries.
//!
//! The 1000-request bound test is `#[ignore]`d by default: run it explicitly
//! with `cargo test --test runtime_contract -- --ignored registry` once the
//! core runtime's per-step settle behavior is configured for test speed (the
//! runtime owns timing seams; QA must not weaken production defaults to make
//! this fast). A tiny 32×32 capture fixture keeps encode/decode cost minimal.

use crate::support::*;

/// One non-ignored regression: distinct ids dispatch exactly once each.
#[test]
fn distinct_request_ids_each_dispatch_once() {
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    for i in 0..3 {
        // Re-observe between steps to avoid staleness.
        let obs_now = if i == 0 {
            obs_id.clone()
        } else {
            observe(&mut h.runtime, &session)["observation_id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let r = call(
            &mut h.runtime,
            "computer_step",
            step_args(
                &session,
                &format!("req-distinct-{i}"),
                &obs_now,
                serde_json::json!({"kind": "move", "position": [10 + i, 10]}),
            ),
        );
        assert!(
            !r.is_error,
            "distinct request {i} must dispatch: {:?}",
            r.data
        );
    }
    let events = h.mock.lock().unwrap().inject_count();
    assert_eq!(events, 3, "each distinct move dispatches exactly one Move");
}

/// Full 1000-request registry bound. Slow (bounded by per-step runtime
/// timing, which the runtime owns), so it is opt-in via --ignored. The tiny
/// 32x32 fixture keeps per-step image cost negligible.
#[test]
#[ignore = "slow: 1000 sequential steps; run explicitly with --ignored"]
fn request_registry_is_bounded_at_1000_without_evicting_retriable_records() {
    let mut h = harness_with(MockBackend::new(32, 32));
    let session = open_session(&mut h.runtime);

    // Fill the registry with 1000 distinct successful requests.
    let mut last_obs = observe(&mut h.runtime, &session);
    for i in 0..1000usize {
        let obs_id = last_obs["observation_id"].as_str().unwrap().to_string();
        let r = call(
            &mut h.runtime,
            "computer_step",
            step_args(
                &session,
                &format!("req-fill-{i}"),
                &obs_id,
                serde_json::json!({"kind": "move", "position": [1, 1]}),
            ),
        );
        assert!(!r.is_error, "fill step {i} failed: {:?}", r.data);
        // Each step returns a fresh observation to base the next on.
        if let Some(new_obs) = r.data.get("observation") {
            last_obs = new_obs.clone();
        } else {
            last_obs = observe(&mut h.runtime, &session);
        }
    }

    // The 1001st distinct request must be rejected with resource_limit —
    // NOT by silently evicting an earlier (still replayable) record.
    let obs_id = last_obs["observation_id"].as_str().unwrap().to_string();
    let overflow = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-overflow",
            &obs_id,
            serde_json::json!({"kind": "move", "position": [2, 2]}),
        ),
    );
    let code = error_code(&overflow);
    assert_eq!(
        code, "resource_limit",
        "1001st request must be resource_limit, got {code}"
    );

    // An early record must still be replayable (dedup intact): metadata-only
    // retention means get_step of req-fill-0 must still return its recorded
    // input outcome even if the image payload was evicted for memory.
    let replay = call(
        &mut h.runtime,
        "computer_get_step",
        serde_json::json!({"session_id": session, "request_id": "req-fill-0"}),
    );
    assert!(
        !replay.is_error,
        "early request records must not be evicted to make room: {:?}",
        replay.data
    );
    assert_eq!(
        replay.data["input_outcome"].as_str().unwrap_or("?"),
        "dispatched",
        "metadata-only retention must preserve the recorded outcome"
    );
}
