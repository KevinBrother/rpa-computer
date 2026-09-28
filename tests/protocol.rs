//! Public protocol surface tests.
//!
//! These exercise the crate exactly the way the MCP transport and external
//! QA scripts do: only `rpa_computer::runtime` and the public `backend` trait
//! types, through `tool_definitions` and `Runtime::call`. No internal
//! modules are touched here.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rpa_computer::backend::{Backend, BackendError, Capture, Geometry, InputEvent};
use rpa_computer::runtime::{tool_definitions, Runtime};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// A fake backend implementing the public trait — no real desktop involved.
// ---------------------------------------------------------------------------

struct Fake {
    geometry: Geometry,
    png: Vec<u8>,
    injected: Vec<InputEvent>,
    release_all_calls: usize,
    fail_release_all: Option<BackendError>,
    /// Shared event counter so tests can assert how much input was injected
    /// after the backend has been moved into the Runtime.
    injected_count: Arc<Mutex<usize>>,
}

impl Fake {
    fn new(width: u32, height: u32) -> (Self, Arc<Mutex<usize>>) {
        let counter = Arc::new(Mutex::new(0usize));
        (
            Self {
                geometry: Geometry {
                    surface_id: "primary".into(),
                    input_origin: (0, 0),
                    input_size: (1600, 900),
                    version: "v1".into(),
                },
                png: png_of(width, height),
                injected: Vec::new(),
                release_all_calls: 0,
                fail_release_all: None,
                injected_count: counter.clone(),
            },
            counter,
        )
    }
}

