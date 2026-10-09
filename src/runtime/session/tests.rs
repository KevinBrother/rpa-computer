//! Session-level tests with the fake backend: image size honesty, coordinate
//! mapping, zero-input validation failures, staleness, partial dispatch,
//! capture-after-dispatch failure, cleanup failure, replay/conflict, cancel.

use super::*;
use crate::backend::InputEvent;
use crate::runtime::testutil::{be, FakeBackend};
use serde_json::json;
use std::sync::atomic::AtomicBool;

fn setup(width: u32, height: u32) -> (Session, FakeBackend, Arc<AtomicBool>) {
    let backend = FakeBackend::new(width, height);
    let geometry = backend.geometry.clone();
    let session = Session::new("s1".into(), geometry, (1366, 768));
    (session, backend, Arc::new(AtomicBool::new(false)))
}

fn observe(
    session: &mut Session,
    backend: &mut FakeBackend,
    cancel: &Arc<AtomicBool>,
) -> Observation {
    capture_observation(session, backend, 0, cancel).expect("observe")
}

fn step(
    session: &mut Session,
    backend: &mut FakeBackend,
    cancel: &Arc<AtomicBool>,
    request_id: &str,
    based_on: &str,
    action: serde_json::Value,
) -> StepRecord {
    let mut ctx = StepContext {
        backend,
        cancel: cancel.clone(),
        config: crate::runtime::execute::ExecutionConfig {
            settle_delay: Duration::from_millis(1),
            ..crate::runtime::execute::ExecutionConfig::production()
        },
    };
    execute_step(session, &mut ctx, request_id, based_on, &action)
}

#[test]
fn observation_metadata_matches_actual_image_bytes() {
    let (mut s, mut b, c) = setup(3008, 1692);
    let obs = observe(&mut s, &mut b, &c);
    // 3008x1692 does not fit 1366x768; the height binds, so the aspect ratio
    // is preserved and the width rounds to the nearest whole pixel (1365,
    // never forced to the 1366 bound, which would distort the image).
    let expected_w = (3008.0f64 * (768.0f64 / 1692.0f64)).round() as u32;
    assert_eq!(obs.meta.height_px, 768);
    assert_eq!(obs.meta.width_px, expected_w);
    assert!(obs.meta.width_px <= 1366);
    let decoded = crate::runtime::image::decode_png(&obs.png).unwrap();
    assert_eq!(decoded.width, obs.meta.width_px);
    assert_eq!(decoded.height, obs.meta.height_px);
    assert_eq!(obs.meta.input_sequence, 0);
}

#[test]
fn coordinates_map_through_capture_dimensions() {
    // Capture 1600x900 bound by 1366x768: the height binds, so the stored
    // image is downscaled (1365x768) and every image pixel maps
    // proportionally into the 1600x900 native input space.
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    assert_eq!(obs.meta.width_px, 1365);
    assert_eq!(obs.meta.height_px, 768);
    // The center of the image must map to the center of the native surface,
    // proving the map scales by capture dimensions, not by image bounds.
    let center = [obs.meta.width_px as i32 / 2, obs.meta.height_px as i32 / 2];
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs.meta.observation_id,
        json!({"kind": "click", "position": center}),
    );
    assert_eq!(rec.input_outcome, InputOutcome::Dispatched);
    let events = b.events();
    match events[0] {
        InputEvent::Move { x, y } => {
            let (ex, ey) = obs.map.to_native(center);
            assert_eq!((*x, *y), (ex, ey));
            assert!((x - 800).abs() <= 1);
            assert!((y - 450).abs() <= 1);
        }
        other => panic!("expected move first, got {other:?}"),
    }
}

