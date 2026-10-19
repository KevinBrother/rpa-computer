//! Parent-only nonblocking private transport. Every retry shares one deadline;
//! no read, write, poll or completion wait uses a fresh budget.
use crate::x11_credentials::{
    ipc::{AuthorityRead, ReplyGate},
    FailureKind,
};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::time::Instant;

pub(super) fn live(deadline: Instant) -> Result<(), FailureKind> {
    if Instant::now() >= deadline {
        Err(FailureKind::Timeout)
    } else {
        Ok(())
    }
}
fn ready(socket: &UnixStream, events: i16, deadline: Instant) -> Result<(), FailureKind> {
    loop {
        live(deadline)?;
        let mut poll = libc::pollfd {
            fd: socket.as_raw_fd(),
            events,
            revents: 0,
        };
        // Floor, not ceil: the absolute deadline is checked again on every turn.
        let millis = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(25) as i32;
        let result = unsafe { libc::poll(&mut poll, 1, millis) };
        live(deadline)?;
        if result < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(FailureKind::ReadFailed);
        }
        if poll.revents & libc::POLLNVAL != 0 {
            return Err(FailureKind::ReadFailed);
        }
        if result > 0 {
            return Ok(());
        }
    }
}
pub(super) fn read(
    socket: &mut UnixStream,
    bytes: &mut [u8],
    deadline: Instant,
) -> Result<usize, FailureKind> {
    loop {
        live(deadline)?;
        match socket.read(bytes) {
            Ok(n) => {
                live(deadline)?;
                return Ok(n);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                ready(socket, libc::POLLIN, deadline)?
            }
            Err(_) => return Err(FailureKind::ReadFailed),
        }
    }
}
pub(super) fn byte(
    socket: &mut UnixStream,
    expected: u8,
    deadline: Instant,
) -> Result<(), FailureKind> {
    let mut byte = [0];
    if read(socket, &mut byte, deadline)? != 1 || byte[0] != expected {
        return Err(FailureKind::ChildSetupFailed);
    }
    Ok(())
}
pub(super) enum WritePoint {
    Before,
    After,
}
pub(super) fn allow(
    socket: &mut UnixStream,
    deadline: Instant,
    mut observe: impl FnMut(WritePoint),
) -> Result<(), FailureKind> {
    loop {
        live(deadline)?;
        observe(WritePoint::Before);
        match socket.write(&[0xa5]) {
            Ok(1) => {
                observe(WritePoint::After);
                return live(deadline);
            }
            Ok(_) => return Err(FailureKind::ReadFailed),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                ready(socket, libc::POLLOUT, deadline)?
            }
            Err(_) => return Err(FailureKind::ReadFailed),
        }
    }
}
pub(super) fn receive(
    socket: &mut UnixStream,
    deadline: Instant,
) -> Result<AuthorityRead, FailureKind> {
    let mut gate = ReplyGate::new(deadline);
    let mut bytes = [0u8; 8192];
    loop {
        let read = read(socket, &mut bytes, deadline);
        match read {
            Ok(0) => return gate.finish(Instant::now()),
            Ok(n) => {
                let result = gate.ingest(&bytes[..n], Instant::now());
                bytes.fill(0);
                result?;
            }
            Err(error) => {
                bytes.fill(0);
                return Err(error);
            }
        }
    }
}
