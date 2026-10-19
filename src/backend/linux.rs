//! Linux/X11 environment guard and leased native input factory.
//! RandR topology and root-pixel capture live in `linux_display`; neither font
//! DPI nor screenshots/display-info participates in input-coordinate mapping.

use super::BackendError;
use std::ffi::OsStr;

fn wayland_session_detected(wayland_display: Option<&OsStr>, session_type: Option<&str>) -> bool {
    wayland_display.is_some_and(|value| !value.is_empty())
        || session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
}

/// Reject Wayland before connecting any capture/input resource. var_os keeps
/// non-UTF8, nonempty WAYLAND_DISPLAY values from accidentally passing the guard.
pub(crate) fn prepare_thread() -> Result<(), BackendError> {
    if wayland_session_detected(
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    ) {
        return Err(BackendError::new(
            "unsupported_session",
            "Wayland/XWayland is unsupported; an X11 desktop is required",
        ));
    }
    Ok(())
}

/// No unleased fallback. Native input checks selection/owner/nonce on its own
/// Xlib connection; all clones of the lease share irreversible revocation.
pub(crate) fn create_driver(
    lease: rpa_native_input::X11Lease,
) -> Result<Box<dyn rpa_native_input::Driver>, BackendError> {
    prepare_thread()?;
    rpa_native_input::native_driver_with_x11_lease(lease).map_err(|error| {
        BackendError::new(
            error.code,
            format!("failed to initialize leased X11 input: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unleased_backend_constructor_fails_before_connecting_to_a_desktop() {
        let error = crate::backend::DesktopBackend::new()
            .err()
            .expect("lease must be required");
        assert_eq!(error.code, "desktop_lease_required");
    }

    #[test]
    fn wayland_display_detected_even_when_display_is_also_set() {
        assert!(wayland_session_detected(
            Some(OsStr::new("wayland-0")),
            Some("x11")
        ));
        assert!(wayland_session_detected(
            Some(OsStr::new("wayland-1")),
            None
        ));
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
        assert!(wayland_session_detected(
            Some(OsStr::new("")),
            Some("wayland")
        ));
        assert!(wayland_session_detected(
            Some(OsStr::new("")),
            Some("WAYLAND")
        ));
    }

    #[test]
    fn x11_sessions_pass() {
        assert!(!wayland_session_detected(None, Some("x11")));
        assert!(!wayland_session_detected(None, Some("tty")));
        assert!(!wayland_session_detected(None, None));
        assert!(!wayland_session_detected(Some(OsStr::new("")), Some("x11")));
        assert!(!wayland_session_detected(Some(OsStr::new("")), None));
    }
}
