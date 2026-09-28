//! Shared mock backend + helpers for the contract test suite.
//!
//! Not a #[test] module; used by every contract submodule.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rpa_computer::backend::{Backend, BackendError, Capture, Direction, Geometry, InputEvent};
use rpa_computer::runtime::Runtime;

// ---------------------------------------------------------------------------
// Mock backend
// ---------------------------------------------------------------------------

pub struct MockBackend {
    pub geometry: Geometry,
    /// Each capture() pops the next result; falls back to a repeating default.
    pub capture_results: VecDeque<Result<Capture, BackendError>>,
    pub default_capture: Capture,
    /// Each inject() pops the next result; default Ok.
    pub inject_results: VecDeque<Result<(), BackendError>>,
    /// Each release_all() pops the next result; default Ok.
    pub release_results: VecDeque<Result<(), BackendError>>,
    pub log: Vec<String>,
}

impl MockBackend {
    pub fn new(width: u32, height: u32) -> Self {
        let geometry = Geometry {
            surface_id: "mock-surface-0".into(),
            input_origin: (0, 0),
            input_size: (1920, 1080),
            version: "geom-v1".into(),
        };
        let default_capture = Capture {
            png: tiny_png(width, height),
            width,
            height,
            geometry: geometry.clone(),
        };
        MockBackend {
            geometry,
            capture_results: VecDeque::new(),
            default_capture,
            inject_results: VecDeque::new(),
            release_results: VecDeque::new(),
            log: Vec::new(),
        }
    }

    pub fn inject_count(&self) -> usize {
        self.log.iter().filter(|e| e.starts_with("inject:")).count()
    }

    pub fn events(&self) -> Vec<&str> {
        self.log
            .iter()
            .filter(|e| e.starts_with("inject:"))
            .map(|s| s.as_str())
            .collect()
    }

    pub fn release_count(&self) -> usize {
        self.log.iter().filter(|e| e.starts_with("release")).count()
    }
}

impl Backend for MockBackend {
    fn platform(&self) -> &'static str {
        "mock"
    }

    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.log.push("geometry".into());
        Ok(self.geometry.clone())
    }

    fn capture(&mut self) -> Result<Capture, BackendError> {
        self.log.push("capture".into());
        self.capture_results
            .pop_front()
            .unwrap_or_else(|| Ok(clone_capture(&self.default_capture)))
    }

    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        self.log.push(format!("inject:{:?}", event));
        self.inject_results.pop_front().unwrap_or(Ok(()))
    }

    fn release_all(&mut self) -> Result<(), BackendError> {
        self.log.push("release_all".into());
        self.release_results.pop_front().unwrap_or(Ok(()))
    }
}

/// Shared handle so tests can inspect the mock after it has been moved into
/// the Runtime. The runtime is single-threaded in tests, so a Mutex suffices.
pub struct SharedBackend {
    pub inner: Arc<Mutex<MockBackend>>,
}

impl Backend for SharedBackend {
    fn platform(&self) -> &'static str {
        "mock"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.inner.lock().unwrap().geometry()
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        self.inner.lock().unwrap().capture()
    }
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        self.inner.lock().unwrap().inject(event)
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        self.inner.lock().unwrap().release_all()
    }
}

pub struct Harness {
    pub runtime: Runtime,
    pub mock: Arc<Mutex<MockBackend>>,
    pub cancel: Arc<AtomicBool>,
}

pub fn harness_with(mock: MockBackend) -> Harness {
    let mock = Arc::new(Mutex::new(mock));
    let cancel = Arc::new(AtomicBool::new(false));
    let runtime = Runtime::new(
        Box::new(SharedBackend {
            inner: Arc::clone(&mock),
        }),
        Arc::clone(&cancel),
    );
    Harness {
        runtime,
        mock,
        cancel,
    }
}

pub fn harness() -> Harness {
    harness_with(MockBackend::new(1280, 720))
}

/// Harness constructor accepting any Backend. Kept separate so the generic
/// harness stays simple.
pub fn harness_backend<B: Backend + 'static>(backend: B) -> (Runtime, Arc<AtomicBool>) {
    let cancel = Arc::new(AtomicBool::new(false));
    (Runtime::new(Box::new(backend), Arc::clone(&cancel)), cancel)
}

pub fn err(code: &str, message: &str) -> BackendError {
    BackendError {
        code: code.into(),
        message: message.into(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Smallest valid PNG with the requested dimensions (all-zero RGB pixels),
/// generated with the `image` crate so width/height metadata is truthful.
pub fn tiny_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::new(width, height);
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    use image::ImageEncoder;
    encoder
        .write_image(img.as_raw(), width, height, image::ColorType::Rgb8)
        .expect("encode test png");
    buf
}

pub fn call(rt: &mut Runtime, name: &str, args: serde_json::Value) -> rpa_computer::runtime::Reply {
    rt.call(name, args)
}

pub fn open_session(rt: &mut Runtime) -> String {
    let reply = call(rt, "computer_open", serde_json::json!({}));
    assert!(
        !reply.is_error,
        "computer_open must succeed with healthy mock, got: {:?}",
        reply.data
    );
    reply.data["session_id"]
        .as_str()
        .expect("open must return session_id")
        .to_string()
}

pub fn observe(rt: &mut Runtime, session_id: &str) -> serde_json::Value {
    let reply = call(
        rt,
        "computer_observe",
        serde_json::json!({ "session_id": session_id }),
    );
    assert!(
        !reply.is_error,
        "observe must succeed with healthy mock, got: {:?}",
        reply.data
    );
    assert!(
        reply.image_png.is_some(),
        "observe must return image bytes alongside metadata"
    );
    reply.data
}

/// Reconstructing a `Capture` by hand (the struct intentionally has no
/// `Clone`; re-encoding keeps byte equality for our all-zero fixtures).
pub fn clone_capture(c: &Capture) -> Capture {
    Capture {
        png: c.png.clone(),
        width: c.width,
        height: c.height,
        geometry: c.geometry.clone(),
    }
}

/// Extract the string `code` of an error reply, tolerating either a top-level
/// `code` field or an `error: { code, message }` object (contract only
/// requires "a machine-readable code").
pub fn error_code(reply: &rpa_computer::runtime::Reply) -> String {
    assert!(
        reply.is_error,
        "expected error reply, got: {:?}",
        reply.data
    );
    if let Some(code) = reply.data.get("code").and_then(|c| c.as_str()) {
        return code.to_string();
    }
    reply
        .data
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or_else(|| {
            panic!(
                "error reply must carry a machine-readable code: {:?}",
                reply.data
            )
        })
        .to_string()
}

pub fn step_args(
    session_id: &str,
    request_id: &str,
    based_on: &str,
    action: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "request_id": request_id,
        "based_on": based_on,
        "action": action,
    })
}

// Silence unused-import warnings for items used only in some configurations.
#[allow(dead_code)]
pub fn _unused(_: Direction) {}
