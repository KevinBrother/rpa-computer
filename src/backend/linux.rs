//! Linux platform primitives for [`super::DesktopBackend`] (X11 only).
//!
//! - Session guard: a Wayland session is refused HONESTLY (both
//!   `WAYLAND_DISPLAY` set and `XDG_SESSION_TYPE=wayland` are detected, even
//!   when `DISPLAY` is also set — under XWayland root-pixel input does not
//!   reliably match what applications see). `unsupported_session`, never a
//!   silent misbehaving fallback.
//! - Capture: `screenshots` crate (X11/XGetImage path), validated against
//!   the dimensions derived from `display_info` exactly like the Windows
//!   backend; a mismatch is a hard error, never a silently relabeled image.
//! - Input: `rpa_native_input::native_driver()` (libX11/libXtst XTest); the
//!   driver itself validates DISPLAY, the X server connection and the XTest
//!   extension, and reports `no_display`/`unsupported_session` honestly.
//! - DPI/scale audit: display-info on X11 reports root pixels with a scale
//!   factor that is normally 1.0 (root pixels ARE the input units and the
//!   capture units). The `width * scale_factor` expected-capture formula in
//!   `capture.rs` is kept because it degenerates to the identity at 1.0 and
//!   stays consistent with what `screenshots` blits on HiDPI X11 setups
//!   (e.g. `xrandr` scale). If a compositor reports an exotic scale, the
//!   hard dimension check below fails loudly instead of shipping mislabeled
//!   geometry.
//! - Unsupported hotkeys: there is no platform hotkey layer here; exotic key
//!   names are already rejected at `keys::parse_key` with `invalid_key`
//!   before any injection, which is the honest reporting path.

use super::capture::{encode_png, ScreenCaptureError, TargetScreen};
use super::BackendError;

/// Pure Wayland-session detector (unit-testable): true when the environment
/// indicates a Wayland session. `wayland_display` models `WAYLAND_DISPLAY`,
/// `session_type` models `XDG_SESSION_TYPE`.
fn wayland_session_detected(wayland_display: Option<&str>, session_type: Option<&str>) -> bool {
    // Independent OR: a nonempty WAYLAND_DISPLAY OR XDG_SESSION_TYPE=wayland
    // is sufficient. An EMPTY WAYLAND_DISPLAY (set but blank) must not
    // short-circuit the check — XDG_SESSION_TYPE still decides.
    wayland_display.is_some_and(|v| !v.is_empty())
        || session_type.is_some_and(|t| t.eq_ignore_ascii_case("wayland"))
}

/// No thread-level preparation needed on Linux: root pixels are the input
/// units and there is no process-wide DPI declaration to make.
pub(crate) fn prepare_thread() -> Result<(), BackendError> {
    Ok(())
}

/// Create the raw input driver, refusing Wayland sessions up front. The
/// driver performs the remaining X11 validation (DISPLAY present, server
/// reachable, XTest extension) and fails honestly otherwise.
pub(crate) fn create_driver() -> Result<Box<dyn rpa_native_input::Driver>, BackendError> {
    if wayland_session_detected(
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    ) {
        return Err(BackendError::new(
            "unsupported_session",
            "Wayland session detected (WAYLAND_DISPLAY or \
             XDG_SESSION_TYPE=wayland): native input requires X11/XTest; \
             run under X11 (XWayland root-pixel input is not supported)",
        ));
    }
    rpa_native_input::native_driver().map_err(|e| {
        BackendError::new(
            e.code.to_owned(),
            format!("failed to initialize Linux X11 input (XTest): {e}"),
        )
    })
}

/// Capture the target display as PNG with actual encoded dimensions.
pub(crate) fn capture_screen(target: &TargetScreen) -> Result<(Vec<u8>, u32, u32), BackendError> {
    let screen = screenshots::Screen::new(&target.display_info);
    let img = screen.capture().map_err(|e| {
        BackendError::from(ScreenCaptureError::Failed(format!(
            "X11 screen capture failed for display {}: {e}",
            target.display_info.id
        )))
    })?;
    let (w, h) = img.dimensions();
    // Hard consistency check (same policy as the Windows backend): expected
    // dimensions derive from display_info * scale, matching what screenshots
    // blits; a mismatch means our geometry metadata would lie, so fail.
    if (w, h) != (target.capture_width, target.capture_height) {
        return Err(BackendError::from(ScreenCaptureError::Failed(format!(
            "capture dimensions {w}x{h} do not match expected {}x{} for display {}",
            target.capture_width, target.capture_height, target.display_info.id
        ))));
    }
    encode_png(w, h, img.into_raw()).map_err(BackendError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_display_detected_even_when_display_is_also_set() {
        assert!(wayland_session_detected(Some("wayland-0"), Some("x11")));
        assert!(wayland_session_detected(Some("wayland-1"), None));
    }

    #[test]
    fn session_type_wayland_detected_without_display_variable() {
        assert!(wayland_session_detected(None, Some("wayland")));
        assert!(wayland_session_detected(None, Some("Wayland")));
    }

    #[test]
    fn empty_wayland_display_does_not_mask_wayland_session_type() {
        // Regression: Some("") used to short-circuit to false even when
        // XDG_SESSION_TYPE=wayland said otherwise.
        assert!(wayland_session_detected(Some(""), Some("wayland")));
        assert!(wayland_session_detected(Some(""), Some("WAYLAND")));
    }

    #[test]
    fn x11_sessions_pass() {
        assert!(!wayland_session_detected(None, Some("x11")));
        assert!(!wayland_session_detected(None, Some("tty")));
        assert!(!wayland_session_detected(None, None));
        assert!(!wayland_session_detected(Some(""), Some("x11")));
        assert!(!wayland_session_detected(Some(""), None));
    }
}
