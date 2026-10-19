//! Bounded owned-reply byte decoding; full-request policy is explicit.
//! Never call GetMapReply::map(): xcb 1.7.1's multi-bitcase decoder is unsafe.
mod components;
mod cursor;
mod header;
mod model;
mod parser;
pub(super) use model::*;
#[cfg(test)]
mod review_fixture;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod tests;

use xcb::Reply;
// Post-wait parser cap ONLY: libxcb already allocated/read the owned reply.
// This does not bound libxcb allocation or wait_for_reply latency.
const MAX_REPLY_BYTES: usize = 1024 * 1024;

/// X11 length is words AFTER the first 32 bytes; GetMap's fixed header is 40.
/// Arithmetic/cap checks must precede constructing the payload slice.
pub(super) fn reply_size(words: u32) -> Result<usize, String> {
    let size = usize::try_from(words)
        .ok()
        .and_then(|v| v.checked_mul(4))
        .and_then(|v| v.checked_add(32))
        .ok_or("XKB GetMap declared length overflow")?;
    if !(40..=MAX_REPLY_BYTES).contains(&size) {
        return Err("XKB GetMap declared length outside bounded reply range".into());
    }
    Ok(size)
}

/// Only accepts a real XCB-owned reply, never a caller-provided raw pointer.
/// libxcb allocates the complete wire reply (32 + length*4 bytes); its lifetime
/// remains borrowed throughout decoding. Unit fixtures use only safe byte-slice
/// decode/decode_full APIs, never fabricated Reply allocations.
pub(super) fn from_reply(reply: &xcb::xkb::GetMapReply) -> Result<Map, String> {
    // SAFETY: Reply::as_raw points into this live libxcb reply's fixed header.
    let header = unsafe { std::slice::from_raw_parts(reply.as_raw(), 8) };
    let size = reply_size(u32::from_ne_bytes(header[4..8].try_into().unwrap()))?;
    // SAFETY: the live owned libxcb allocation contains its declared wire length;
    // size has been checked above. This is a byte slice: no alignment casts,
    // generated bitcase accessors, trailing libxcb data or mutable aliases.
    let bytes = unsafe { std::slice::from_raw_parts(reply.as_raw(), size) };
    decode_full(bytes)
}

/// Production full-request policy. The SAME structural reader is used below,
/// with header coverage checked for full=all/virtual_mods=all before variable
/// components are allocated. Presence bits alone are not proof of coverage.
pub(super) fn decode_full(bytes: &[u8]) -> Result<Map, String> {
    parser::read(bytes, true)
}

/// Structural entry for packed-subset/partial-range protocol fixtures. Shares
/// the production parser, but does not grant full-request coverage. Production
/// from_reply must call decode_full; downstream semantics are checked separately.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn decode(bytes: &[u8]) -> Result<Map, String> {
    parser::read(bytes, false)
}
