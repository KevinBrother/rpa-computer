//! Polling pipes. Both endpoints used by the Host are genuinely nonblocking;
//! no blocked reader/writer helper threads exist to abandon at teardown.
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_desktop;
#[cfg(windows)]
mod windows_security;
#[cfg(unix)]
pub(crate) use unix::spawn;
#[cfg(windows)]
pub(crate) use windows::spawn;
