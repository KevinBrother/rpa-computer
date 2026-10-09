//! Windows platform primitives for [`super::DesktopBackend`].
//!
//! - DPI awareness: the process is made Per-Monitor-V2 aware on first use,
//!   and the resulting context is VERIFIED (not assumed). Without this,
//!   GDI capture pixels, `SendInput` coordinates and `display_info`
//!   values disagree under display scaling.
//! - Capture: persistent `display::DisplayProvider<GdiProvider>` in DesktopBackend,
//!   using full physical topology and per-source GDI composition; no primary
//!   screenshots fallback.
//! - Permissions: Windows has no input-injection grant, so construction
//!   cannot prove input will succeed; `inject()` reports `SendInput`
//!   failures honestly (e.g. when the process is not in an interactive
//!   session or UIPI blocks delivery).

use super::BackendError;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetDpiAwarenessContextForProcess, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

/// Current process DPI awareness context (NULL handle == current process,
/// per the Win32 API contract). Namespaces per the actual windows 0.58
/// bindings: `DPI_AWARENESS_CONTEXT` lives in `UI::HiDpi`, the parameter is
/// a plain `Foundation::HANDLE` (there is no HPROCESS in this version).
fn current_dpi_awareness_context() -> DPI_AWARENESS_CONTEXT {
    unsafe { GetDpiAwarenessContextForProcess(HANDLE::default()) }
}

/// Make (and verify) the process Per-Monitor-V2 DPI aware. Errors are
/// reported, never swallowed: running unaware would silently desynchronize
/// capture pixels from input coordinates under display scaling. A successful
/// `SetProcessDpiAwarenessContext` call is ALSO verified by re-reading the
/// current context afterwards. Uses the official `windows` crate bindings
/// (official Bindings over hand-written user32 externs).
pub(crate) fn prepare_thread() -> Result<(), BackendError> {
    unsafe {
        let current = current_dpi_awareness_context();
        if AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            .as_bool()
        {
            return Ok(());
        }
        // Not yet aware: try to become aware. This can fail if the process
        // already used DPI-affected APIs with a different context or a
        // manifest pins awareness.
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            return Err(BackendError::new(
                "internal",
                format!(
                    "failed to set Per-Monitor-V2 DPI awareness (SetProcessDpiAwarenessContext failed; current context {:?})",
                    current.0
                ),
            ));
        }
        // The call reported success: VERIFY the resulting context rather
        // than trusting the return value.
        let after = current_dpi_awareness_context();
        if !AreDpiAwarenessContextsEqual(after, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            .as_bool()
        {
            return Err(BackendError::new(
                "internal",
                format!(
                    "Per-Monitor-V2 DPI awareness set reported success but verification failed (current context {:?})",
                    after.0
                ),
            ));
        }
    }
    Ok(())
}

/// Create the raw input driver. Windows grants injection without a permission
/// prompt, but driver construction can still fail outside an interactive
/// session. Note: success here does NOT prove later input will be delivered
/// (UIPI, locked desktop); failures surface from `inject()` as
/// `input_failed`.
pub(crate) fn create_driver() -> Result<Box<dyn rpa_native_input::Driver>, BackendError> {
    rpa_native_input::native_driver()
        .map_err(|e| BackendError::new(e.code.to_owned(), format!("Windows input: {e}")))
}
