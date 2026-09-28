//! 7. Screenshot failure after dispatched input.

use std::sync::{Arc, Mutex};

use rpa_computer::backend::{Backend, BackendError, Capture, Geometry, InputEvent};

use crate::support::*;

/// Backend whose capture works until the first inject, then fails — this
/// models "screenshot error AFTER input was dispatched".
pub struct FailCaptureBackend {
    geometry: Geometry,
    capture: Capture,
    injected: bool,
    pub log: Arc<Mutex<Vec<String>>>,
}

impl FailCaptureBackend {
    pub fn new(width: u32, height: u32) -> Self {
        let geometry = Geometry {
            surface_id: "mock-surface-0".into(),
            input_origin: (0, 0),
            input_size: (1920, 1080),
            version: "geom-v1".into(),
        };
        FailCaptureBackend {
            capture: Capture {
                png: tiny_png(width, height),
                width,
                height,
                geometry: geometry.clone(),
            },
            geometry,
            injected: false,
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Backend for FailCaptureBackend {
    fn platform(&self) -> &'static str {
        "mock"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        Ok(self.geometry.clone())
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        if self.injected {
            Err(err("capture_error", "screen went away after input"))
        } else {
            Ok(clone_capture(&self.capture))
        }
    }
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        self.injected = true;
        self.log.lock().unwrap().push(format!("inject:{:?}", event));
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}

#[test]
fn capture_failure_after_dispatch_preserves_dispatched_outcome() {
    let (mut rt, _cancel) = harness_backend(FailCaptureBackend::new(1280, 720));
    let open = rt.call("computer_open", serde_json::json!({}));
    assert!(!open.is_error, "{:?}", open.data);
    let session = open.data["session_id"].as_str().unwrap().to_string();
    let obs = rt.call(
        "computer_observe",
        serde_json::json!({"session_id": session}),
    );
    assert!(!obs.is_error, "{:?}", obs.data);
    let obs_id = obs.data["observation_id"].as_str().unwrap().to_string();

    let reply = rt.call(
        "computer_step",
        step_args(
            &session,
            "req-cap-fail",
            &obs_id,
            serde_json::json!({"kind": "click", "position": [10, 10]}),
        ),
    );
    let outcome = reply
        .data
        .get("input_outcome")
        .and_then(|o| o.as_str())
        .expect("input_outcome present");
    assert_eq!(
        outcome, "dispatched",
        "input succeeded; screenshot failure must not erase the dispatched outcome: {:?}",
        reply.data
    );
    let obs_outcome = reply
        .data
        .get("observation_outcome")
        .and_then(|o| o.as_str())
        .unwrap_or("?");
    assert!(
        obs_outcome == "failed" || obs_outcome == "skipped",
        "observation_outcome must honestly report the capture failure, got {obs_outcome}"
    );
    assert!(
        reply.image_png.is_none(),
        "no image may be attached when the post-step capture failed"
    );
    if reply.is_error {
        let code = error_code(&reply);
        assert_eq!(code, "capture_error", "expected capture_error, got {code}");
    }

    // get_step must preserve the original recorded outcome afterwards, even
    // though no image was captured.
    let g = rt.call(
        "computer_get_step",
        serde_json::json!({"session_id": session, "request_id": "req-cap-fail"}),
    );
    assert!(
        !g.is_error,
        "get_step must return the recorded result after capture failure: {:?}",
        g.data
    );
    assert_eq!(
        g.data["input_outcome"].as_str().unwrap_or("?"),
        "dispatched",
        "recorded dispatched outcome must survive the capture failure"
    );
}

#[test]
fn capture_failure_after_move_also_preserves_dispatched() {
    let (mut rt, _cancel) = harness_backend(FailCaptureBackend::new(1280, 720));
    let open = rt.call("computer_open", serde_json::json!({}));
    assert!(!open.is_error, "{:?}", open.data);
    let session = open.data["session_id"].as_str().unwrap().to_string();
    let obs = rt.call(
        "computer_observe",
        serde_json::json!({"session_id": session}),
    );
    assert!(!obs.is_error, "{:?}", obs.data);
    let obs_id = obs.data["observation_id"].as_str().unwrap().to_string();

    let reply = rt.call(
        "computer_step",
        step_args(
            &session,
            "req-cap-fail-2",
            &obs_id,
            serde_json::json!({"kind": "move", "position": [50, 50]}),
        ),
    );
    assert_eq!(
        reply.data["input_outcome"].as_str().unwrap_or("?"),
        "dispatched",
        "capture failure must not rewrite dispatched input outcome: {:?}",
        reply.data
    );
}
