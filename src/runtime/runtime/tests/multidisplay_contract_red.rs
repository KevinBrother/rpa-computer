//! Task 1 only: executable contracts missing from the unmodified baseline.
//!
//! Uses the existing in-memory FakeBackend, NOT a Windows desktop backend.
//! One synthetic display is all the old Backend API can express. These tests
//! expose the public metadata/schema gap; they do NOT pretend to test real
//! enumeration, two-monitor composition, DPI, GDI, drag or feedback.
//!
//! Metadata projection used here: displays[], native_unit, topology_generation;
//! observations additionally carry selected_display_ids[] and mapping_regions[]
//! (display_id, image_rect, native_bounds). Generation may be an opaque string
//! or structured object: equality tests enforce stable authority and distinct
//! fresh trackers without prescribing token internals or revision encoding.

use super::{open, Arc, AtomicBool, FakeBackend, Runtime};
use crate::backend::Geometry;
use crate::runtime::{image::decode_png, tools::tool_definitions};
use serde_json::{json, Value};

const DISPLAY_ID: &str = "mock-display-negative-origin";

fn mock_runtime() -> Runtime {
    // Hand-authored backend facts, intentionally unlike FakeBackend's default
    // primary/1600x900/origin0. PNG bytes and native geometry are independent.
    let mut backend = FakeBackend::new(320, 180);
    backend.set_geometry(Geometry {
        surface_id: DISPLAY_ID.into(),
        input_origin: (-640, 120),
        input_size: (640, 360),
        version: "mock-geometry-v1".into(),
    });
    Runtime::new(Box::new(backend), Arc::new(AtomicBool::new(false)))
}

fn describe(rt: &mut Runtime) -> Value {
    let reply = rt.call("computer_describe", json!({}));
    assert!(!reply.is_error, "describe failed: {}", reply.data);
    assert_eq!(reply.data["available"], true);
    assert_eq!(reply.data["platform"], "fake");
    assert!(
        reply.image_png.is_none(),
        "describe must not return a capture"
    );
    reply.data
}

fn native_bounds() -> Value {
    // Never derive expectations from the result or a production mapper.
    json!({"x": -640, "y": 120, "width": 640, "height": 360})
}

fn full_generation(data: &Value) -> &Value {
    let generation = &data["topology_generation"];
    let nonempty_token = generation.as_str().is_some_and(|s| !s.is_empty())
        || generation.as_object().is_some_and(|o| !o.is_empty());
    assert!(
        nonempty_token,
        "missing full topology_generation: require a nonempty opaque token/object, not a bare revision: {data}"
    );
    generation
}

#[test]
fn describe_lists_backend_display_facts_not_static_primary() {
    let data = describe(&mut mock_runtime());
    let displays = data["displays"]
        .as_array()
        .expect("describe must expose actual backend displays, not only action capabilities");
    assert_eq!(displays.len(), 1, "only one synthetic display was supplied");
    assert_eq!(displays[0]["id"], DISPLAY_ID);
    assert_eq!(displays[0]["is_primary"], true);
    assert_eq!(displays[0]["native_bounds"], native_bounds());
    // No invented secondary screens or default primary geometry are allowed.
}

#[test]
fn describe_declares_native_unit_separately_from_image_coordinates() {
    let data = describe(&mut mock_runtime());
    assert_eq!(data["coordinate_space"], "image_pixels_top_left_origin");
    assert!(
        matches!(
            data["native_unit"].as_str(),
            Some("points" | "physical_pixels")
        ),
        "describe must separately declare the backend native unit: {data}"
    );
    // FakeBackend has no OS/platform unit authority in today's API. Do not
    // infer a real Windows physical-pixel claim just from the test target.
}

#[test]
fn describe_full_generation_is_stable_but_distinguishes_fresh_authorities() {
    let mut first = mock_runtime();
    let initial = describe(&mut first);
    let repeated = describe(&mut first);
    assert_eq!(
        full_generation(&initial),
        full_generation(&repeated),
        "unchanged facts in the same authority must retain generation"
    );
    // Identical geometry/version and initial revision must NOT alias a fresh
    // runtime/tracker. Comparing only revision or Geometry.version is unsafe.
    let independent = describe(&mut mock_runtime());
    assert_ne!(
        full_generation(&initial),
        full_generation(&independent),
        "full generation must include authority identity, not just revision/geometry"
    );
}

