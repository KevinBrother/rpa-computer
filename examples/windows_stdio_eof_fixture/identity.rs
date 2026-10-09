//! Windows-only identity helpers shared by the two standalone test executables.
//! BCrypt hashes executable bytes; no subprocess, shell, guessed filename or new dependency.
use std::ffi::c_void;
use std::io;
use std::os::windows::io::RawHandle;
use std::path::Path;

pub const SCHEMA: &str = "windows-stdio-eof-v1";
pub const BUILD_ID: &str = match option_env!("RPA_STDIO_EOF_BUILD_ID") {
    Some(value) => value,
    None => "UNFROZEN_BUILD",
};
pub fn version() -> String {
    format!(
        "rpa-computer/{};{SCHEMA};{BUILD_ID}",
        env!("CARGO_PKG_VERSION")
    )
}
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptOpenAlgorithmProvider(
        handle: *mut *mut c_void,
        algorithm: *const u16,
        implementation: *const u16,
        flags: u32,
    ) -> i32;
    fn BCryptHash(
        handle: *mut c_void,
        secret: *const u8,
        secret_len: u32,
        input: *const u8,
        input_len: u32,
        output: *mut u8,
        output_len: u32,
    ) -> i32;
    fn BCryptCloseAlgorithmProvider(handle: *mut c_void, flags: u32) -> i32;
}
struct Algorithm(*mut c_void);
impl Drop for Algorithm {
    fn drop(&mut self) {
        unsafe {
            BCryptCloseAlgorithmProvider(self.0, 0);
        }
    }
}
fn nt_status(value: i32) -> io::Result<()> {
    if value >= 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!("BCrypt NTSTATUS {value:#x}")))
    }
}
pub fn executable_sha256(path: &Path) -> io::Result<String> {
    let metadata = path.metadata()?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return Err(io::Error::other(
            "executable must be a regular file <=128 MiB",
        ));
    }
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(b"MZ") {
        return Err(io::Error::other("not a Windows PE executable"));
    }
    let mut handle = std::ptr::null_mut();
    let name: Vec<u16> = "SHA256\0".encode_utf16().collect();
    // All buffers live for the complete synchronous call; SHA256 needs no key.
    unsafe {
        nt_status(BCryptOpenAlgorithmProvider(
            &mut handle,
            name.as_ptr(),
            std::ptr::null(),
            0,
        ))?;
    }
    let algorithm = Algorithm(handle);
    let mut digest = [0u8; 32];
    unsafe {
        nt_status(BCryptHash(
            algorithm.0,
            std::ptr::null(),
            0,
            bytes.as_ptr(),
            u32::try_from(bytes.len()).map_err(io::Error::other)?,
            digest.as_mut_ptr(),
            32,
        ))?;
    }
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}
#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> RawHandle;
    fn QueryPerformanceCounter(value: *mut i64) -> i32;
    fn QueryPerformanceFrequency(value: *mut i64) -> i32;
    fn GetProcessTimes(
        process: RawHandle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
    fn QueryFullProcessImageNameW(
        process: RawHandle,
        flags: u32,
        buffer: *mut u16,
        size: *mut u32,
    ) -> i32;
}
/// Caller retains the live owned Child handle (or the current-process pseudo-handle).
pub fn process_identity(handle: RawHandle, pid: u32) -> io::Result<serde_json::Value> {
    let (mut creation, mut exit, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    let mut session = 0;
    let mut image = vec![0u16; 32768];
    let mut length = image.len() as u32;
    unsafe {
        if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) == 0
            || ProcessIdToSessionId(pid, &mut session) == 0
            || QueryFullProcessImageNameW(handle, 0, image.as_mut_ptr(), &mut length) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(
        serde_json::json!({"pid":pid, "creation_filetime": (u64::from(creation.high)<<32)|u64::from(creation.low), "session_id":session, "image":String::from_utf16_lossy(&image[..length as usize])}),
    )
}
pub fn current_identity() -> io::Result<serde_json::Value> {
    // Pseudo-handle is borrowed and must not be closed.
    process_identity(unsafe { GetCurrentProcess() }, std::process::id())
}

/// Machine-wide monotonic clock: permits parent ACK/child Press ordering without
/// assuming synchronized wall clocks or measuring only diagnostic receipt delay.
pub fn qpc_ticks() -> io::Result<u64> {
    let mut value = 0i64;
    if unsafe { QueryPerformanceCounter(&mut value) } == 0 {
        return Err(io::Error::last_os_error());
    }
    u64::try_from(value).map_err(io::Error::other)
}
pub fn qpc_frequency() -> io::Result<u64> {
    let mut value = 0i64;
    if unsafe { QueryPerformanceFrequency(&mut value) } == 0 || value <= 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(value as u64)
}