#[test]
fn coordinates_map_identity_when_image_not_downscaled() {
    // A capture that already fits the bounds is stored unscaled, so image
    // pixels map 1:1 onto native coordinates, offset by the input origin.
    let mut backend = FakeBackend::new(800, 600);
    let mut geometry = backend.geometry.clone();
    geometry.input_size = (800, 600);
    geometry.input_origin = (10, 20);
    backend.set_geometry(geometry.clone());
    let mut s = Session::new("s-id".into(), geometry, (1366, 768));
    let mut b = backend;
    let c = Arc::new(AtomicBool::new(false));
    let obs = capture_observation(&mut s, &mut b, 0, &c).expect("observe");
    assert_eq!(obs.meta.width_px, 800);
    assert_eq!(obs.meta.height_px, 600);
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs.meta.observation_id,
        json!({"kind": "click", "position": [400, 300]}),
    );
    assert_eq!(rec.input_outcome, InputOutcome::Dispatched);
    let events = b.events();
    match events[0] {
        InputEvent::Move { x, y } => assert_eq!((*x, *y), (410, 320)),
        other => panic!("expected move first, got {other:?}"),
    }
}

#[test]
fn invalid_chord_injects_zero_events() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    for bad in [
        json!({"kind": "key_chord", "modifiers": ["ctrl"], "key": "notakey"}),
        json!({"kind": "key_chord", "modifiers": ["enter"], "key": "a"}),
        json!({"kind": "key_chord", "modifiers": [], "key": "a"}),
        json!({"kind": "key_hold", "key": "f13", "duration_ms": 10}),
        json!({"kind": "click", "position": [-1, 0]}),
        json!({"kind": "click", "position": [1600, 0]}),
        json!({"kind": "text_input", "text": "x".repeat(4097)}),
        // Per-scalar planning regression: the whole payload must be
        // rejected at parse time, so no scalar before the control char is
        // ever injected.
        json!({"kind": "text_input", "text": "abc\0"}),
        json!({"kind": "text_input", "text": "ok\u{1}later"}),
        json!({"kind": "text_input", "text": "bell\u{7}"}),
        json!({"kind": "text_input", "text": "a\u{7f}b"}),
    ] {
        let before = b.events().len();
        let rec = step(
            &mut s,
            &mut b,
            &c,
            &format!("bad-{before}"),
            &obs.meta.observation_id,
            bad,
        );
        assert_eq!(rec.input_outcome, InputOutcome::NotStarted);
        assert!(rec.error.is_some());
        assert_eq!(b.events().len(), before, "no input may be injected");
        assert_eq!(b.release_all_calls(), 0, "no cleanup needed without input");
    }
    // And the session is still usable afterwards.
    assert_eq!(s.input_sequence, 0);
}

#[test]
fn stale_observation_rejected_after_successful_step() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs.meta.observation_id,
        json!({"kind": "move", "position": [1, 1]}),
    );
    assert_eq!(rec.input_outcome, InputOutcome::Dispatched);
    // The old observation_id is now stale twice over: superseded by the
    // step's own post-observation and by the bumped input_sequence.
    let rec2 = step(
        &mut s,
        &mut b,
        &c,
        "r2",
        &obs.meta.observation_id,
        json!({"kind": "move", "position": [2, 2]}),
    );
    assert_eq!(rec2.input_outcome, InputOutcome::NotStarted);
    assert_eq!(rec2.error.as_ref().unwrap().code, codes::STALE_OBSERVATION);
}

#[test]
fn reobserve_without_input_keeps_prior_observation_valid_basis() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs1 = observe(&mut s, &mut b, &c);
    let obs2 = observe(&mut s, &mut b, &c);
    assert_ne!(obs1.meta.observation_id, obs2.meta.observation_id);
    assert_eq!(obs1.meta.input_sequence, obs2.meta.input_sequence);
    // A read-only re-observe does not invalidate anything: the immediately
    // previous observation remains a truthful basis for input.
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs1.meta.observation_id,
        json!({"kind": "click", "position": [100, 100]}),
    );
    assert_eq!(rec.input_outcome, InputOutcome::Dispatched);
    assert!(rec.error.is_none());
    // An observation that is neither current nor previous is still rejected.
    let rec2 = step(
        &mut s,
        &mut b,
        &c,
        "r2",
        &obs2.meta.observation_id,
        json!({"kind": "click", "position": [100, 100]}),
    );
    assert_eq!(rec2.input_outcome, InputOutcome::NotStarted);
    assert_eq!(rec2.error.as_ref().unwrap().code, codes::STALE_OBSERVATION);
}

