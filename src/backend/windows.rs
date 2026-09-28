//! Windows platform primitives for [`super::DesktopBackend`].
//!
//! - DPI awareness: the process is made Per-Monitor-V2 aware on first use,
//!   and the resulting context is VERIFIED (not assumed). Without this,
//!   GDI capture pixels, `SendInput` coordinates and `display_info`
//!   values disagree under display scaling.
//! - Capture: `screenshots` crate (GDI), whose pixel buffer is validated
//!   against the dimensions derived from `display_info` (native pixels on an
//!   aware process). A mismatch is a hard error, never a silently relabeled
//!   image.
//! - Permissions: Windows has no input-injection grant, so construction
//!   cannot prove input will succeed; `inject()` reports `SendInput`
//!   failures honestly (e.g. when the process is not in an interactive
//!   session or UIPI blocks delivery).

use super::capture::{encode_png, ScreenCaptureError, TargetScreen};
use super::BackendError;

#[link(name = "user32")]
extern "system" {
    /// DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 == -4.
    fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    /// Per Microsoft docs: passing NULL returns the DPI awareness context of
    /// the CURRENT process (equivalent to passing GetCurrentProcess()).
    fn GetDpiAwarenessContextForProcess(process: *mut core::ffi::c_void) -> isize;
    fn AreDpiAwarenessContextsEqual(a: isize, b: isize) -> i32;
}

const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

/// Current process DPI awareness context (NULL handle == current process,
/// per the Win32 API contract).
fn current_dpi_awareness_context() -> isize {
    unsafe { GetDpiAwarenessContextForProcess(std::ptr::null_mut()) }
}

/// Make (and verify) the process Per-Monitor-V2 DPI aware. Errors are
/// reported, never swallowed: running unaware would silently desynchronize
/// capture pixels from input coordinates under display scaling. A successful
/// `SetProcessDpiAwarenessContext` call is ALSO verified by re-reading the
/// current context afterwards.
pub(crate) fn prepare_thread() -> Result<(), BackendError> {
    unsafe {
        let current = current_dpi_awareness_context();
        if AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 {
            return Ok(());
        }
        // Not yet aware: try to become aware. This can fail if the process
        // already used DPI-affected APIs with a different context or a
        // manifest pins awareness.
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
            return Err(BackendError::new(
                "internal",
                format!(
                    "failed to set Per-Monitor-V2 DPI awareness (SetProcessDpiAwarenessContext returned 0; current context {current})"
                ),
            ));
        }
        // The call reported success: VERIFY the resulting context rather
        // than trusting the return value.
        let after = current_dpi_awareness_context();
        if AreDpiAwarenessContextsEqual(after, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
            return Err(BackendError::new(
                "internal",
                format!(
                    "Per-Monitor-V2 DPI awareness set reported success but verification failed (current context {after})"
                ),
            ));
        }
    }
    Ok(())
}

/// Create the input device. Windows grants injection without a permission
/// prompt, but `Enigo::new` can still fail outside an interactive session.
/// Note: success here does NOT prove later input will be delivered (UIPI,
/// locked desktop); failures surface from `inject()` as `input_failed`.
pub(crate) fn create_enigo() -> Result<enigo::Enigo, BackendError> {
    enigo::Enigo::new(&enigo::Settings::default()).map_err(|e| {
        BackendError::new(
            "input_failed",
            format!("failed to initialize Windows input (SendInput): {e}"),
        )
    })
}

/// Capture the target display as PNG with actual encoded dimensions.
pub(crate) fn capture_screen(target: &TargetScreen) -> Result<(Vec<u8>, u32, u32), BackendError> {
    let screen = screenshots::Screen::new(&target.display_info);
    let img = screen.capture().map_err(|e| {
        BackendError::from(ScreenCaptureError::Failed(format!(
            "GDI screen capture failed for display {}: {e}",
            target.display_info.id
        )))
    })?;
    let (w, h) = img.dimensions();
    // Consistency check, NOT double-scaling detection: `capture_width` is
    // computed with the exact expression screenshots 0.8.10 uses for its GDI
    // blit (`display_info.width * scale_factor`, physical pixels on a
    // DPI-aware process), so expected and actual agree by construction. A
    // mismatch means the installed capture path changed behavior or returned
    // a bitmap of a different size — our geometry metadata would lie, so fail
    // rather than ship mislabeled dimensions.
    if (w, h) != (target.capture_width, target.capture_height) {
        return Err(BackendError::from(ScreenCaptureError::Failed(format!(
            "capture dimensions {w}x{h} do not match expected {}x{} for display {}",
            target.capture_width, target.capture_height, target.display_info.id
        ))));
    }
    encode_png(w, h, img.into_raw()).map_err(BackendError::from)
}
