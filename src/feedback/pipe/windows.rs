//! Private byte-mode named pipes with PIPE_NOWAIT on the Host ends only.
//! Child sees ordinary blocking stdio. Host polling never blocks in ReadFile
//! or WriteFile, including a renderer which refuses to drain stdin.
use std::{
    ffi::c_void,
    fs::File,
    io::{self, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
    process::{Child, Command, Stdio},
};
type Handle = *mut c_void;
const INVALID: Handle = -1isize as Handle;
const PIPE_NOWAIT: u32 = 1;
const FIRST_INSTANCE: u32 = 0x0008_0000;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const ERROR_NO_DATA: i32 = 232;
const ERROR_BROKEN_PIPE: i32 = 109;
const ERROR_PIPE_CONNECTED: i32 = 535;

#[link(name = "kernel32")]
extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        access: u32,
        mode: u32,
        instances: u32,
        out_size: u32,
        in_size: u32,
        timeout: u32,
        security: *const super::windows_security::Attributes,
    ) -> Handle;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
    fn ReadFile(
        handle: Handle,
        data: *mut c_void,
        len: u32,
        read: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn WriteFile(
        handle: Handle,
        data: *const c_void,
        len: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}
pub(crate) struct Reader(OwnedHandle);
pub(crate) struct Writer(OwnedHandle);
impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut n = 0;
        // SAFETY: live owned handle; initialized output length, writable buffer.
        let ok = unsafe {
            ReadFile(
                self.0.as_raw_handle(),
                buf.as_mut_ptr().cast(),
                buf.len().min(u32::MAX as usize) as u32,
                &mut n,
                std::ptr::null_mut(),
            )
        };
        if ok != 0 {
            return Ok(n as usize);
        }
        let e = io::Error::last_os_error();
        match e.raw_os_error() {
            Some(ERROR_NO_DATA) => Err(io::ErrorKind::WouldBlock.into()),
            Some(ERROR_BROKEN_PIPE) => Ok(0),
            _ => Err(e),
        }
    }
}
impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut n = 0;
        // Byte-mode PIPE_NOWAIT writes as much as fits, possibly zero. Zero is
        // backpressure, NOT an EOF; SnapshotWriter must preserve the old frame.
        let ok = unsafe {
            WriteFile(
                self.0.as_raw_handle(),
                buf.as_ptr().cast(),
                buf.len().min(u32::MAX as usize) as u32,
                &mut n,
                std::ptr::null_mut(),
            )
        };
        if ok != 0 && n > 0 {
            return Ok(n as usize);
        }
        if ok != 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(ERROR_NO_DATA) {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Err(e)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn pair(host_reads: bool) -> io::Result<(OwnedHandle, File)> {
    let nonce = super::windows_security::nonce()?;
    let security = super::windows_security::Security::new()?;
    let name: Vec<u16> = format!(r"\\.\pipe\rpa-feedback-{}-{}", std::process::id(), nonce)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // Handles are non-inheritable. Stdio duplicates only the explicit child
    // endpoint; no tokens or desktop access handles are inherited here.
    let host = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            FIRST_INSTANCE | if host_reads { 1 } else { 2 },
            PIPE_NOWAIT | 8,
            1,
            16_384,
            16_384,
            0,
            &security.attributes,
        )
    };
    if host == INVALID {
        return Err(io::Error::last_os_error());
    }
    let host = unsafe { OwnedHandle::from_raw_handle(host as RawHandle) };
    let child = unsafe {
        CreateFileW(
            name.as_ptr(),
            if host_reads {
                GENERIC_WRITE
            } else {
                GENERIC_READ
            },
            0,
            std::ptr::null(),
            3,
            0,
            std::ptr::null_mut(),
        )
    };
    if child == INVALID {
        return Err(io::Error::last_os_error());
    }
    let child = unsafe { File::from_raw_handle(child as RawHandle) };
    if unsafe { ConnectNamedPipe(host.as_raw_handle(), std::ptr::null_mut()) } == 0
        && io::Error::last_os_error().raw_os_error() != Some(ERROR_PIPE_CONNECTED)
    {
        return Err(io::Error::last_os_error());
    }
    Ok((host, child))
}
pub(crate) fn spawn(command: &mut Command) -> io::Result<(Child, Reader, Writer)> {
    super::windows_desktop::verify()?;
    let (input, child_input) = pair(false)?;
    let (output, child_output) = pair(true)?;
    let child = command
        .stdin(Stdio::from(child_input))
        .stdout(Stdio::from(child_output))
        .spawn()?;
    Ok((child, Reader(output), Writer(input)))
}
