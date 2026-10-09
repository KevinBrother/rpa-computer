use std::{
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    process::{Child, Command, Stdio},
};

pub(crate) fn spawn(command: &mut Command) -> io::Result<(Child, File, File)> {
    // Configure all Host descriptors BEFORE spawn. A configuration failure
    // therefore never leaves a child whose reap status would be unknown.
    let (child_input, input) = pair()?;
    let (output, child_output) = pair()?;
    nonblocking(input.as_raw_fd())?;
    nonblocking(output.as_raw_fd())?;
    let child = command
        .stdin(Stdio::from(child_input))
        .stdout(Stdio::from(child_output))
        .spawn()?;
    Ok((child, output, input))
}
fn pair() -> io::Result<(File, File)> {
    let mut fds = [-1; 2];
    // SAFETY: two writable fd slots; pipe success transfers both descriptors.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    for fd in [read.as_raw_fd(), write.as_raw_fd()] {
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok((File::from(read), File::from(write)))
}
fn nonblocking(fd: std::os::fd::RawFd) -> io::Result<()> {
    // SAFETY: live owned Host endpoint; child endpoint remains blocking.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
