//! Fail-closed, exact-PID ScreenCaptureKit still capture. No input, UI, network,
//! permission request or fallback screenshot implementation.
#![forbid(unsafe_op_in_unsafe_fn)]

mod error;
mod flight;
#[cfg(target_os = "macos")]
mod native;
mod request;
mod selection;

use error::classify_native_error;
pub use error::{CaptureError, ErrorKind, NativeErrorDomain, Result, Stage};
use flight::FlightSlot;
use request::{validate_png_header, Deadline};
pub use request::{
    CaptureRequest, CapturedImage, MAX_DIMENSION, MAX_PIXELS, MAX_PNG_BYTES, MAX_TIMEOUT,
};
use std::time::Instant;

/// Available from macOS 14; checks OS version only, not TCC or screen contents.
pub fn is_supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        native::is_supported()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Keep one client for the lifetime of the Host capture lane. Do NOT construct a
/// fresh client for each timeout: old asynchronous calls cannot be cancelled by
/// this API. New is purely Rust allocation and makes no native capture/TCC call.
#[derive(Debug, Default)]
pub struct CaptureClient {
    flights: FlightSlot<CapturedImage>,
}
impl CaptureClient {
    pub fn new() -> Self {
        Self::default()
    }
    /// True while the OS callback is outstanding, including after a timed-out wait.
    pub fn is_busy(&self) -> bool {
        self.flights.is_busy()
    }
    /// Synchronous bounded wait; call on a Host worker, not the UI main thread.
    pub fn capture(&mut self, request: CaptureRequest) -> Result<CapturedImage> {
        let started = Instant::now();
        request.validate()?;
        let deadline = Deadline::new_at(started, request.timeout)?;
        // Busy is checked BEFORE permissions/discovery; no new native work on retry.
        if self.is_busy() {
            return Err(CaptureError::new(ErrorKind::Busy, Stage::Wait));
        }
        #[cfg(target_os = "macos")]
        {
            native::preflight()?;
            let flight = self.flights.begin(deadline)?;
            native::start(request, flight.clone());
            flight.wait()
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = deadline;
            Err(CaptureError::new(
                ErrorKind::Unsupported,
                Stage::Availability,
            ))
        }
    }
}

#[cfg(test)]
mod tests;
