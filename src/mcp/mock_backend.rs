//! Explicit mock backend for the `--mock-backend` diagnostic CLI mode.
//!
//! This is a DIAGNOSTIC mode only: it is activated exclusively by the
//! explicit `--mock-backend` CLI flag (never implicitly, never as a fallback
//! from native errors) and exists so protocol harnesses such as
//! `tests/mcp_protocol.py` can exercise the full MCP pipeline without any
//! real screen capture or OS input injection.
//!
//! Honesty contract:
//! - `platform()` reports `"mock"` and the geometry surface is
//!   `mock-surface`, so every protocol reply visibly identifies the mock.
//! - `capture()` returns a REAL, decodable PNG (generated locally, well
//!   above the runtime's minimum image dimension); dimensions are reported
//!   truthfully.
//! - `inject()` performs NO input at all — it records nothing on the
//!   desktop and only counts events in memory.
//! - The binary skips the real desktop writer lock, the native emergency
//!   hotkey, and all real input while this mode is active; that is reported
//!   on stderr at startup and in `--describe`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::backend::{Backend, BackendError, Capture, Geometry, InputEvent};

/// Mock surface size in "input units" and capture pixels. Deliberately far
/// above the runtime's `MIN_IMAGE_DIM` so captures survive the real
/// geometry/scale validation path.
const MOCK_WIDTH: u32 = 64;
const MOCK_HEIGHT: u32 = 64;

/// Deterministic, in-memory fake desktop. Not `Send`-restricted; the worker
/// constructs and owns it on the native thread like any other backend.
pub struct MockBackend {
    injected: AtomicU64,
}

impl MockBackend {
    pub fn new() -> Self {
        MockBackend {
            injected: AtomicU64::new(0),
        }
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for MockBackend {
    fn platform(&self) -> &'static str {
        "mock"
    }

    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        Ok(Geometry {
            surface_id: "mock-surface".into(),
            input_origin: (0, 0),
            input_size: (MOCK_WIDTH, MOCK_HEIGHT),
            version: "mock-v1".into(),
        })
    }

    fn capture(&mut self) -> Result<Capture, BackendError> {
        // A real, decodable PNG generated locally (no external fixture).
        // Pixel content is a fixed solid color — honest: nothing here ever
        // came from a screen.
        let img = image::RgbaImage::from_pixel(
            MOCK_WIDTH,
            MOCK_HEIGHT,
            image::Rgba([0x20, 0x40, 0x80, 0xFF]),
        );
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| BackendError::new("capture_failed", format!("mock png encode: {e}")))?;
        Ok(Capture {
            png: buf.into_inner(),
            width: MOCK_WIDTH,
            height: MOCK_HEIGHT,
            geometry: self.geometry()?,
        })
    }

    fn inject(&mut self, _event: &InputEvent) -> Result<(), BackendError> {
        // NO input is ever dispatched in mock mode; count only.
        self.injected.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn release_all(&mut self) -> Result<(), BackendError> {
        // Nothing was ever pressed; release is trivially complete.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_identifies_itself_and_captures_valid_png() {
        let mut b = MockBackend::new();
        assert_eq!(b.platform(), "mock");
        let cap = b.capture().expect("mock capture");
        assert_eq!(cap.width, MOCK_WIDTH);
        assert_eq!(cap.height, MOCK_HEIGHT);
        assert!(cap.width >= crate::runtime::image::MIN_IMAGE_DIM);
        // Decode with the same decoder the runtime uses.
        let decoded = image::load_from_memory(&cap.png).expect("mock png must decode");
        assert_eq!(decoded.width(), MOCK_WIDTH);
        assert_eq!(decoded.height(), MOCK_HEIGHT);
        assert_eq!(cap.geometry.surface_id, "mock-surface");
    }

    #[test]
    fn mock_inject_is_a_noop() {
        let mut b = MockBackend::new();
        b.inject(&InputEvent::Move { x: 1, y: 2 }).unwrap();
        b.inject(&InputEvent::Text {
            text: "hello".into(),
        })
        .unwrap();
        b.release_all().unwrap();
        assert_eq!(b.injected.load(Ordering::SeqCst), 2);
    }
}
