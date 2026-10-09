//! Enabled-only preflight: Session 0 / non-input desktop is never accepted.
use std::{ffi::c_void, io};
type Handle = *mut c_void;
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcessId() -> u32;
    fn GetCurrentThreadId() -> u32;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> Handle;
    fn GetThreadDesktop(thread: u32) -> Handle;
    fn CloseDesktop(desktop: Handle) -> i32;
    fn GetUserObjectInformationW(
        object: Handle,
        index: i32,
        data: *mut c_void,
        size: u32,
        needed: *mut u32,
    ) -> i32;
}
struct InputDesktop(Handle);
impl Drop for InputDesktop {
    fn drop(&mut self) {
        unsafe {
            CloseDesktop(self.0);
        }
    }
}
pub(super) fn verify() -> io::Result<()> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 || session == 0 {
        return Err(io::Error::other(
            "feedback requires an interactive console session",
        ));
    }
    let input = unsafe { OpenInputDesktop(0, 0, 1) };
    if input.is_null() {
        return Err(io::Error::last_os_error());
    }
    let input = InputDesktop(input);
    let own = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
    if own.is_null() || name(own)? != name(input.0)? {
        return Err(io::Error::other(
            "feedback requires the user's active input desktop",
        ));
    }
    Ok(())
}
fn name(handle: Handle) -> io::Result<Vec<u16>> {
    let mut buf = [0u16; 256];
    let mut needed = 0;
    if unsafe { GetUserObjectInformationW(handle, 2, buf.as_mut_ptr().cast(), 512, &mut needed) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    let end = buf
        .iter()
        .position(|c| *c == 0)
        .ok_or_else(|| io::Error::other("desktop name exceeds bound"))?;
    Ok(buf[..end].to_vec())
}
