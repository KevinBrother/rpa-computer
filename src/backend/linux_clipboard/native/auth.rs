//! Bounded, binary-safe parsing; no auth material appears in errors or logs.

pub(super) fn local_display_number(display: &str) -> Result<String, String> {
    let suffix = display.strip_prefix("unix/:")
        .or_else(|| display.strip_prefix("unix:"))
        .or_else(|| display.strip_prefix(':'))
        .ok_or("X11 clipboard supports only local Unix DISPLAY (:N[.S]); TCP/SSH forwarding is unsupported")?;
    let (number, screen) = suffix
        .split_once('.')
        .map_or((suffix, None), |(n, s)| (n, Some(s)));
    let decimal = |value: &str| !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit());
    if !decimal(number) || screen.is_some_and(|s| !decimal(s)) {
        return Err("malformed local X11 DISPLAY".into());
    }
    let number: u16 = number
        .parse()
        .map_err(|_| "X11 display number is out of range")?;
    Ok(number.to_string())
}

pub(super) fn select_cookie(
    bytes: &[u8],
    hostname: &[u8],
    display: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut rest = bytes;
    let mut selected = None;
    while !rest.is_empty() {
        let family = read_u16(&mut rest)?;
        let address = field(&mut rest)?;
        let number = field(&mut rest)?;
        let name = field(&mut rest)?;
        let data = field(&mut rest)?;
        let local = family == 65535 || (family == 256 && address == hostname);
        if local && number == display.as_bytes() && name == b"MIT-MAGIC-COOKIE-1" {
            if data.len() != 16 {
                return Err("invalid MIT-MAGIC-COOKIE-1 credential length in Xauthority".into());
            }
            if selected.is_none() {
                selected = Some(data.to_vec());
            }
        }
    }
    Ok(selected)
}

fn read_u16(rest: &mut &[u8]) -> Result<u16, String> {
    if rest.len() < 2 {
        return Err("truncated Xauthority record".into());
    }
    let value = u16::from_be_bytes([rest[0], rest[1]]);
    *rest = &rest[2..];
    Ok(value)
}

fn field<'a>(rest: &mut &'a [u8]) -> Result<&'a [u8], String> {
    let size = usize::from(read_u16(rest)?);
    if rest.len() < size {
        return Err("truncated Xauthority field".into());
    }
    let (value, next) = rest.split_at(size);
    *rest = next;
    Ok(value)
}
