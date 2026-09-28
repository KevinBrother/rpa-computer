//! 5. Cleanup failure honesty, release on all failure paths, and EXPLICIT idempotent close/shutdown state.
//!
//! Strengthened: arbitrary errors from repeated close are no longer accepted without checking the reported state.

use crate::support::*;

#[test]
fn cleanup_failure_is_reported_not_masked() {
    let mut mock = MockBackend::new(1280, 720);
    mock.release_results
        .push_back(Err(err("cleanup_error", "button stuck")));
    let mut h = harness_with(mock);

    let session = open_session(&mut h.runtime);
    let reply = call(
        &mut h.runtime,
        "computer_close",
        serde_json::json!({"session_id": session}),
    );
    // Close must NOT report a clean "released" state when release failed.
    let cleanup = reply
        .data
        .get("cleanup_outcome")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    assert!(
        reply.is_error || cleanup == "failed" || cleanup == "unknown",
        "cleanup failure must be visible (is_error or cleanup_outcome failed/unknown), got: {:?}",
        reply.data
    );
    assert!(
        !reply
            .data
            .get("cleanup_outcome")
            .and_then(|c| c.as_str())
            .map(|c| c == "released")
            .unwrap_or(false),
        "must never claim 'released' when release_all returned an error"
    );
}

#[test]
fn shutdown_reports_cleanup_failure_and_is_idempotent() {
    let mut mock = MockBackend::new(1280, 720);
    mock.release_results
        .push_back(Err(err("cleanup_error", "stuck key")));
    mock.release_results.push_back(Ok(()));
    let mut h = harness_with(mock);
    let _session = open_session(&mut h.runtime);

    let s1 = h.runtime.shutdown();
    let cleanup = s1
        .data
        .get("cleanup_outcome")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    assert!(
        s1.is_error || cleanup == "failed" || cleanup == "unknown",
        "shutdown with failing release must not claim success: {:?}",
        s1.data
    );

    // Second shutdown must be safe and MUST NOT invent a success: with the
    // release retry succeeding, the recorded outcome may become released;
    // without a retry it must still carry the recorded failure. Either way
    // it must never panic, and must never claim "released" while the mock's
    // last release_all answer was an error.
    let s2 = h.runtime.shutdown();
    let cleanup2 = s2
        .data
        .get("cleanup_outcome")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let releases = h.mock.lock().unwrap().release_count();
    if releases == 1 {
        // No retry happened: outcome must still reflect the original failure.
        assert!(
            s2.is_error || cleanup2 == "failed" || cleanup2 == "unknown",
            "without a release retry the recorded failure must persist, got: {:?}",
            s2.data
        );
    }
    // If a retry DID happen (releases == 2) the mock answered Ok, so any of
    // released/failed/unknown reported honestly is acceptable.
}

#[test]
fn release_is_attempted_on_all_failure_paths() {
    // Even when dispatch fails mid-action, the runtime must attempt cleanup.
    let mut mock = MockBackend::new(1280, 720);
    mock.inject_results.push_back(Ok(()));
    mock.inject_results
        .push_back(Err(err("input_error", "second event failed")));
    let mut h = harness_with(mock);

    let session = open_session(&mut h.runtime);
    let obs = observe(&mut h.runtime, &session);
    let obs_id = obs["observation_id"].as_str().unwrap().to_string();

    let _ = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-mid-fail",
            &obs_id,
            serde_json::json!({"kind": "key_chord", "modifiers": ["ctrl", "shift"], "key": "x"}),
        ),
    );
    let releases = h.mock.lock().unwrap().release_count();
    assert!(
        releases >= 1,
        "cleanup (release_all) must be attempted after mid-action failure"
    );
}

#[test]
fn close_is_idempotent_with_explicit_terminal_state() {
    // Strengthened (was: "any structured error is fine"). A repeated close
    // must either succeed idempotently reporting an explicit terminal state
    // (closed/faulted), or fail with a session-scoped code — and must never
    // dispatch input or claim a fresh release it did not perform.
    let mut h = harness();
    let session = open_session(&mut h.runtime);
    let c1 = call(
        &mut h.runtime,
        "computer_close",
        serde_json::json!({"session_id": session}),
    );
    assert!(!c1.is_error, "first close: {:?}", c1.data);
    let releases_after_first = h.mock.lock().unwrap().release_count();
    assert!(
        releases_after_first >= 1,
        "close must attempt input release"
    );

    let c2 = call(
        &mut h.runtime,
        "computer_close",
        serde_json::json!({"session_id": session}),
    );
    if c2.is_error {
        // Acceptable: the session is gone/closed, reported as a structured
        // session-scoped error — not an internal failure.
        let code = error_code(&c2);
        assert!(
            code == "session_not_found" || code == "session_closed" || code.contains("session"),
            "second close error must be session-scoped, got {code}"
        );
    } else {
        // Idempotent success: state must be an explicit terminal value and
        // cleanup_outcome must not fabricate a release.
        let state = c2.data.get("state").and_then(|s| s.as_str()).unwrap_or("");
        assert!(
            state == "closed" || state == "faulted",
            "idempotent close must report explicit terminal state closed/faulted, got {state:?} in {:?}",
            c2.data
        );
        let cleanup = c2
            .data
            .get("cleanup_outcome")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        assert!(
            ["not_needed", "released", "failed", "unknown"].contains(&cleanup),
            "cleanup_outcome must be a contract enum value, got {cleanup}"
        );
    }
    // Repeated close must not dispatch new input.
    assert_eq!(h.mock.lock().unwrap().inject_count(), 0);
}

#[test]
fn close_after_cleanup_failure_reports_faulted_not_released() {
    // First close fails cleanup. A second close must still not claim
    // "released" — the failed cleanup state is sticky.
    let mut mock = MockBackend::new(1280, 720);
    mock.release_results
        .push_back(Err(err("cleanup_error", "release boom")));
    mock.release_results.push_back(Ok(())); // a retry, if attempted, succeeds
    let mut h = harness_with(mock);
    let session = open_session(&mut h.runtime);

    let c1 = call(
        &mut h.runtime,
        "computer_close",
        serde_json::json!({"session_id": session}),
    );
    let cleanup1 = c1
        .data
        .get("cleanup_outcome")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    assert!(
        c1.is_error || cleanup1 == "failed" || cleanup1 == "unknown",
        "first close must surface the cleanup failure: {:?}",
        c1.data
    );

    let c2 = call(
        &mut h.runtime,
        "computer_close",
        serde_json::json!({"session_id": session}),
    );
    let releases = h.mock.lock().unwrap().release_count();
    if releases == 1 {
        // No retry: the sticky recorded outcome must remain a failure.
        let cleanup2 = c2
            .data
            .get("cleanup_outcome")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        assert!(
            c2.is_error || cleanup2 == "failed" || cleanup2 == "unknown",
            "sticky cleanup failure must persist without a retry, got: {:?}",
            c2.data
        );
    }
}
