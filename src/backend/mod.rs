//! Native desktop backend: real screen capture + real OS input injection.
//!
//! Contract owner: backend worker. The public API in this module is exactly the
//! `backend API` section of `.agents/CONTRACT.md`; do not change signatures
//! without coordination.
//!
//! Coordinate model
//! ----------------
//! - `Geometry.input_origin` / `Geometry.input_size` describe the target
//!   (primary) display in **native backend input units**, i.e. the units that
//!   [`Backend::inject`] accepts for `InputEvent::Move { x, y }`:
//!   - macOS: global CoreGraphics points (origin at the primary display's
//!     top-left; other displays extend into negative/larger coordinates).
//!   - Windows: virtual-screen pixels (origin at the primary display's
//!     top-left, which is `(0, 0)`); the process must be DPI aware so these
//!     match what applications see.
//! - `Capture.width` / `Capture.height` are the **actual** dimensions of the
//!   encoded PNG. On macOS Retina this is `points * backing_scale` (e.g.
//!   3024x1964 for a 1512x982 display); the Runtime maps model image
//!   coordinates into `input_size` using these capture dimensions.
//! - `Geometry.version` changes whenever the input geometry, DPI/scale or the
//!   monitor identity changes, so a stale image is never silently reused
//!   against a new display layout.
//!
//! Error codes (machine readable): `unsupported_platform`, `no_display`,
//! `screen_capture_denied`, `capture_failed`, `permission_denied`,
//! `input_failed`, `invalid_key`, `invalid_button`, `invalid_text`,
//! `geometry_changed`, `internal`.

mod capture;
mod desktop;
pub(crate) mod dispatch;
mod dispatch_core;
pub mod keys;
#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
mod live_test;

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "windows")]
pub(crate) mod windows;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("backend::DesktopBackend supports only macOS and Windows targets");

pub use capture::{encode_png, target_screen, ScreenCaptureError, TargetScreen};
pub use desktop::DesktopBackend;
pub use keys::{parse_button, parse_key, BUTTON_NAMES, KEY_NAMES};

use crate::backend::keys::ParsedKey;

/// Geometry of the capture/input surface in native backend input units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry {
    /// Identifier of the captured surface (native display id, e.g. `macos:1`).
    pub surface_id: String,
    /// Top-left of the surface in native backend input units.
    pub input_origin: (i32, i32),
    /// Size of the surface in native backend input units.
    pub input_size: (u32, u32),
    /// Changes whenever input geometry, DPI/scale or monitor identity changes.
    pub version: String,
}

/// A screen capture. `width`/`height` are the ACTUAL pixel dimensions of the
/// encoded PNG, which may differ from `geometry.input_size` (e.g. macOS
/// Retina backing scale, Windows DPI scaling).
pub struct Capture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub geometry: Geometry,
}

/// Direction of a button/key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Press,
    Release,
}

/// One atomic input event. Coordinates are absolute, in native backend input
/// units (already mapped from image coordinates by the Runtime).
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Move {
        x: i32,
        y: i32,
    },
    Button {
        button: String,
        direction: Direction,
    },
    Key {
        key: String,
        direction: Direction,
    },
    Text {
        text: String,
    },
    /// Wheel ticks, positive = right/down.
    Scroll {
        x: i32,
        y: i32,
    },
}

/// Structured backend failure. `code` is stable and machine readable.
#[derive(Debug, Clone)]
pub struct BackendError {
    pub code: String,
    pub message: String,
}

impl BackendError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for BackendError {}

impl From<ScreenCaptureError> for BackendError {
    fn from(e: ScreenCaptureError) -> Self {
        match e {
            ScreenCaptureError::NoDisplay(m) => BackendError::new("no_display", m),
            ScreenCaptureError::Denied(m) => BackendError::new("screen_capture_denied", m),
            ScreenCaptureError::Failed(m) => BackendError::new("capture_failed", m),
        }
    }
}

/// Native platform backend. Intentionally NOT `Send`: native input resources
/// (CGEventSource / SendInput state) must be created and used on the same
/// worker thread.
pub trait Backend {
    fn platform(&self) -> &'static str;
    /// Current target-display geometry. Must reflect display/DPI changes.
    fn geometry(&mut self) -> Result<Geometry, BackendError>;
    /// Capture the target display. PNG dimensions are reported truthfully.
    fn capture(&mut self) -> Result<Capture, BackendError>;
    /// Inject a single atomic input event. Names are validated before any
    /// input is emitted.
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError>;
    /// Best-effort release of every key/button this backend pressed. Tries
    /// all of them even when individual releases fail; reports failures.
    fn release_all(&mut self) -> Result<(), BackendError>;
}

/// What [`DesktopBackend`] tracks as currently held, for `release_all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HeldItem {
    Key(ParsedKey),
    Button(enigo::Button),
}

impl HeldItem {
    /// Deterministic ordering key for release attempts (buttons before keys,
    /// then by debug/name): keeps `release_all` diagnostics reproducible.
    pub(crate) fn sort_key(&self) -> (u8, String) {
        match self {
            HeldItem::Button(b) => (0, format!("{b:?}")),
            HeldItem::Key(k) => (1, k.name()),
        }
    }
}

impl std::fmt::Display for HeldItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeldItem::Key(k) => write!(f, "key({})", k.name()),
            HeldItem::Button(b) => write!(f, "button({b:?})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_error_is_structured() {
        let e = BackendError::new("invalid_key", "unknown key: foo");
        assert_eq!(e.code, "invalid_key");
        assert!(e.message.contains("foo"));
        assert_eq!(e.to_string(), "[invalid_key] unknown key: foo");
    }

    #[test]
    fn screen_capture_error_maps_to_codes() {
        let denied: BackendError = ScreenCaptureError::Denied("tcc".into()).into();
        assert_eq!(denied.code, "screen_capture_denied");
        let nodisp: BackendError = ScreenCaptureError::NoDisplay("x".into()).into();
        assert_eq!(nodisp.code, "no_display");
        let failed: BackendError = ScreenCaptureError::Failed("x".into()).into();
        assert_eq!(failed.code, "capture_failed");
    }
}