#[test]
fn explicit_primary_is_accepted_by_runtime_and_advertised_open_schema() {
    let mut rt = mock_runtime();
    let reply = rt.call("computer_open", json!({"display": {"kind": "primary"}}));
    assert!(
        !reply.is_error,
        "explicit primary must be legal: {}",
        reply.data
    );
    assert_eq!(reply.data["surface_id"], DISPLAY_ID);
    let sid = reply.data["session_id"].as_str().expect("session ID");
    let closed = rt.call("computer_close", json!({"session_id": sid}));
    assert!(
        !closed.is_error,
        "mock session cleanup failed: {}",
        closed.data
    );

    // Baseline Runtime silently ignores 'display', so success alone would be
    // a false green. Its advertised additionalProperties=false schema must
    // also actually admit the explicit primary discriminated object.
    let definitions = tool_definitions();
    assert_eq!(definitions.len(), 8, "retain the eight existing tools");
    let schema = &definitions
        .iter()
        .find(|t| t.name == "computer_open")
        .expect("computer_open definition")
        .input_schema;
    assert!(
        schema["properties"]["display"].is_object(),
        "computer_open schema must declare optional display; silently ignoring it is not support"
    );
    let variants = schema["properties"]["display"]["oneOf"]
        .as_array()
        .expect("display must be a discriminated object schema");
    let primary = variants
        .iter()
        .find(|v| v["properties"]["kind"]["const"] == "primary")
        .expect("display schema must have a primary branch");
    assert_eq!(primary["type"], "object");
    assert_eq!(primary["required"], json!(["kind"]));
    assert_eq!(primary["additionalProperties"], false);
    assert_eq!(primary["properties"].as_object().unwrap().len(), 1);
    assert!(
        !schema["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|v| v == "display")),
        "display remains optional; omitted means primary"
    );
}

#[test]
fn observe_reports_selected_region_using_actual_resized_png_and_native_bounds() {
    let mut rt = mock_runtime();
    // Omit the selector here so metadata RED isn't masked by selector parsing.
    let opened = rt.call("computer_open", json!({"max_width": 160, "max_height": 90}));
    assert!(!opened.is_error, "open failed: {}", opened.data);
    let sid = opened.data["session_id"].as_str().expect("session ID");
    let observed = rt.call("computer_observe", json!({"session_id": sid}));
    assert!(!observed.is_error, "observe failed: {}", observed.data);
    let png = observed.image_png.as_ref().expect("actual in-memory PNG");
    let decoded = decode_png(png).expect("valid PNG");
    assert_eq!((decoded.width, decoded.height), (160, 90));
    assert_eq!(observed.data["width_px"], 160);
    assert_eq!(observed.data["height_px"], 90);
    assert_eq!(observed.data["input_sequence"], 0, "no input requested");
    let closed = rt.call("computer_close", json!({"session_id": sid}));
    assert!(
        !closed.is_error,
        "mock session cleanup failed: {}",
        closed.data
    );

    let regions = observed.data["mapping_regions"]
        .as_array()
        .expect("observation must publish per-display mapping regions, not just total image size");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0]["display_id"], DISPLAY_ID);
    assert_eq!(
        regions[0]["image_rect"],
        json!({"x": 0, "y": 0, "width": 160, "height": 90})
    );
    assert_eq!(regions[0]["native_bounds"], native_bounds());
    assert_eq!(observed.data["selected_display_ids"], json!([DISPLAY_ID]));
}

#[test]
fn observe_generation_matches_describe_and_survives_unchanged_recapture() {
    let mut rt = mock_runtime();
    let before = describe(&mut rt);
    let sid = open(&mut rt);
    let first = rt.call("computer_observe", json!({"session_id": sid}));
    let second = rt.call("computer_observe", json!({"session_id": sid}));
    assert!(
        !first.is_error && !second.is_error,
        "captures failed: {} / {}",
        first.data,
        second.data
    );
    assert!(first.image_png.is_some() && second.image_png.is_some());
    assert_ne!(first.data["observation_id"], second.data["observation_id"]);
    let after = describe(&mut rt);
    let closed = rt.call("computer_close", json!({"session_id": sid}));
    assert!(
        !closed.is_error,
        "mock session cleanup failed: {}",
        closed.data
    );

    // Check observation first: a missing field here is its own RED, independent
    // of the separate describe generation test. Never accept null == null.
    assert_eq!(full_generation(&first.data), full_generation(&before));
    assert_eq!(full_generation(&second.data), full_generation(&first.data));
    assert_eq!(full_generation(&after), full_generation(&first.data));
    assert_eq!(first.data["native_unit"], before["native_unit"]);
    assert!(matches!(
        first.data["native_unit"].as_str(),
        Some("points" | "physical_pixels")
    ));
}
