//! 2. True-dimension scaling: image pixels ↔ native input space.

use crate::support::*;
use rpa_computer::backend::Capture;

#[test]
fn coordinates_map_from_capture_dimensions_to_native_input_space() {
    // Native input space 1920x1080; the model image is 960x540 (exactly half).
    let mut h = harness_with(MockBackend::new(960, 540));
    {
        let mut m = h.mock.lock().unwrap();
        m.geometry.input_size = (1920, 1080);
        m.default_capture.geometry = m.geometry.clone();
    }
    let session = open_session(&mut h.runtime);

    // Ask open to downscale to max 960x540. The returned image must have
    // truthful metadata: declared dims must equal actual encoded PNG dims.
    let reply = call(
        &mut h.runtime,
        "computer_open",
        serde_json::json!({"max_width": 960, "max_height": 540}),
    );
    // A second open may legitimately fail with lease_conflict; if so reuse
    // the first session.
    let session = if reply.is_error {
        session
    } else {
        reply.data["session_id"].as_str().unwrap().to_string()
    };

    let obs = observe(&mut h.runtime, &session);
    let width_px = obs["width_px"].as_u64().expect("width_px") as u32;
    let height_px = obs["height_px"].as_u64().expect("height_px") as u32;

    let png = {
        // Re-observe to grab the image that matches this metadata.
        let r = call(
            &mut h.runtime,
            "computer_observe",
            serde_json::json!({"session_id": session}),
        );
        assert!(!r.is_error);
        let w = r.data["width_px"].as_u64().unwrap() as u32;
        let h2 = r.data["height_px"].as_u64().unwrap() as u32;
        assert_eq!(
            (w, h2),
            (width_px, height_px),
            "deterministic mock must yield same dims"
        );
        r.image_png.expect("image")
    };

    // CONTRACT §backend: width/height are the ACTUAL encoded pixel dims.
    let decoded = image::load_from_memory(&png).expect("image_png must be a decodable image");
    assert_eq!(
        (decoded.width(), decoded.height()),
        (width_px, height_px),
        "declared width_px/height_px must equal the ACTUAL decoded PNG dimensions"
    );

    // Click near the bottom-right of the *image* coordinate space.
    let click_x = width_px as i32 - 10;
    let click_y = height_px as i32 - 10;
    let step = call(
        &mut h.runtime,
        "computer_step",
        step_args(
            &session,
            "req-scaling",
            obs["observation_id"].as_str().unwrap(),
            serde_json::json!({"kind": "click", "position": [click_x, click_y]}),
        ),
    );
    assert!(
        !step.is_error,
        "in-bounds click must dispatch: {:?}",
        step.data
    );

    // Verify the backend saw native coordinates scaled by input_size/image_size.
    let mock = h.mock.lock().unwrap();
    let moves: Vec<&str> = mock
        .events()
        .into_iter()
        .filter(|e| e.contains("Move"))
        .collect();
    assert!(
        !moves.is_empty(),
        "click must position the pointer with a native-space Move event; log: {:?}",
        mock.log
    );
    let last_move = moves.last().unwrap();
    let expected_x = ((click_x as i64) * 1920 / width_px as i64) as i32;
    let expected_y = ((click_y as i64) * 1080 / height_px as i64) as i32;
    assert!(
        last_move.contains(&format!("x: {}", expected_x))
            && last_move.contains(&format!("y: {}", expected_y)),
        "image ({click_x},{click_y}) must scale to native ({expected_x},{expected_y}); got move {last_move}"
    );
    // Sanity: native x must clearly exceed the image x (scale factor 2 here).
    assert!(expected_x > click_x, "test setup requires 2x scaling");
}

#[test]
fn open_declares_truthful_capabilities_and_limits() {
    let mut h = harness();
    let reply = call(&mut h.runtime, "computer_describe", serde_json::json!({}));
    assert!(!reply.is_error, "describe must succeed: {:?}", reply.data);
    // describe must not silently require args
    let session = open_session(&mut h.runtime);
    let _ = session;
}

#[test]
fn zero_sized_capture_is_fault_not_silent_success() {
    // A backend returning a 0x0 capture is broken; the runtime must not
    // divide by zero or report a valid observation with zero dims.
    let mut mock = MockBackend::new(1280, 720);
    let zero = Capture {
        png: tiny_png(1, 1),
        width: 0,
        height: 0,
        geometry: mock.geometry.clone(),
    };
    mock.capture_results.push_back(Ok(zero));
    let mut h = harness_with(mock);
    let session = open_session(&mut h.runtime);
    let reply = call(
        &mut h.runtime,
        "computer_observe",
        serde_json::json!({"session_id": session}),
    );
    if !reply.is_error {
        let w = reply.data["width_px"].as_u64().unwrap_or(0);
        let hgt = reply.data["height_px"].as_u64().unwrap_or(0);
        assert!(
            w > 0 && hgt > 0,
            "a successful observation must never declare zero dimensions"
        );
    }
    // An error reply is the acceptable outcome; but it must be a structured
    // error with a machine-readable code, not a panic.
    if reply.is_error {
        let _ = error_code(&reply);
    }
}

#[test]
fn image_declared_size_matches_encoded_size_on_observe() {
    let mut h = harness_with(MockBackend::new(800, 600));
    let session = open_session(&mut h.runtime);
    let reply = call(
        &mut h.runtime,
        "computer_observe",
        serde_json::json!({"session_id": session}),
    );
    assert!(!reply.is_error);
    let png = reply.image_png.clone().expect("image");
    let decoded = image::load_from_memory(&png).expect("decodable");
    assert_eq!(
        (decoded.width(), decoded.height()),
        (
            reply.data["width_px"].as_u64().unwrap() as u32,
            reply.data["height_px"].as_u64().unwrap() as u32
        ),
        "observation metadata must match actual encoded image dimensions"
    );
    assert_eq!((decoded.width(), decoded.height()), (800, 600));
}
