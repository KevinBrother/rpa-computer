//! Exact handles only: PID from owned child evidence is verified BEFORE ownership.
use super::identity;
use serde_json::Value;
use std::{
    ffi::c_void,
    io,
    net::SocketAddr,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
    time::{Duration, Instant},
};
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> RawHandle;
    fn WaitForSingleObject(handle: RawHandle, ms: u32) -> u32;
    fn GetExitCodeProcess(handle: RawHandle, code: *mut u32) -> i32;
    fn TerminateProcess(handle: RawHandle, code: u32) -> i32;
}
#[link(name = "iphlpapi")]
extern "system" {
    fn GetExtendedTcpTable(
        table: *mut c_void,
        size: *mut u32,
        order: i32,
        family: u32,
        class: u32,
        reserved: u32,
    ) -> u32;
}
pub struct OwnedWorker {
    handle: OwnedHandle,
    pub identity: Value,
    pub kill_used: bool,
}
impl OwnedWorker {
    pub fn open(expected: &Value) -> io::Result<Self> {
        let pid = u32::try_from(
            expected["pid"]
                .as_u64()
                .ok_or_else(|| io::Error::other("no pid"))?,
        )
        .map_err(io::Error::other)?;
        let raw = unsafe { OpenProcess(0x0010_0000 | 0x1000 | 0x0001, 0, pid) }; // SYNCHRONIZE | QUERY_LIMITED_INFORMATION | TERMINATE
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let actual = identity::process_identity(handle.as_raw_handle(), pid)?;
        if &actual != expected {
            return Err(io::Error::other(
                "PID/creation/image/session mismatch; no termination authorized",
            ));
        }
        Ok(Self {
            handle,
            identity: actual,
            kill_used: false,
        })
    }
    pub fn status(&self) -> io::Result<Option<u32>> {
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
            258 => Ok(None),
            0 => {
                let mut exit = 0;
                if unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut exit) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(exit))
            }
            0xffff_ffff => Err(io::Error::last_os_error()),
            x => Err(io::Error::other(format!(
                "unexpected process wait result {x}"
            ))),
        }
    }
    pub fn cleanup(&mut self) -> io::Result<Option<u32>> {
        if let Some(exit) = self.status()? {
            return Ok(Some(exit));
        }
        if unsafe { TerminateProcess(self.handle.as_raw_handle(), 72) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.kill_used = true;
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(exit) = self.status()? {
                return Ok(Some(exit));
            }
            if Instant::now() >= until {
                return Ok(None);
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }
}
impl Drop for OwnedWorker {
    fn drop(&mut self) {
        if self.status().ok().flatten().is_none() {
            let _ = self.cleanup();
        }
    }
}
/// No socket probe is used for readiness. Check the OS's listener owning PID.
/// FFI layout/class checked against windows 0.58 IpHelper bindings in evidence.
pub fn listener_owner(addr: SocketAddr, expected_pid: u32) -> io::Result<()> {
    assert!(addr.ip() == std::net::Ipv4Addr::LOCALHOST && addr.port() != 0);
    let mut size = 0;
    let first = unsafe { GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, 2, 3, 0) };
    if first != 122 {
        return Err(io::Error::other(format!("TCP table size result {first}")));
    }
    for _ in 0..4 {
        if size > 4 * 1024 * 1024 {
            return Err(io::Error::other("TCP table cap"));
        }
        let mut words = vec![0u32; (size as usize).div_ceil(4)];
        let result =
            unsafe { GetExtendedTcpTable(words.as_mut_ptr().cast(), &mut size, 0, 2, 3, 0) };
        if result == 122 {
            continue;
        }
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        if words.is_empty() || size < 4 {
            return Err(io::Error::other("short TCP table"));
        }
        let count = words[0] as usize;
        if count > words.len().saturating_sub(1) / 6 || (1 + 6 * count) * 4 > size as usize {
            return Err(io::Error::other("invalid TCP row count"));
        }
        let matched: Vec<_> = words[1..1 + 6 * count]
            .chunks_exact(6)
            .filter(|r| {
                r[0] == 2
                    && r[1].to_ne_bytes() == [127, 0, 0, 1]
                    && u16::from_be(r[2] as u16) == addr.port()
            })
            .collect();
        if matched.len() != 1 || matched[0][5] != expected_pid {
            return Err(io::Error::other(
                "loopback listener ownership not unique/expected",
            ));
        }
        return Ok(());
    }
    Err(io::Error::other("TCP ownership table never stabilized"))
}
