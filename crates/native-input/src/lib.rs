//! `rpa-native-input` — extractable native desktop input for Windows, macOS
//! and X11.
//!
//! Raw platform APIs only: no host, MCP, screenshot, serde, image, agent or
//! network dependencies. The crate owns native event construction; callers
//! own cancellation, deadlines and any screenshot semantics.
//!
//! Layers:
//! - [`Driver`] (`types`): the raw per-platform injection seam. The
//!   `windows`, `macos` and `linux_x11` modules each export a
//!   `pub(crate) struct Platform` implementing it; [`native_driver`]
//!   constructs the one for the current target.
//! - [`Input`] (`session`): the reusable session wrapper owning local
//!   held-key/button state, validation-before-dispatch, Return/Tab text
//!   mapping and best-effort release semantics.
//!
//! Host integration note (documented single-authority decision): the host
//! runtime (`rpa-computer`) uses [`native_driver`] through its OWN existing
//! state machine (`backend::dispatch_core::BackendCore`) and deliberately
//! does NOT wrap the driver in [`Input`], so exactly one component in the
//! process tracks held keys/buttons. [`Input`] remains the higher-level API
//! for standalone/embedded users of this crate.

mod session;
mod types;

// Compile the actual SendInput planner's pure tests on non-Windows hosts
// too; no Win32 calls, platform initialization or desktop input are involved.
#[cfg(all(test, not(target_os = "windows")))]
#[path = "windows/builder.rs"]
#[allow(dead_code)]
mod windows_builder_tests;

#[cfg(target_os = "linux")]
mod linux_x11;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub use session::Input;
pub use types::{is_valid_click_count, Axis, Button, Direction, Driver, InputError, Key, Result};

/// Construct the raw platform [`Driver`] for the current target. The host
/// runtime wraps this in its own state machine; `Input::new` wraps it in the
/// crate's [`Input`] session. No `Send`/`Sync` promises: the driver is
/// thread-bound and must be created and used on the same worker thread.
pub fn native_driver() -> Result<Box<dyn Driver>> {
    #[cfg(target_os = "windows")]
    return Ok(Box::new(windows::Platform::new()?));
    #[cfg(target_os = "macos")]
    return Ok(Box::new(macos::Platform::new()?));
    #[cfg(target_os = "linux")]
    return Ok(Box::new(linux_x11::Platform::new()?));
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return Err(InputError::new(
        "unsupported_session",
        format!("unsupported target OS: {}", std::env::consts::OS),
    ));
}
