//! Bounded decimal input boundary; not the X11 socket/connection implementation.
//! Checked c_int arithmetic, not a u16 display-number ceiling.
use super::FailureKind;
use std::ffi::c_int;

/// Resource/input bound, not a claim that X11 display numbers are 16-bit.
/// Allows leading zeros; normalized output is the resulting nonnegative c_int.
pub(super) const MAX_DISPLAY_DIGITS: usize = 64;

pub(super) fn parse_display_number(input: &str) -> Result<c_int, FailureKind> {
    if input.is_empty() || input.len() > MAX_DISPLAY_DIGITS {
        return Err(FailureKind::InvalidRequest);
    }
    let mut number: c_int = 0;
    for byte in input.bytes() {
        if !byte.is_ascii_digit() {
            return Err(FailureKind::InvalidRequest);
        }
        number = number
            .checked_mul(10)
            .and_then(|n| n.checked_add(c_int::from(byte - b'0')))
            .ok_or(FailureKind::InvalidRequest)?;
    }
    Ok(number)
}