#[test]
fn partial_input_reported_and_cleaned_up() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    // Fail the second inject (the button press after the move).
    b.fail_inject_at.push_back((1, be("input_error", "boom")));
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs.meta.observation_id,
        json!({"kind": "click", "position": [100, 100]}),
    );
    assert_eq!(rec.input_outcome, InputOutcome::Partial);
    // Move (index 0) landed; the press (index 1) failed and its planned
    // release was correctly NOT injected — the press never happened, so
    // releasing could unlock UI state the user owns. release_all covers the
    // possible side effects of the failed press instead.
    assert_eq!(rec.events_completed, vec![0]);
    assert_eq!(rec.events_completed.len(), 1);
    assert!(rec.events_total >= 3); // move + press + release
    assert_eq!(rec.cleanup_outcome, CleanupOutcome::Released);
    assert_eq!(b.release_all_calls(), 1);
    // No phantom release event was dispatched for the never-pressed button.
    use crate::backend::Direction;
    assert!(!b.events().iter().any(|e| matches!(
        e,
        InputEvent::Button {
            direction: Direction::Release,
            ..
        }
    )));
    assert_eq!(rec.error.as_ref().unwrap().code, "input_error");
    // Observation still attempted and available (fake capture works).
    assert_eq!(rec.observation_outcome, ObservationOutcome::Available);
    // Even though dispatch failed, input sequence advanced: old obs stale.
    assert_eq!(rec.observation.as_ref().unwrap().input_sequence, 1);
}

#[test]
fn capture_failure_after_dispatch_keeps_dispatched() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    b.fail_capture = Some(be("capture_error", "screen gone"));
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs.meta.observation_id,
        json!({"kind": "move", "position": [10, 10]}),
    );
    assert_eq!(rec.input_outcome, InputOutcome::Dispatched);
    assert_eq!(rec.observation_outcome, ObservationOutcome::Failed);
    assert_eq!(rec.error.as_ref().unwrap().code, "capture_error");
    assert!(rec.observation.is_none());
}

#[test]
fn cleanup_failure_faults_session() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    b.fail_inject_at.push_back((0, be("input_error", "nope")));
    b.fail_release_all = Some(be("cleanup_error", "release failed"));
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs.meta.observation_id,
        json!({"kind": "click", "position": [1, 1]}),
    );
    assert_eq!(rec.cleanup_outcome, CleanupOutcome::Failed);
    assert_eq!(s.state, SessionState::Faulted);
    // Faulted sessions refuse further steps.
    let rec2 = step(
        &mut s,
        &mut b,
        &c,
        "r2",
        &obs.meta.observation_id,
        json!({"kind": "move", "position": [1, 1]}),
    );
    assert_eq!(rec2.input_outcome, InputOutcome::NotStarted);
    assert_eq!(rec2.error.as_ref().unwrap().code, codes::SESSION_STATE);
}

#[test]
fn replay_same_request_returns_prior_result_without_reinput() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    let action = json!({"kind": "click", "position": [50, 50]});
    let r1 = step(
        &mut s,
        &mut b,
        &c,
        "req-9",
        &obs.meta.observation_id,
        action.clone(),
    );
    assert_eq!(r1.input_outcome, InputOutcome::Dispatched);
    let n_events = b.events().len();

    // Replay with the identical body: stored result, no new input — even
    // though the original observation is now stale (dedup wins over stale).
    let r2 = step(
        &mut s,
        &mut b,
        &c,
        "req-9",
        &obs.meta.observation_id,
        action,
    );
    assert_eq!(b.events().len(), n_events, "replay must not re-inject");
    assert_eq!(r2.input_outcome, r1.input_outcome);
    assert_eq!(r2.request_id, "req-9");
}

