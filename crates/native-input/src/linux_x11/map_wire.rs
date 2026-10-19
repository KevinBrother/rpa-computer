//! Validated action-empty native payload and post-submit error classification.
//! Portable policy; the Linux-only child uses the actual public XCB serializer.
#[cfg(target_os = "linux")]
use super::super::super::map_request::{check_descriptor, Changes, NativeMapRequest};
use super::super::Definition;
#[cfg(not(target_os = "linux"))]
use crate::linux_x11_map_request::{check_descriptor, Changes, NativeMapRequest};
use crate::{InputError, Result};

fn invalid(message: &'static str) -> InputError {
    InputError::new("input_failed", message)
}

/// Validate before any generated constructor narrows a count or any IO occurs.
/// Candidate policy only lends effect-free slots; never serialize unsupported
/// nonzero values as zero, even if a caller bypassed that earlier policy.
pub(crate) fn validate_definition(definition: &Definition) -> Result<()> {
    if !definition.actions.is_empty()
        || definition.behavior != [0; 2]
        || definition.modmap != 0
        || definition.vmodmap != 0
    {
        return Err(invalid(
            "native map writer requires empty actions/behavior/modifier maps",
        ));
    }
    validate_symbols(
        definition.group_info,
        definition.width,
        definition.symbols.iter().map(|symbol| *symbol as u64),
    )
}

pub(crate) fn validate_symbols(
    group_info: u8,
    width: u8,
    symbols: impl ExactSizeIterator<Item = u64>,
) -> Result<()> {
    let count = symbols.len();
    let groups = usize::from(group_info & 0x0f);
    if count > usize::from(u16::MAX) || groups > 4 || count != groups * usize::from(width) {
        return Err(invalid(
            "invalid native SetMap symbol count/group/width geometry",
        ));
    }
    // Header36 + symbol descriptor8 + padded action-count4 + explicit pair
    // padded to4. No nonempty modifier/behavior lists exist in this subset.
    let bytes = count.checked_mul(4).and_then(|bytes| bytes.checked_add(52));
    if bytes.is_none_or(|bytes| bytes / 4 > usize::from(u16::MAX)) {
        return Err(invalid("native SetMap request exceeds protocol length"));
    }
    if symbols
        .into_iter()
        .any(|symbol| u32::try_from(symbol).is_err())
    {
        return Err(invalid("native keysym does not fit protocol CARD32"));
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum Payload<'a> {
    Full(&'a Definition),
    Explicit(u8),
}

/// Only `prepare` can construct this: typed request provenance and exact ranges
/// are validated before the public serializer sees counts/payloads.
pub(crate) struct PreparedMap<'a> {
    request: &'a NativeMapRequest,
    payload: Payload<'a>,
}
impl PreparedMap<'_> {
    pub fn request(&self) -> &NativeMapRequest {
        self.request
    }
    pub fn payload(&self) -> Payload<'_> {
        self.payload
    }
    pub fn flags(&self) -> u16 {
        0
    } // no recompute, no resize-types
}
pub(crate) fn prepare<'a>(
    request: &'a NativeMapRequest,
    payload: Payload<'a>,
) -> Result<PreparedMap<'a>> {
    let explicit = matches!(payload, Payload::Explicit(_));
    let range = (request.changes.min_key_code, request.changes.max_key_code);
    check_descriptor(request, request.device_id, range, explicit)?;
    if range.0 < 8 || range.0 > range.1 {
        return Err(invalid("invalid native SetMap keycode range"));
    }
    let expected = if explicit {
        Changes::explicit_only(range.0, range.1, request.key)
    } else {
        Changes::full_key(range.0, range.1, request.key)
    };
    if request.changes != expected {
        return Err(invalid(
            "native SetMap request does not have exact single-key ranges",
        ));
    }
    if let Payload::Full(definition) = payload {
        validate_definition(definition)?;
    }
    Ok(PreparedMap { request, payload })
}

/// Classify by Result variants, never diagnostics text. check_request's null
/// error pointer is not success when transport health is unknown. All callers
/// retain their existing pre-submit journal obligation on either error.
pub(crate) fn complete(request: Result<()>, connection: Result<()>) -> Result<()> {
    match connection {
        Err(mut transport) => {
            if let Err(protocol) = request {
                transport
                    .message
                    .push_str(&format!("; request also failed: {protocol}"));
            }
            Err(transport)
        }
        Ok(()) => request,
    }
}

#[cfg(target_os = "linux")]
#[path = "map_xcb.rs"]
pub(crate) mod xcb;

#[cfg(test)]
#[path = "map_wire_tests.rs"]
mod map_wire_tests;
