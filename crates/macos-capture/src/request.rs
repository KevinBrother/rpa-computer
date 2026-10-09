use crate::{CaptureError, ErrorKind, Result, Stage};
use std::fmt;
use std::time::{Duration, Instant};

pub const MAX_DIMENSION: u32 = 16_384;
pub const MAX_PIXELS: u64 = 32 * 1024 * 1024;
pub const MAX_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_PNG_BYTES: usize = (MAX_PIXELS * 4 + 1024 * 1024) as usize;
pub(crate) const MAX_RAW_BYTES: usize = (MAX_PIXELS * 8 + 1024 * 1024) as usize;

#[derive(Debug, Clone, Copy)]
pub struct CaptureRequest {
    /// CoreGraphics/ScreenCaptureKit native display identifier, not a list index.
    pub display_id: u32,
    /// Trusted Host-owned child PID; never sourced from an Agent-supplied argument.
    pub excluded_process_id: u32,
    /// Requested output pixel dimensions, not display points.
    pub width: u32,
    pub height: u32,
    /// Absolute total budget for discovery, screenshot and result delivery.
    pub timeout: Duration,
}
impl CaptureRequest {
    pub fn validate(&self) -> Result<()> {
        if self.excluded_process_id == 0
            || self.excluded_process_id > i32::MAX as u32
            || self.timeout.is_zero()
            || self.timeout > MAX_TIMEOUT
        {
            return Err(CaptureError::new(
                ErrorKind::InvalidRequest,
                Stage::Validate,
            ));
        }
        validate_dimensions(self.width, self.height, ErrorKind::InvalidRequest)
    }
}

pub(crate) fn validate_dimensions(width: u32, height: u32, kind: ErrorKind) -> Result<()> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || pixels > MAX_PIXELS
    {
        return Err(CaptureError::new(kind, Stage::Validate));
    }
    Ok(())
}

pub struct CapturedImage {
    pub png: Vec<u8>,
    /// Actual CGImage dimensions verified against the encoded PNG IHDR.
    pub width: u32,
    pub height: u32,
}
impl fmt::Debug for CapturedImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapturedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("png_bytes", &self.png.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Deadline {
    expires: Instant,
}
impl Deadline {
    pub(crate) fn new_at(start: Instant, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() || timeout > MAX_TIMEOUT {
            return Err(CaptureError::new(
                ErrorKind::InvalidRequest,
                Stage::Validate,
            ));
        }
        Ok(Self {
            expires: start.checked_add(timeout).ok_or(CaptureError::new(
                ErrorKind::InvalidRequest,
                Stage::Validate,
            ))?,
        })
    }
    pub(crate) fn remaining_at(self, now: Instant) -> Option<Duration> {
        self.expires
            .checked_duration_since(now)
            .filter(|remaining| !remaining.is_zero())
    }
}

/// Production post-encode guard, not a standalone PNG decoder. ImageIO is the
/// encoder. The dimensions reported to Host must match that encoder's actual IHDR.
pub(crate) fn validate_png_header(
    png: &[u8],
    actual_width: u32,
    actual_height: u32,
) -> Result<(u32, u32)> {
    let error = CaptureError::new(ErrorKind::InvalidImage, Stage::Encode);
    if png.len() < 33
        || png.len() > MAX_PNG_BYTES
        || &png[..8] != b"\x89PNG\r\n\x1a\n"
        || &png[8..16] != b"\0\0\0\rIHDR"
    {
        return Err(error);
    }
    let width = u32::from_be_bytes(png[16..20].try_into().map_err(|_| error)?);
    let height = u32::from_be_bytes(png[20..24].try_into().map_err(|_| error)?);
    validate_dimensions(width, height, ErrorKind::InvalidImage)?;
    if width != actual_width || height != actual_height {
        return Err(error);
    }
    Ok((width, height))
}