impl Backend for Fake {
    fn platform(&self) -> &'static str {
        "fake"
    }

    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        Ok(self.geometry.clone())
    }

    fn capture(&mut self) -> Result<Capture, BackendError> {
        Ok(Capture {
            png: self.png.clone(),
            width: png_dimensions(&self.png).0,
            height: png_dimensions(&self.png).1,
            geometry: self.geometry.clone(),
        })
    }

    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        self.injected.push(event.clone());
        *self.injected_count.lock().unwrap() += 1;
        Ok(())
    }

    fn release_all(&mut self) -> Result<(), BackendError> {
        self.release_all_calls += 1;
        match &self.fail_release_all {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
}

/// Encode a solid PNG without depending on crate-internal helpers.
fn png_of(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([1, 2, 3, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

/// Read the actual dimensions out of PNG bytes (IHDR), so assertions compare
/// reported metadata against the real returned image, not against a claim.
fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert!(png.len() >= 24, "png too short");
    let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    (w, h)
}

fn runtime_with(pair: (Fake, Arc<Mutex<usize>>)) -> (Runtime, Arc<AtomicBool>, Arc<Mutex<usize>>) {
    let (fake, counter) = pair;
    let cancel = Arc::new(AtomicBool::new(false));
    (
        Runtime::new(Box::new(fake), cancel.clone()),
        cancel,
        counter,
    )
}

fn call(rt: &mut Runtime, name: &str, args: Value) -> rpa_computer::runtime::Reply {
    rt.call(name, args)
}

fn error_code(reply: &rpa_computer::runtime::Reply) -> String {
    assert!(reply.is_error, "expected error reply, got {:?}", reply.data);
    reply.data["error"]["code"].as_str().unwrap().to_string()
}

/// Open a session and return its id.
fn open_session(rt: &mut Runtime) -> String {
    let reply = call(rt, "computer_open", json!({}));
    assert!(!reply.is_error, "open failed: {:?}", reply.data);
    reply.data["session_id"].as_str().unwrap().to_string()
}

/// Observe and return the observation id.
fn observe(rt: &mut Runtime, session_id: &str) -> (String, Vec<u8>) {
    let reply = call(rt, "computer_observe", json!({"session_id": session_id}));
    assert!(!reply.is_error, "observe failed: {:?}", reply.data);
    let id = reply.data["observation_id"].as_str().unwrap().to_string();
    (id, reply.image_png.expect("observe must return an image"))
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

#[test]
fn exposes_exactly_the_contract_tools() {
    let defs = tool_definitions();
    let names: Vec<&str> = defs.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "computer_describe",
            "computer_open",
            "computer_observe",
            "computer_step",
            "computer_get_step",
            "computer_pause",
            "computer_resume",
            "computer_close",
        ]
    );
    for def in tool_definitions() {
        assert!(!def.description.is_empty());
        assert_eq!(def.input_schema["type"], "object");
    }
}

#[test]
fn unknown_tool_is_structured_error() {
    let (mut rt, _c, _count) = runtime_with(Fake::new(800, 600));
    let reply = call(&mut rt, "computer_teleport", json!({}));
    assert!(reply.is_error);
    assert_eq!(error_code(&reply), "unknown_tool");
}

// ---------------------------------------------------------------------------
// Honest observation metadata
// ---------------------------------------------------------------------------

#[test]
fn observation_metadata_matches_returned_png() {
    let (mut rt, _c, _count) = runtime_with(Fake::new(1600, 900));
    let session = open_session(&mut rt);
    let (_obs, png) = observe(&mut rt, &session);
    let reply = call(&mut rt, "computer_observe", json!({"session_id": session}));
    let (actual_w, actual_h) = png_dimensions(&png);
    assert_eq!(reply.data["image"]["width_px"], json!(actual_w));
    assert_eq!(reply.data["image"]["height_px"], json!(actual_h));
    assert_eq!(reply.data["image"]["mime_type"], "image/png");
    assert!(reply.data["input_sequence"].is_u64());
    assert!(reply.data["geometry_version"].is_string());
}

// ---------------------------------------------------------------------------
// Input validation happens before any dispatch
// ---------------------------------------------------------------------------

#[test]
fn invalid_actions_never_dispatch_and_keep_session_usable() {
    let (mut rt, _c, count) = runtime_with(Fake::new(800, 600));
    let session = open_session(&mut rt);
    let (obs, _png) = observe(&mut rt, &session);

    let bad_actions = vec![
        json!({"kind": "key_chord", "modifiers": ["ctrl"], "key": "definitely_not_a_key"}),
        json!({"kind": "key_chord", "modifiers": ["ctrl"], "key": "shift"}), // modifier as key
        json!({"kind": "key_chord", "modifiers": ["a"], "key": "b"}), // non-modifier as modifier
        json!({"kind": "click", "position": [10, 10], "button": "hover"}),
        json!({"kind": "text_input", "text": "x".repeat(5000)}), // over 4096 chars
        json!({"kind": "scroll", "position": [1, 1], "delta_x": 0, "delta_y": 1, "unit": "pixels"}),
        json!({"kind": "drag", "path": [[0, 0]], "button": "left"}), // only one point
        json!({"kind": "nope"}),
        json!({"kind": "click", "position": [9999, 9999]}), // outside the image
    ];

    for (i, action) in bad_actions.iter().enumerate() {
        let reply = call(
            &mut rt,
            "computer_step",
            json!({
                "session_id": session,
                "request_id": format!("bad-{i}"),
                "based_on": obs,
                "action": action,
            }),
        );
        assert_eq!(
            reply.data["input_outcome"], "not_started",
            "action {i} dispatched input"
        );
        assert_eq!(reply.data["events_completed"], json!([]));
        assert!(
            reply.data["error"].is_object(),
            "action {i} had no error: {:?}",
            reply.data
        );
    }

    // The runtime never touched the backend for any of them.
    assert_eq!(
        *count.lock().unwrap(),
        0,
        "invalid actions must never inject input"
    );

    // Session still usable: a valid step succeeds afterwards.
    let reply = call(
        &mut rt,
        "computer_step",
        json!({
            "session_id": session,
            "request_id": "good-1",
            "based_on": obs,
            "action": {"kind": "click", "position": [100, 100]},
        }),
    );
    assert_eq!(reply.data["input_outcome"], "dispatched");
}

// ---------------------------------------------------------------------------
// Stale-basis rejection
// ---------------------------------------------------------------------------

#[test]
fn stale_based_on_rejected_without_input() {
    let (mut rt, _c, _count) = runtime_with(Fake::new(800, 600));
    let session = open_session(&mut rt);
    let (obs1, _p1) = observe(&mut rt, &session);

    // A step based on obs1 succeeds and produces a new observation.
    let reply = call(
        &mut rt,
        "computer_step",
        json!({
            "session_id": session,
            "request_id": "r1",
            "based_on": obs1,
            "action": {"kind": "move", "position": [50, 50]},
        }),
    );
    assert_eq!(reply.data["input_outcome"], "dispatched");

    // A second step still based on the old observation is stale.
    let reply = call(
        &mut rt,
        "computer_step",
        json!({
            "session_id": session,
            "request_id": "r2",
            "based_on": obs1,
            "action": {"kind": "move", "position": [60, 60]},
        }),
    );
    assert_eq!(reply.data["input_outcome"], "not_started");
    assert_eq!(reply.data["error"]["code"], "stale_observation");
}

// ---------------------------------------------------------------------------
// Idempotency: replay vs conflict
// ---------------------------------------------------------------------------

#[test]
fn request_id_replay_is_idempotent_conflict_is_error() {
    let (mut rt, _c, _count) = runtime_with(Fake::new(800, 600));
    let session = open_session(&mut rt);
    let (obs, _png) = observe(&mut rt, &session);

    let args = json!({
        "session_id": session,
        "request_id": "req-1",
        "based_on": obs,
        "action": {"kind": "click", "position": [100, 100]},
    });
    let first = call(&mut rt, "computer_step", args.clone());
    assert_eq!(first.data["input_outcome"], "dispatched");

    // Same id + same body: replays the stored result, does NOT re-dispatch.
    let replay = call(&mut rt, "computer_step", args.clone());
    assert!(!replay.is_error);
    assert_eq!(replay.data["input_outcome"], "dispatched");
    assert_eq!(replay.data["duration_ms"], first.data["duration_ms"]);

    // Same id + different body: conflict error, no input.
    let conflict = call(
        &mut rt,
        "computer_step",
        json!({
            "session_id": session,
            "request_id": "req-1",
            "based_on": obs,
            "action": {"kind": "click", "position": [200, 200]},
        }),
    );
    assert_eq!(conflict.data["input_outcome"], "not_started");
    assert_eq!(conflict.data["error"]["code"], "request_conflict");
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

#[test]
fn cancelled_hold_finishes_quickly_and_reports_partial() {
    let (mut rt, cancel, _count) = runtime_with(Fake::new(800, 600));
    let session = open_session(&mut rt);
    let (obs, _png) = observe(&mut rt, &session);

    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let started = std::time::Instant::now();
    let reply = call(
        &mut rt,
        "computer_step",
        json!({
            "session_id": session,
            "request_id": "hold-1",
            "based_on": obs,
            "action": {"kind": "key_hold", "key": "shift", "duration_ms": 5000},
        }),
    );
    let elapsed = started.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "hold did not honor cancellation: {elapsed:?}"
    );
    assert_eq!(reply.data["cancelled"], true);
    // The press went out, the release didn't: partial input, then cleanup.
    assert_eq!(reply.data["input_outcome"], "partial");
    assert_eq!(reply.data["cleanup_outcome"], "released");
}

#[test]
fn pause_blocks_steps_and_resume_clears() {
    let (mut rt, cancel, _count) = runtime_with(Fake::new(800, 600));
    let session = open_session(&mut rt);
    let (obs, _png) = observe(&mut rt, &session);

    let reply = call(&mut rt, "computer_pause", json!({"session_id": session}));
    assert!(!reply.is_error);
    assert_eq!(reply.data["state"], "paused");
    assert!(
        cancel.load(std::sync::atomic::Ordering::SeqCst),
        "pause must request stop"
    );

    let reply = call(
        &mut rt,
        "computer_step",
        json!({
            "session_id": session,
            "request_id": "p1",
            "based_on": obs,
            "action": {"kind": "move", "position": [10, 10]},
        }),
    );
    assert!(reply.is_error);
    assert_eq!(error_code(&reply), "cancelled");

    let reply = call(&mut rt, "computer_resume", json!({"session_id": session}));
    assert!(!reply.is_error);
    assert_eq!(reply.data["state"], "ready");
    assert!(
        !cancel.load(std::sync::atomic::Ordering::SeqCst),
        "resume must clear stop"
    );
}

// ---------------------------------------------------------------------------
// Cleanup failure honesty
// ---------------------------------------------------------------------------

#[test]
fn close_with_cleanup_failure_is_error_and_faulted() {
    let (mut fake, counter) = Fake::new(800, 600);
    fake.fail_release_all = Some(BackendError::new("input_failed", "release failed"));
    let (mut rt, _c, _count) = runtime_with((fake, counter));
    let session = open_session(&mut rt);

    let reply = call(&mut rt, "computer_close", json!({"session_id": session}));
    assert!(
        reply.is_error,
        "cleanup failure must not be reported as a clean close"
    );
    assert_eq!(reply.data["cleanup_outcome"], "failed");
    assert_eq!(reply.data["state"], "faulted");
}

#[test]
fn shutdown_without_session_is_harmless() {
    let (mut rt, _c, _count) = runtime_with(Fake::new(800, 600));
    let r1 = rt.shutdown();
    let r2 = rt.shutdown();
    assert!(!r1.is_error && !r2.is_error);
}

// ---------------------------------------------------------------------------
// Key chord dispatch shape
// ---------------------------------------------------------------------------

#[test]
fn chord_dispatches_modifier_key_sequence() {
    let (mut rt, _c, _count) = runtime_with(Fake::new(800, 600));
    let session = open_session(&mut rt);
    let (obs, _png) = observe(&mut rt, &session);

    let reply = call(
        &mut rt,
        "computer_step",
        json!({
            "session_id": session,
            "request_id": "chord-1",
            "based_on": obs,
            "action": {"kind": "key_chord", "modifiers": ["ctrl", "shift"], "key": "a"},
        }),
    );
    assert_eq!(reply.data["input_outcome"], "dispatched");
    // 2 modifier presses + key press + key release + 2 modifier releases.
    assert_eq!(reply.data["events_total"], 6);
    assert_eq!(reply.data["events_completed"], json!([0, 1, 2, 3, 4, 5]));
}

#[test]
fn get_step_returns_stored_result_and_missing_is_error() {
    let (mut rt, _c, _count) = runtime_with(Fake::new(800, 600));
    let session = open_session(&mut rt);
    let (obs, _png) = observe(&mut rt, &session);

    let reply = call(
        &mut rt,
        "computer_get_step",
        json!({"session_id": session, "request_id": "never-sent"}),
    );
    assert!(reply.is_error);
    assert_eq!(error_code(&reply), "request_not_found");

    let step = call(
        &mut rt,
        "computer_step",
        json!({
            "session_id": session,
            "request_id": "g1",
            "based_on": obs,
            "action": {"kind": "move", "position": [10, 10]},
        }),
    );
    let fetched = call(
        &mut rt,
        "computer_get_step",
        json!({"session_id": session, "request_id": "g1"}),
    );
    assert!(!fetched.is_error);
    assert_eq!(fetched.data["input_outcome"], step.data["input_outcome"]);
}
