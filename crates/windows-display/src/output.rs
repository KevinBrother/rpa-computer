use crate::{error::overflow, png, DisplayError};
use rpa_display_topology::PixelSize;

/// Exact image payload lengths for this crate's current stored-DEFLATE encoder.
/// Does NOT include JSON metadata/envelope, message framing, TLS overhead, or
/// Host buffers. It is a pure size estimate, not a transport admission check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodedSize {
    /// Exact PNG bytes, including chunks/checksums.
    pub png_bytes: u64,
    /// Exact padded standard-base64 ASCII byte count (4 * ceil(PNG_bytes/3)).
    /// Includes neither JSON string quotes nor a data-URL prefix.
    pub base64_bytes: u64,
}

/// Estimate output WITHOUT capture, pixel allocation, or encoding. Uses the same
/// PNG length authority as the real encoder. Host must include JSON/framing in
/// its own limit, and recheck actual output after any new topology/plan and before
/// allocating base64 or serializing/sending an MCP message.
pub fn estimate_encoded_size(size: PixelSize) -> Result<EncodedSize, DisplayError> {
    let png_bytes = png::encoded_len(size)?;
    let base64_bytes = png_bytes
        .checked_add(2)
        .map(|n| n / 3)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| overflow("base64 output size"))?;
    Ok(EncodedSize {
        png_bytes,
        base64_bytes,
    })
}
