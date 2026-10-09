//! Conservative pre-capture payload envelope, independent of topology races.
use super::{error, BackendError};
use rpa_display_topology::{CaptureBudget, PixelSize};

// Half the existing 32MiB session cache; images above this cannot enter root.
// Leave ample room under the unchanged 64MiB MCP cap for base64, metadata, ID.
pub const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_OUTPUT_PIXELS: u64 = ((MAX_PNG_BYTES - 128) / 6) as u64;
pub fn bounded(b: CaptureBudget) -> Result<CaptureBudget, BackendError> {
    CaptureBudget::new(
        u64::from(b.max_width()),
        u64::from(b.max_height()),
        b.max_pixels().min(MAX_OUTPUT_PIXELS),
    )
    .map_err(error)
}
pub fn requested(width: u32, height: u32) -> Result<CaptureBudget, BackendError> {
    bounded(
        CaptureBudget::new(
            u64::from(width),
            u64::from(height),
            u64::from(width) * u64::from(height),
        )
        .map_err(error)?,
    )
}
pub fn check_estimate(size: PixelSize) -> Result<(), BackendError> {
    let estimate = rpa_windows_display::estimate_encoded_size(size).map_err(error)?;
    check_png(usize::try_from(estimate.png_bytes).map_err(error)?)
}
pub fn check_png(bytes: usize) -> Result<(), BackendError> {
    if bytes > MAX_PNG_BYTES {
        Err(BackendError::new(
            "output_budget",
            "PNG exceeds the bounded observation payload budget",
        ))
    } else {
        Ok(())
    }
}
