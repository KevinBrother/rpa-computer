//! macOS platform primitives for [`super::DesktopBackend`].
//!
//! - Capture: in-memory via the `screenshots` crate, which uses the
//!   CoreGraphics `CGDisplay::screenshot` API (CGWindowListCreateImage) —
//!   no subprocess, no temporary files. The actual encoded PNG dimensions
//!   are authoritative for `Capture.width/height`. `CGDisplayPixelsWide/High`
//!   is used only as a sanity cross-check when it is at least as large as the
//!   capture: some logical display modes (e.g. scaled "more space" modes)
//!   make `CGDisplayPixelsWide` report the logical-mode size, which is
//!   SMALLER than the backing store `CGDisplay::screenshot` actually
//!   renders; a valid capture must never be rejected for that.
//! - Permissions: `CGPreflightScreenCaptureAccess` preflight before capture
//!   (macOS 10.15+ FFI, no prompt; the CG API would otherwise return a
//!   wallpaper-only or null image). Input permission is preflighted via
//!   `AXIsProcessTrusted` — also never prompting — in `create_enigo`.

use super::capture::{encode_png, ScreenCaptureError, TargetScreen};
use super::BackendError;

// ---------------------------------------------------------------------------
// FFI (no extra crate dependencies; linked frameworks are always present).
// ---------------------------------------------------------------------------

type CGDirectDisplayID = u32;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// Pixel width/height reported by CoreGraphics for a display. Note: this
    /// reflects the current display MODE and may be a logical-mode size
    /// smaller than the actual backing store rendered by
    /// `CGDisplay::screenshot`; use only as a >= cross-check.
    fn CGDisplayPixelsWide(display: CGDirectDisplayID) -> usize;
    fn CGDisplayPixelsHigh(display: CGDirectDisplayID) -> usize;
    /// Screen Recording permission preflight (macOS 10.15+). Never prompts.
    fn CGPreflightScreenCaptureAccess() -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    /// Accessibility (input injection) permission preflight. Never prompts.
    fn AXIsProcessTrusted() -> bool;
}

// ---------------------------------------------------------------------------
// Backend hooks
// ---------------------------------------------------------------------------

/// No thread-level preparation needed on macOS (input units == global
/// CoreGraphics points, independent of any process DPI declaration).
pub(crate) fn prepare_thread() -> Result<(), BackendError> {
    Ok(())
}

/// Create the input device. macOS requires the Accessibility grant; this
/// preflights WITHOUT prompting and refuses to construct otherwise, so
/// `DesktopBackend::new()` never silently claims input capability it lacks.
pub(crate) fn create_enigo() -> Result<enigo::Enigo, BackendError> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err(BackendError::new(
            "permission_denied",
            "Accessibility (input monitoring) permission is not granted to this \
             process; grant it in System Settings > Privacy & Security > \
             Accessibility and restart the process",
        ));
    }
    let settings = enigo::Settings {
        // Never open the system prompt from inside the backend; permission
        // acquisition is an explicit user action.
        open_prompt_to_get_permissions: false,
        ..Default::default()
    };
    enigo::Enigo::new(&settings).map_err(|e| {
        BackendError::new(
            "permission_denied",
            format!("failed to create macOS input event source: {e}"),
        )
    })
}

/// Pixel dimensions reported by `CGDisplayPixelsWide/High`, if nonzero.
/// Pure query helper; callers must treat the result as informational only
/// (see module docs for the logical-mode caveat).
pub(crate) fn reported_display_pixels(display_id: u32) -> Option<(u32, u32)> {
    let w = unsafe { CGDisplayPixelsWide(display_id) };
    let h = unsafe { CGDisplayPixelsHigh(display_id) };
    if w == 0 || h == 0 {
        None
    } else {
        Some((w as u32, h as u32))
    }
}

/// Capture the target display as PNG, fully in memory (no subprocess).
/// Returns the encoded bytes and the ACTUAL pixel dimensions of the image
/// CoreGraphics produced — those are authoritative.
pub(crate) fn capture_screen(target: &TargetScreen) -> Result<(Vec<u8>, u32, u32), BackendError> {
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        return Err(BackendError::new(
            "screen_capture_denied",
            "Screen Recording permission is not granted to this process; grant \
             it in System Settings > Privacy & Security > Screen Recording \
             and restart the process",
        ));
    }
    let id = target.display_info.id;
    let screen = screenshots::Screen::new(&target.display_info);
    let img = screen.capture().map_err(|e| {
        BackendError::from(ScreenCaptureError::Failed(format!(
            "CoreGraphics screen capture failed for display {id}: {e}"
        )))
    })?;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err(BackendError::from(ScreenCaptureError::Failed(format!(
            "CoreGraphics returned an empty image for display {id}"
        ))));
    }

    // Sanity cross-check only: CGDisplayPixelsWide/High may report a
    // logical-mode size SMALLER than the rendered backing store (scaled
    // display modes), so only fail when it claims MORE pixels than we got —
    // that combination would mean our capture is genuinely short.
    if let Some((pw, ph)) = reported_display_pixels(id) {
        if (pw as u64) * (ph as u64) > (w as u64) * (h as u64) {
            return Err(BackendError::from(ScreenCaptureError::Failed(format!(
                "capture {w}x{h} has fewer pixels than CGDisplayPixelsWide/High reports ({pw}x{ph}) for display {id}"
            ))));
        }
    }
    encode_png(w, h, img.into_raw()).map_err(BackendError::from)
}
