//! Parent-only bounded Xauthority parser. No environment, filesystem or MCP.
use super::{display, Cookie, FailureKind, MAX_AUTH_BYTES, MAX_HOSTNAME_BYTES};

pub(super) fn select_cookie(
    bytes: &[u8],
    hostname: &[u8],
    display_number: &str,
) -> Result<Option<Cookie>, FailureKind> {
    if bytes.len() > MAX_AUTH_BYTES {
        return Err(FailureKind::Oversized);
    }
    if hostname.len() > MAX_HOSTNAME_BYTES {
        return Err(FailureKind::InvalidRequest);
    }
    let number = display::parse_display_number(display_number)?.to_string();
    let mut rest = bytes;
    let mut selected = None;
    while !rest.is_empty() {
        let family = read_u16(&mut rest)?;
        let address = field(&mut rest)?;
        let display = field(&mut rest)?;
        let protocol = field(&mut rest)?;
        let data = field(&mut rest)?;
        if (family == 65535 || (family == 256 && address == hostname))
            && display == number.as_bytes()
            && protocol == b"MIT-MAGIC-COOKIE-1"
        {
            let bytes: [u8; 16] = data
                .try_into()
                .map_err(|_| FailureKind::MalformedAuthority)?;
            if selected.is_none() {
                selected = Some(Cookie(bytes));
            }
        }
        // Continue validating even after finding the first matching record.
    }
    Ok(selected)
}
fn read_u16(rest: &mut &[u8]) -> Result<u16, FailureKind> {
    if rest.len() < 2 {
        return Err(FailureKind::MalformedAuthority);
    }
    let n = u16::from_be_bytes([rest[0], rest[1]]);
    *rest = &rest[2..];
    Ok(n)
}
fn field<'a>(rest: &mut &'a [u8]) -> Result<&'a [u8], FailureKind> {
    let n = usize::from(read_u16(rest)?);
    if rest.len() < n {
        return Err(FailureKind::MalformedAuthority);
    }
    let (field, tail) = rest.split_at(n);
    *rest = tail;
    Ok(field)
}
