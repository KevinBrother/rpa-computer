//! Local socket acquisition before XCB setup. All socket waits have a deadline.
use super::auth;
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Instant;

pub(super) fn check_deadline(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("X11 clipboard initialization deadline exceeded".into())
    } else {
        Ok(())
    }
}

pub(super) fn open_socket(number: &str, deadline: Instant) -> Result<UnixStream, String> {
    let name = format!("/tmp/.X11-unix/X{number}");
    // Match Linux X11 conventions: abstract namespace first, filesystem next.
    let mut last = String::new();
    for abstract_namespace in [true, false] {
        check_deadline(deadline)?;
        match connect_unix(name.as_bytes(), abstract_namespace, deadline) {
            Ok(socket) => return Ok(socket),
            Err(error) => last = error,
        }
    }
    Err(format!("cannot connect to local X11 socket: {last}"))
}

fn connect_unix(
    name: &[u8],
    abstract_namespace: bool,
    deadline: Instant,
) -> Result<UnixStream, String> {
    // SAFETY: valid constants; the descriptor is immediately put in an owner.
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    // SAFETY: fresh owned descriptor from socket().
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: zero is valid for all fields; length below controls path encoding.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    if name.len() + 1 > address.sun_path.len() {
        return Err("X11 socket path is too long".into());
    }
    let offset = usize::from(abstract_namespace);
    for (dest, &byte) in address.sun_path[offset..].iter_mut().zip(name) {
        *dest = byte as libc::c_char;
    }
    let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + name.len() + 1;
    // SAFETY: pointer and length describe an initialized AF_UNIX address.
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
            return Err(error.to_string());
        }
        loop {
            check_deadline(deadline)?;
            let mut poll = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            let timeout = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, 100) as i32;
            // SAFETY: one live pollfd, bounded timeout, owned socket remains open.
            let ready = unsafe { libc::poll(&mut poll, 1, timeout) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.to_string());
            }
            if ready > 0 {
                break;
            }
        }
        let mut error: libc::c_int = 0;
        let mut size = std::mem::size_of_val(&error) as libc::socklen_t;
        // SAFETY: writable integer output with correct size, valid descriptor.
        let result = unsafe {
            libc::getsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut error as *mut libc::c_int).cast(),
                &mut size,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error).to_string());
        }
    }
    check_deadline(deadline)?;
    Ok(UnixStream::from(fd))
}

// KNOWN LIVENESS BLOCKER: O_NONBLOCK does not bound regular-file open/stat/read
// on a stalled NFS/FUSE backing store. These checks are cooperative, and this
// happens before the socket watchdog exists. Do not claim that socket shutdown
// interrupts arbitrary filesystem I/O. See BUILD_NOTES.md for the pending gate.
pub(super) fn load_cookie(number: &str, deadline: Instant) -> Result<Option<Vec<u8>>, String> {
    const MAX_AUTH_BYTES: usize = 1024 * 1024;
    check_deadline(deadline)?;
    let path = std::env::var_os("XAUTHORITY")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".Xauthority")));
    let Some(path) = path else {
        return Ok(None);
    };
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("cannot open inherited Xauthority file".into()),
    };
    // Reject FIFOs/devices rather than waiting indefinitely for credential input.
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect Xauthority file")?;
    if !metadata.is_file() || metadata.len() > MAX_AUTH_BYTES as u64 {
        return Err("Xauthority must be a regular file no larger than 1 MiB".into());
    }
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        check_deadline(deadline)?;
        let count = match file.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err("cannot read Xauthority file".into()),
        };
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > MAX_AUTH_BYTES {
            return Err("Xauthority exceeds 1 MiB".into());
        }
    }
    let mut hostname = [0u8; 256];
    // SAFETY: writable fixed-size byte buffer, size correctly supplied.
    if unsafe { libc::gethostname(hostname.as_mut_ptr().cast(), hostname.len()) } != 0 {
        return Err("cannot determine local Xauthority hostname".into());
    }
    let end = hostname
        .iter()
        .position(|&byte| byte == 0)
        .ok_or("local hostname is too long")?;
    check_deadline(deadline)?;
    let result = auth::select_cookie(&bytes, &hostname[..end], number);
    bytes.fill(0);
    result
}
