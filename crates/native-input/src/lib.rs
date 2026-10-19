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
//! runtime (`rpa-computer`) uses [`native_driver_with_x11_lease`] on Linux
//! (and [`native_driver`] elsewhere) through its OWN existing
//! state machine (`backend::dispatch_core::BackendCore`) and deliberately
//! does NOT wrap the driver in [`Input`], so exactly one component in the
//! process tracks held keys/buttons. [`Input`] remains the higher-level API
//! for standalone/embedded users of this crate.

mod session;
mod types;
mod x11_lease;
pub use x11_lease::{X11Lease, X11_LEASE_NONCE_PROPERTY, X11_LEASE_SELECTION};

#[cfg(test)]
mod x11_lease_tests;

// Compile the actual SendInput planner's pure tests on non-Windows hosts
// too; no Win32 calls, platform initialization or desktop input are involved.
#[cfg(all(test, not(target_os = "windows")))]
#[path = "windows/builder.rs"]
#[allow(dead_code)]
mod windows_builder_tests;

// Linux text policy is pure Rust: exercise the same lease/guard logic on
// other hosts without compiling or linking X11/XTest FFI.
#[cfg(all(test, not(target_os = "linux")))]
#[path = "linux_x11/text_policy.rs"]
#[allow(dead_code)]
mod linux_x11_text_policy;

#[cfg(all(test, not(target_os = "linux")))]
#[path = "linux_x11/keysym.rs"]
#[allow(dead_code)]
mod linux_x11_keysyms;

#[cfg(all(test, not(target_os = "linux")))]
#[path = "linux_x11/definition.rs"]
#[allow(dead_code)]
mod linux_x11_definition;
#[cfg(all(test, not(target_os = "linux")))]
#[path = "linux_x11/device_policy.rs"]
#[allow(dead_code)]
mod linux_x11_device_policy;

#[cfg(all(test, not(target_os = "linux")))]
#[path = "linux_x11/transient_button.rs"]
#[allow(dead_code)]
mod linux_x11_transient_button;

#[cfg(all(test, not(target_os = "linux")))]
#[path = "linux_x11/map_request.rs"]
#[allow(dead_code)]
mod linux_x11_map_request;

#[cfg(all(test, not(target_os = "linux")))]
#[path = "linux_x11/inventory.rs"]
#[allow(dead_code)]
mod linux_x11_inventory;

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

/// Construct a driver bound to the host's established X11 desktop lease.
/// Never falls back to an unleased driver. Other platforms reject this factory.
pub fn native_driver_with_x11_lease(lease: X11Lease) -> Result<Box<dyn Driver>> {
    #[cfg(target_os = "linux")]
    return Ok(Box::new(linux_x11::Platform::with_x11_lease(lease)?));
    #[cfg(not(target_os = "linux"))]
    {
        let _ = lease;
        Err(InputError::new(
            "unsupported_session",
            "X11 lease factory requires Linux/X11",
        ))
    }
}
