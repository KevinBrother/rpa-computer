//! Local Unix transport acquired before handing its owned fd to XCB.
//! No DNS, TCP fallback, filesystem file reads, or detached connect tasks.
use super::{unavailable, LockError};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::time::Instant;

pub(super) fn check_deadline(deadline: Instant) -> Result<(), LockError> {
    if Instant::now() >= deadline {
        Err(unavailable("X11 ownership setup exceeded its deadline"))
    } else {
        Ok(())
    }
}

pub(super) fn display_number(display: &str) -> Result<String, LockError> {
    let suffix = display
        .strip_prefix("unix/:")
        .or_else(|| display.strip_prefix("unix:"))
        .or_else(|| display.strip_prefix(':'))
        .ok_or_else(|| unavailable("X11 ownership requires local Unix DISPLAY (:N[.S]); TCP/SSH forwarding is unsupported"))?;
    let (number, screen) = suffix
        .split_once('.')
        .map_or((suffix, None), |(n, s)| (n, Some(s)));
    let decimal = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !decimal(number) || screen.is_some_and(|s| !decimal(s)) {
        return Err(unavailable("malformed local X11 DISPLAY"));
    }
    // Core XCB accepts an int display number; fixture endpoints also use high
    // numbers. Screen does not affect this server-wide lease's identity.
    let number: i32 = number
        .parse()
        .map_err(|_| unavailable("X11 display number is out of range"))?;
    Ok(number.to_string())
}

pub(super) fn open(number: &str, deadline: Instant) -> Result<UnixStream, LockError> {
    let name = format!("/tmp/.X11-unix/X{number}");
    let mut last = String::new();
    for abstract_namespace in [true, false] {
        check_deadline(deadline)?;
        match connect(name.as_bytes(), abstract_namespace, deadline) {
            Ok(socket) => return Ok(socket),
            Err(error) => last = error.to_string(),
        }
    }
    check_deadline(deadline)?;
    Err(unavailable(format!(
        "cannot connect to local X11 socket: {last}"
    )))
}

fn connect(
    name: &[u8],
    abstract_namespace: bool,
    deadline: Instant,
) -> Result<UnixStream, LockError> {
    // SAFETY: fresh AF_UNIX descriptor, immediately given an RAII owner.
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if fd < 0 {
        return Err(unavailable(io::Error::last_os_error()));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: zeroed sockaddr_un is valid; length below specifies the address.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    if name.len() + 1 > address.sun_path.len() {
        return Err(unavailable("X11 socket path too long"));
    }
    let offset = usize::from(abstract_namespace);
    for (dest, &byte) in address.sun_path[offset..].iter_mut().zip(name) {
        *dest = byte as libc::c_char;
    }
    let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + name.len() + 1;
    // SAFETY: live owned socket and correctly sized initialized address.
    let result = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            length as libc::socklen_t,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(unavailable(error));
        }
        loop {
            check_deadline(deadline)?;
            let mut poll = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            let millis = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, 50) as i32;
            // SAFETY: one live pollfd, bounded wait, fd remains owned here.
            let ready = unsafe { libc::poll(&mut poll, 1, millis) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(unavailable(error));
            }
            if ready > 0 {
                break;
            }
        }
        let mut error: libc::c_int = 0;
        let mut size = std::mem::size_of_val(&error) as libc::socklen_t;
        // SAFETY: integer output and size match SO_ERROR's ABI.
        if unsafe {
            libc::getsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut error as *mut libc::c_int).cast(),
                &mut size,
            )
        } != 0
        {
            return Err(unavailable(io::Error::last_os_error()));
        }
        if error != 0 {
            return Err(unavailable(io::Error::from_raw_os_error(error)));
        }
    }
    check_deadline(deadline)?;
    Ok(UnixStream::from(fd))
}

pub(super) fn nonce(deadline: Instant) -> Result<[u8; 16], LockError> {
    let mut bytes = [0u8; 16];
    let mut offset = 0;
    while offset < bytes.len() {
        check_deadline(deadline)?;
        // SAFETY: writable remaining slice. GRND_NONBLOCK fails closed if the
        // kernel RNG is not ready; no /dev file open or blocking entropy wait.
        let count = unsafe {
            libc::getrandom(
                bytes[offset..].as_mut_ptr().cast(),
                bytes.len() - offset,
                libc::GRND_NONBLOCK,
            )
        };
        if count < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(unavailable(format!("cannot generate lease nonce: {error}")));
        }
        if count == 0 {
            return Err(unavailable("kernel returned no lease entropy"));
        }
        offset += count as usize;
    }
    Ok(bytes)
}
