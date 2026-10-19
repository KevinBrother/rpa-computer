//! Low-level native input dispatch, isolated behind the [`NativeInput`] trait
//! so held-state tracking, scroll translation and release semantics can be
//! regression-tested with a recording mock (no real desktop input).
//!
//! Contract: all coordinates/directions arriving here are already in native
//! backend input units; scroll deltas follow the contract convention
//! **positive = right/down**, which is exactly the `rpa-native-input` crate's
//! documented `Driver::scroll` convention, so deltas pass through unchanged.
//!
//! Single held-state authority: the production dispatcher wraps the raw
//! `rpa_native_input::Driver` (from [`rpa_native_input::native_driver`]) and
//! does NOT use the crate's `Input` session wrapper — `BackendCore` below is
//! the one component that tracks held keys/buttons in this process. `Input`
//! remains the higher-level API for standalone users of the crate.

use super::{BackendError, Direction};
use rpa_native_input::{Button, Driver, InputError, Key};

/// Native input device abstraction. Production uses [`DriverInput`]; tests
/// substitute a recording implementation. NOT required to be `Send`: it is
/// constructed and used on the backend worker thread only.
pub(crate) trait NativeInput {
    /// Whether control text scalars must pass through the native text guard.
    /// Other backends retain BackendCore's existing Return/Tab key behavior.
    fn control_text_uses_scalar(&self) -> bool {
        false
    }

    /// Release driver-owned session resources after tracked release attempts.
    /// Retryable on failure, and does not destroy the reusable native device.
    fn cleanup(&mut self) -> Result<(), BackendError> {
        Ok(())
    }

    fn move_mouse(&mut self, x: i32, y: i32) -> Result<(), BackendError>;
    /// `length` in wheel ticks, positive = right (Horizontal) / down
    /// (Vertical). Passed through to the driver unchanged.
    fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), BackendError>;
    /// `click_count` is the native click-state of THIS press/up pair
    /// (valid 1..=3, validated upstream by `BackendCore` and again by the
    /// driver); passed through to the driver unchanged.
    fn button(
        &mut self,
        button: Button,
        direction: Direction,
        click_count: u8,
    ) -> Result<(), BackendError>;
    fn key(&mut self, key: Key, direction: Direction) -> Result<(), BackendError>;
    /// One Unicode scalar on the native text path. `'\n'`/`'\r'`→Return and
    /// `'\t'`→Tab mapping is applied by `BackendCore` (so the clicks go
    /// through its held-state machine); everything else lands here.
    fn text_scalar(&mut self, ch: char) -> Result<(), BackendError>;
    /// Explicit host clipboard path, never a fallback for native text. On
    /// success the clipboard has been replaced; verify immediately before
    /// dispatching the chord. Unsupported platforms must have no effects.
    fn prepare_text_paste(&mut self, _text: &str) -> Result<(), BackendError> {
        Err(BackendError::new(
            "unsupported_platform",
            "explicit clipboard paste is supported only on Windows; clipboard unchanged",
        ))
    }
    fn verify_text_paste(&mut self) -> Result<(), BackendError> {
        Err(BackendError::new(
            "unsupported_platform",
            "explicit clipboard paste is supported only on Windows; clipboard unchanged",
        ))
    }
}

/// Scroll axis (mirrors `rpa_native_input::Axis` without leaking it into the seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

/// Production dispatcher: thin adapter over the raw platform `Driver`.
///
/// No translation happens here on purpose:
/// - Scroll deltas: the crate documents positive = down/right; any native
///   wheel-delta sign handling is a platform detail inside the driver.
///   Negating here would invert the user's scroll direction.
/// - `Key::Character(c)`: the driver maps the character to a real key event
///   via the current keyboard layout. We only ever pass single ASCII
///   alphanumerics.
pub(crate) struct DriverInput {
    driver: Box<dyn Driver>,
    #[cfg(target_os = "windows")]
    prepared_paste: Option<super::windows_clipboard::PreparedPaste>,
    #[cfg(target_os = "linux")]
    clipboard: Option<super::linux_clipboard::Clipboard>,
}

impl DriverInput {
    /// Test-only access to install an owner transport without a desktop.
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn clipboard_for_test(&mut self) -> &mut super::linux_clipboard::Clipboard {
        self.clipboard.as_mut().expect("leased test adapter")
    }

    pub(crate) fn new(driver: Box<dyn Driver>) -> Self {
        Self {
            driver,
            #[cfg(target_os = "windows")]
            prepared_paste: None,
            #[cfg(target_os = "linux")]
            clipboard: None,
        }
    }

