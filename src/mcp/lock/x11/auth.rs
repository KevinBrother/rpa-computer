//! Interim credential adapter, pending the coordinated common auth helper.
//!
//! IMPORTANT: size limits/O_NONBLOCK/deadline checks do NOT bound regular-file
//! open, metadata or read on a stalled filesystem. This is an explicit remaining
//! startup blocker, not part of the interruptible socket acquisition guarantee.
//! It runs before any X connection/task exists; a delayed read cannot acquire a
//! selection. Do not move it into the setup task and claim shutdown+join cancels it.
use super::{transport::check_deadline, unavailable, LockError};
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Instant;

pub(super) struct Cookie(pub Option<Vec<u8>>);
impl Drop for Cookie {
    fn drop(&mut self) {
        if let Some(bytes) = &mut self.0 {
            bytes.fill(0);
        }
    }
}
struct Bytes(Vec<u8>);
impl Drop for Bytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

pub(super) fn load(number: &str, deadline: Instant) -> Result<Cookie, LockError> {
    const LIMIT: usize = 1024 * 1024;
    check_deadline(deadline)?;
    let path = std::env::var_os("XAUTHORITY")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".Xauthority")));
    let Some(path) = path else {
        return Ok(Cookie(None));
    };
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Cookie(None)),
        Err(_) => return Err(unavailable("cannot open inherited Xauthority file")),
    };
    let metadata = file
        .metadata()
        .map_err(|_| unavailable("cannot inspect Xauthority file"))?;
    if !metadata.is_file() || metadata.len() > LIMIT as u64 {
        return Err(unavailable(
            "Xauthority must be a regular file no larger than 1 MiB",
        ));
    }
    // Preallocate the cap to avoid reallocations leaving copies of credentials.
    let mut bytes = Bytes(Vec::with_capacity(LIMIT));
    let mut buffer = [0u8; 4096];
    loop {
        check_deadline(deadline)?;
        let n = match file.read(&mut buffer) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(unavailable("cannot read Xauthority file")),
        };
        if n == 0 {
            break;
        }
        if bytes.0.len() + n > LIMIT {
            buffer.fill(0);
            return Err(unavailable("Xauthority exceeds 1 MiB"));
        }
        bytes.0.extend_from_slice(&buffer[..n]);
        buffer.fill(0);
    }
    let mut hostname = [0u8; 256];
    // SAFETY: correctly sized writable buffer. Never log authority records.
    if unsafe { libc::gethostname(hostname.as_mut_ptr().cast(), hostname.len()) } != 0 {
        return Err(unavailable("cannot determine local Xauthority hostname"));
    }
    let end = hostname
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| unavailable("local hostname is too long"))?;
    check_deadline(deadline)?;
    select(&bytes.0, &hostname[..end], number)
}

fn select(mut bytes: &[u8], hostname: &[u8], number: &str) -> Result<Cookie, LockError> {
    let mut result = Cookie(None);
    while !bytes.is_empty() {
        let family = u16_field(&mut bytes)?;
        let address = field(&mut bytes)?;
        let display = field(&mut bytes)?;
        let protocol = field(&mut bytes)?;
        let data = field(&mut bytes)?;
        if (family == 65535 || (family == 256 && address == hostname))
            && display == number.as_bytes()
            && protocol == b"MIT-MAGIC-COOKIE-1"
        {
            if data.len() != 16 {
                return Err(unavailable("invalid MIT-MAGIC-COOKIE-1 credential length"));
            }
            if result.0.is_none() {
                result.0 = Some(data.to_vec());
            }
        }
    }
    Ok(result)
}
fn u16_field(bytes: &mut &[u8]) -> Result<u16, LockError> {
    if bytes.len() < 2 {
        return Err(unavailable("truncated Xauthority record"));
    }
    let n = u16::from_be_bytes([bytes[0], bytes[1]]);
    *bytes = &bytes[2..];
    Ok(n)
}
fn field<'a>(bytes: &mut &'a [u8]) -> Result<&'a [u8], LockError> {
    let n = u16_field(bytes)? as usize;
    if bytes.len() < n {
        return Err(unavailable("truncated Xauthority field"));
    }
    let (value, rest) = bytes.split_at(n);
    *bytes = rest;
    Ok(value)
}
