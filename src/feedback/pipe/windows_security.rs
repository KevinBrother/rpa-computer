//! Logon-session-only DACL; no default Everyone/anonymous pipe read grant.
use std::{
    ffi::c_void,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
#[repr(C)]
pub(super) struct Attributes {
    length: u32,
    descriptor: *mut c_void,
    inherit: i32,
}
#[repr(C)]
struct Group {
    sid: *mut c_void,
    attributes: u32,
}
#[repr(C)]
struct Groups {
    count: u32,
    first: Group,
}
pub(super) struct Security {
    pub attributes: Attributes,
}
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
    fn GetTokenInformation(
        token: *mut c_void,
        class: u32,
        info: *mut c_void,
        length: u32,
        needed: *mut u32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: *mut c_void, text: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        text: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        length: *mut u32,
    ) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
}
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, length: u32, flags: u32) -> i32;
}
impl Security {
    pub fn new() -> io::Result<Self> {
        let mut raw = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), 8, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut needed = 0;
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                2,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
        }
        if !(std::mem::size_of::<Groups>()..=65536).contains(&(needed as usize)) {
            return Err(io::Error::other("logon group bounds"));
        }
        // u64 storage gives the TOKEN_GROUPS/SID_AND_ATTRIBUTES proper alignment.
        let mut storage = vec![0u64; (needed as usize).div_ceil(8)];
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                2,
                storage.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let groups = unsafe { &*(storage.as_ptr().cast::<Groups>()) };
        let offset = std::mem::offset_of!(Groups, first);
        let count = groups.count as usize;
        if count > (storage.len() * 8 - offset) / std::mem::size_of::<Group>() {
            return Err(io::Error::other("invalid logon groups"));
        }
        let entries =
            unsafe { std::slice::from_raw_parts(std::ptr::addr_of!(groups.first), count) };
        let group = entries
            .iter()
            .find(|g| g.attributes & 0xC000_0000 == 0xC000_0000)
            .ok_or_else(|| io::Error::other("interactive logon SID unavailable"))?;
        let mut text: *mut u16 = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(group.sid, &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while len < 184 && unsafe { *text.add(len) } != 0 {
            len += 1;
        }
        let sid = if len < 184 {
            String::from_utf16(unsafe { std::slice::from_raw_parts(text, len) }).ok()
        } else {
            None
        };
        unsafe {
            LocalFree(text.cast());
        }
        let sid = sid.ok_or_else(|| io::Error::other("invalid logon SID"))?;
        let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            attributes: Attributes {
                length: std::mem::size_of::<Attributes>() as u32,
                descriptor,
                inherit: 0,
            },
        })
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.attributes.descriptor);
        }
    }
}
pub(super) fn nonce() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    if unsafe { BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), 16, 2) } != 0 {
        return Err(io::Error::other("pipe nonce unavailable"));
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