    /// Host explicitly passes the same lease used by the native input driver.
    /// Clipboard connection and ownership remain lazy until explicit paste.
    #[cfg(target_os = "linux")]
    pub(crate) fn new_with_x11_lease(
        driver: Box<dyn Driver>,
        lease: rpa_native_input::X11Lease,
    ) -> Self {
        Self {
            driver,
            clipboard: Some(super::linux_clipboard::Clipboard::new(lease)),
        }
    }
}

impl NativeInput for DriverInput {
    fn control_text_uses_scalar(&self) -> bool {
        self.driver.control_text_uses_scalar()
    }

    fn cleanup(&mut self) -> Result<(), BackendError> {
        let native = self
            .driver
            .cleanup()
            .map_err(|e| input_error("native cleanup", e));
        #[cfg(target_os = "linux")]
        {
            let clipboard = self.clipboard.as_mut().map_or(Ok(()), |c| c.cleanup());
            return match (native, clipboard) {
                (result, Ok(())) => result,
                (Ok(()), Err(e)) => Err(e),
                (Err(native), Err(clipboard)) => Err(BackendError::new(
                    "input_failed",
                    format!("{}; {}", native.message, clipboard.message),
                )),
            };
        }
        #[cfg(not(target_os = "linux"))]
        native
    }

    #[cfg(target_os = "linux")]
    fn prepare_text_paste(&mut self, text: &str) -> Result<(), BackendError> {
        self.clipboard
            .as_mut()
            .ok_or_else(|| {
                BackendError::new(
                    "permission_denied",
                    "explicit Linux paste requires an injected X11 lease; clipboard unchanged",
                )
            })?
            .prepare(text)
    }

    #[cfg(target_os = "linux")]
    fn verify_text_paste(&mut self) -> Result<(), BackendError> {
        self.clipboard
            .as_mut()
            .ok_or_else(|| {
                BackendError::new(
                    "permission_denied",
                    "explicit Linux paste requires an injected X11 lease; no chord dispatched",
                )
            })?
            .verify()
    }

    #[cfg(target_os = "windows")]
    fn prepare_text_paste(&mut self, text: &str) -> Result<(), BackendError> {
        // A failed preparation must never leave a usable old receipt.
        self.prepared_paste = None;
        self.prepared_paste = Some(super::windows_clipboard::prepare(text)?);
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn verify_text_paste(&mut self) -> Result<(), BackendError> {
        // Consume even on guard failure; receipts cannot authorize a retry.
        let prepared = self.prepared_paste.take().ok_or_else(|| {
            BackendError::new(
                "input_failed",
                "no prepared clipboard paste receipt; clipboard state unknown",
            )
        })?;
        prepared.verify()
    }

    fn move_mouse(&mut self, x: i32, y: i32) -> Result<(), BackendError> {
        self.driver
            .move_mouse(x, y)
            .map_err(|e| input_error("mouse move", e))
    }

    fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), BackendError> {
        let axis = match axis {
            Axis::Horizontal => rpa_native_input::Axis::Horizontal,
            Axis::Vertical => rpa_native_input::Axis::Vertical,
        };
        self.driver
            .scroll(length, axis)
            .map_err(|e| input_error("scroll", e))
    }

    fn button(
        &mut self,
        button: Button,
        direction: Direction,
        click_count: u8,
    ) -> Result<(), BackendError> {
        let d = match direction {
            Direction::Press => rpa_native_input::Direction::Press,
            Direction::Release => rpa_native_input::Direction::Release,
        };
        self.driver
            .button(button, d, click_count)
            .map_err(|e| input_error("mouse button", e))
    }

    fn key(&mut self, key: Key, direction: Direction) -> Result<(), BackendError> {
        let d = match direction {
            Direction::Press => rpa_native_input::Direction::Press,
            Direction::Release => rpa_native_input::Direction::Release,
        };
        self.driver.key(key, d).map_err(|e| input_error("key", e))
    }

    fn text_scalar(&mut self, ch: char) -> Result<(), BackendError> {
        self.driver
            .text_scalar(ch)
            .map_err(|e| input_error("text input", e))
    }
}

fn input_error(context: &str, e: InputError) -> BackendError {
    // Preserve the driver's diagnostic code (permission_denied, no_display,
    // unsupported_session, invalid_text, ...) rather than flattening every
    // failure into input_failed; the protocol mapper in runtime/error.rs
    // maps unknown codes safely. Diagnostic context stays in the message.
    BackendError::new(e.code.to_owned(), format!("{context}: {e}"))
}

pub(crate) use crate::backend::dispatch_core::BackendCore;

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dispatch_cleanup_tests.rs"]
mod cleanup_tests;