#[test]
fn conflicting_request_body_rejected_without_input() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    let r1 = step(
        &mut s,
        &mut b,
        &c,
        "req-7",
        &obs.meta.observation_id,
        json!({"kind": "click", "position": [50, 50]}),
    );
    assert_eq!(r1.input_outcome, InputOutcome::Dispatched);
    let n_events = b.events().len();
    let current = s
        .current_observation
        .as_ref()
        .unwrap()
        .meta
        .observation_id
        .clone();
    let r2 = step(
        &mut s,
        &mut b,
        &c,
        "req-7",
        &current,
        json!({"kind": "click", "position": [60, 60]}),
    );
    assert_eq!(r2.input_outcome, InputOutcome::NotStarted);
    assert_eq!(r2.error.as_ref().unwrap().code, codes::REQUEST_CONFLICT);
    assert_eq!(b.events().len(), n_events);
}

#[test]
fn cancellation_before_first_input_is_not_started() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    c.store(true, Ordering::SeqCst);
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs.meta.observation_id,
        json!({"kind": "click", "position": [5, 5]}),
    );
    assert_eq!(rec.input_outcome, InputOutcome::NotStarted);
    assert!(rec.cancelled);
    assert_eq!(b.events().len(), 0);
    // No input dispatched, but cleanup is still not needed.
    assert_eq!(rec.cleanup_outcome, CleanupOutcome::NotNeeded);
}

