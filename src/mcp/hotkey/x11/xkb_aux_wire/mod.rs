//! Safe bytes-only full Controls/Compat configuration normalization.
//! The 1MiB guard below is POST-wait only; it does not bound libxcb's preceding
//! allocation/read. The registration-owned transport watchdog bounds blocking.
mod decode;
#[cfg(test)]
mod tests;

/// Exact config bytes in protocol field order: offsets8..15,16..46,48..92.
/// Only seq/length/response and protocol padding excluded. All16/32-bit fields
/// remain native wire byte order on this connection; actions remain opaque.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Controls {
    pub device: u8,
    pub fields: [u8; 81],
}
impl Controls {
    pub fn groups(&self) -> u8 {
        self.fields[1]
    }
    pub fn internal_mask(&self) -> u8 {
        self.fields[3]
    }
    pub fn ignore_lock_mask(&self) -> u8 {
        self.fields[4]
    }
    pub fn enabled(&self) -> u32 {
        u32::from_ne_bytes(self.fields[45..49].try_into().unwrap())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Compatibility {
    pub device: u8,
    pub interpretations: Vec<[u8; 16]>,
    pub groups: [[u8; 4]; 4],
}

// Only these concrete XCB-owned reply wrappers may create byte slices; unit
// tests supply safe slices to the exact downstream production decode functions.
fn owned_bytes<R: xcb::Reply>(reply: &R) -> Result<&[u8], String> {
    // SAFETY: a live owned libxcb reply includes its fixed8-byte header.
    let header = unsafe { std::slice::from_raw_parts(reply.as_raw(), 8) };
    let size = usize::try_from(u32::from_ne_bytes(header[4..8].try_into().unwrap()))
        .ok()
        .and_then(|words| words.checked_mul(4))
        .and_then(|n| n.checked_add(32))
        .filter(|n| *n <= 1024 * 1024)
        .ok_or("auxiliary XKB post-wait reply cap exceeded")?;
    // SAFETY: libxcb owns the declared complete wire allocation. Byte reads
    // impose no alignment and the borrowed reply outlives this slice.
    Ok(unsafe { std::slice::from_raw_parts(reply.as_raw(), size) })
}
pub(super) fn controls_from_reply(reply: &xcb::xkb::GetControlsReply) -> Result<Controls, String> {
    decode_controls(owned_bytes(reply)?)
}
pub(super) fn compatibility_from_reply(
    reply: &xcb::xkb::GetCompatMapReply,
) -> Result<Compatibility, String> {
    decode_compatibility(owned_bytes(reply)?)
}
pub(super) fn decode_controls(bytes: &[u8]) -> Result<Controls, String> {
    decode::controls(bytes)
}
pub(super) fn decode_compatibility(bytes: &[u8]) -> Result<Compatibility, String> {
    decode::compatibility(bytes)
}