#[test]
fn cancellation_during_hold_stops_promptly() {
    use crate::runtime::testutil::race::{RaceGate, RaceTask};
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    let gate = Arc::new(RaceGate::default());
    b.after_key_press_gate = Some(gate.clone());
    let cancel = c.clone();
    // The backend has validated and recorded the real mock press and marked
    // shift held before this helper can send cancel. PNG/preflight latency is
    // deliberately NOT part of the cancellation-response measurement.
    let stopper = RaceTask::spawn(gate, c.clone(), move || {
        let sent = Instant::now();
        cancel.store(true, Ordering::SeqCst);
        sent
    });
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "r1",
        &obs.meta.observation_id,
        json!({"kind": "key_hold", "key": "shift", "duration_ms": 5000}),
    );
    let finished = Instant::now();
    let cancel_sent = stopper.finish();
    assert!(rec.cancelled);
    assert_eq!(rec.input_outcome, InputOutcome::Partial); // press was injected
    assert!(
        finished.duration_since(cancel_sent) < Duration::from_secs(2),
        "cancel must be prompt after delivery, independently of setup/PNG work"
    );
    assert_eq!(rec.cleanup_outcome, CleanupOutcome::Released);
    assert!(
        matches!(b.events().first(), Some(InputEvent::Key { key, direction: crate::backend::Direction::Press }) if key == "shift")
    );
    assert_eq!(
        b.events()
            .iter()
            .filter(|e| matches!(
                e,
                InputEvent::Key {
                    direction: crate::backend::Direction::Press,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(b.release_all_calls(), 1);
    assert!(
        b.held_keys.is_empty(),
        "cancel cleanup must leave no held key"
    );
}

#[test]
fn geometry_change_faults_on_observe() {
    let (mut s, mut b, c) = setup(1600, 900);
    let _ = observe(&mut s, &mut b, &c);
    b.pending_version = Some("v2".into());
    let err = capture_observation(&mut s, &mut b, 0, &c).unwrap_err();
    assert_eq!(err.code, codes::GEOMETRY_CHANGED);
    assert_eq!(s.state, SessionState::Faulted);
}

#[test]
fn get_step_results_and_missing_requests() {
    let (mut s, mut b, c) = setup(1600, 900);
    let obs = observe(&mut s, &mut b, &c);
    let rec = step(
        &mut s,
        &mut b,
        &c,
        "rq",
        &obs.meta.observation_id,
        json!({"kind": "move", "position": [3, 3]}),
    );
    assert_eq!(rec.input_outcome, InputOutcome::Dispatched);
    let data = s.get_result("rq").unwrap();
    assert_eq!(data["input_outcome"], "dispatched");
    assert!(s.result_image("rq").is_some());
    let err = s.get_result("never-seen").unwrap_err();
    assert_eq!(err.code, codes::REQUEST_NOT_FOUND);
}

#[test]
fn result_metadata_retained_for_all_registered_requests_images_trimmed() {
    let (mut s, mut b, c) = setup(1600, 900);
    let mut obs = observe(&mut s, &mut b, &c);
    let total = MAX_RESULT_IMAGES_PER_SESSION + 20;
    for i in 0..total {
        let rec = step(
            &mut s,
            &mut b,
            &c,
            &format!("r{i}"),
            &obs.meta.observation_id,
            json!({"kind": "move", "position": [1, 1]}),
        );
        assert_eq!(rec.input_outcome, InputOutcome::Dispatched);
        obs = Observation {
            meta: rec.observation.clone().unwrap(),
            captured_at: Instant::now(),
            png: Vec::new(),
            map: obs.map,
            regions: obs.regions.clone(),
        };
    }
    // Metadata for ALL requests is retained — none becomes falsely unknown.
    for i in 0..total {
        let data = s.get_result(&format!("r{i}")).unwrap();
        assert_eq!(data["input_outcome"], "dispatched");
    }
    // Only recent images are retained; old results report it honestly.
    let first = s.get_result("r0").unwrap();
    assert_eq!(first["observation"]["image_available"], false);
    assert!(s.result_image("r0").is_none());
    let last = s.get_result(&format!("r{}", total - 1)).unwrap();
    assert_eq!(last["observation"]["image_available"], true);
    // Replaying an old request still returns its stored result, no re-input.
    let before = b.events().len();
    let replay = step(
        &mut s,
        &mut b,
        &c,
        "r0",
        "s1-obs-1",
        json!({"kind": "move", "position": [1, 1]}),
    );
    assert_eq!(replay.input_outcome, InputOutcome::Dispatched);
    assert_eq!(
        b.events().len(),
        before,
        "replay must never re-inject input"
    );
}

#[test]
fn image_budget_accounts_all_retained_pngs_once() {
    let (mut s, mut b, c) = setup(1600, 900);
    let png_len = b.png.len() as u64;
    // Budget for roughly 3 images: the cache must count every retained PNG
    // (current, previous, and result-referenced) exactly once.
    s.set_image_byte_budget(png_len * 3 + png_len / 2);
    let mut obs = observe(&mut s, &mut b, &c);
    for i in 0..6 {
        let rec = step(
            &mut s,
            &mut b,
            &c,
            &format!("m{i}"),
            &obs.meta.observation_id,
            json!({"kind": "move", "position": [1, 1]}),
        );
        assert_eq!(rec.input_outcome, InputOutcome::Dispatched);
        obs = Observation {
            meta: rec.observation.clone().unwrap(),
            captured_at: Instant::now(),
            png: Vec::new(),
            map: obs.map,
            regions: obs.regions.clone(),
        };
        assert!(
            s.image_bytes() <= png_len * 3 + png_len / 2,
            "image cache exceeded budget: {} bytes",
            s.image_bytes()
        );
    }
    // Over-budget eviction dropped the oldest images, never the current one,
    // and never silently re-added the previous one (the reviewed bug).
    assert!(s.result_image("m0").is_none());
    assert!(s.result_image("m5").is_some());
}

#[test]
fn single_image_exceeding_budget_errors_without_state_corruption() {
    let (mut s, mut b, c) = setup(1600, 900);
    s.set_image_byte_budget(8);
    let err = capture_observation(&mut s, &mut b, 0, &c).unwrap_err();
    assert_eq!(err.code, codes::RESOURCE_LIMIT);
}

mod multiclick;
