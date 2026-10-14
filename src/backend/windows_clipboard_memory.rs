//! Movable clipboard allocation and scoped, bounded reads of our own payload.
use super::{combine_cleanup, error, BackendError};
use windows::Win32::Foundation::{
    GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HANDLE, HGLOBAL,
};
use windows::Win32::System::DataExchange::SetClipboardData;
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Movable allocation owns memory until SetClipboardData succeeds. Any early
/// return unlocks/frees our allocation; transferred memory is never freed here.
pub(super) struct GlobalText {
    handle: Option<HGLOBAL>,
    locked: bool,
}

impl GlobalText {
    pub(super) fn new(utf16: &[u16]) -> Result<Self, BackendError> {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of_val(utf16))
                .map_err(|e| error(format!("paste GlobalAlloc failed: {e}")))?;
            let mut allocation = Self {
                handle: Some(handle),
                locked: false,
            };
            let pointer = GlobalLock(handle);
            if pointer.is_null() {
                return Err(error(format!(
                    "paste GlobalLock failed: {}",
                    windows::core::Error::from_win32()
                )));
            }
            allocation.locked = true;
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), pointer.cast::<u16>(), utf16.len());
            // GlobalUnlock returns FALSE on a successful final unlock. The
            // windows-rs BOOL wrapper alone cannot distinguish that from an
            // error: explicitly clear/read last-error per the Win32 contract.
            SetLastError(ERROR_SUCCESS);
            let unlocked = GlobalUnlock(handle);
            if unlocked.is_err() && GetLastError() != ERROR_SUCCESS {
                return Err(error(format!(
                    "paste GlobalUnlock failed: {}",
                    windows::core::Error::from_win32()
                )));
            }
            allocation.locked = false;
            Ok(allocation)
        }
    }

    pub(super) fn publish(mut self) -> Result<(), BackendError> {
        let handle = self.handle.expect("allocation has not been transferred");
        unsafe { SetClipboardData(u32::from(CF_UNICODETEXT.0), HANDLE(handle.0)) }
            .map_err(|e| error(format!("paste SetClipboardData failed: {e}")))?;
        self.handle = None; // The system, not this RAII value, now owns HGLOBAL.
        Ok(())
    }
}

impl Drop for GlobalText {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe {
                if self.locked {
                    let _ = GlobalUnlock(handle);
                }
                let _ = GlobalFree(handle);
            }
        }
    }
}

/// Caller must hold the clipboard lock and have already proven ownership.
/// Only the expected number of UTF-16 units is inspected, never allocator
/// padding or unbounded clipboard contents. System-owned memory is not freed.
pub(super) fn inspect_utf16<T>(
    handle: HANDLE,
    units: usize,
    inspect: impl FnOnce(&[u16]) -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    let handle = HGLOBAL(handle.0);
    let bytes = units
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| error("paste payload inspection size overflow"))?;
    if bytes == 0 || unsafe { GlobalSize(handle) } < bytes {
        return Err(error(
            "published clipboard allocation is shorter than requested payload",
        ));
    }
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        return Err(error(format!(
            "cannot lock published clipboard payload: {}",
            windows::core::Error::from_win32()
        )));
    }
    let mut locked = ReadLock(Some(handle));
    // SAFETY: GlobalSize proves this bounded range exists; GlobalLock yields
    // aligned memory, kept locked until after inspect returns. No slice escapes.
    let payload = unsafe { std::slice::from_raw_parts(pointer.cast::<u16>(), units) };
    let result = inspect(payload);
    combine_cleanup(result, locked.unlock())
}

struct ReadLock(Option<HGLOBAL>);
impl ReadLock {
    fn unlock(&mut self) -> Result<(), BackendError> {
        if let Some(handle) = self.0 {
            unsafe {
                SetLastError(ERROR_SUCCESS);
                let result = GlobalUnlock(handle);
                if result.is_err() && GetLastError() != ERROR_SUCCESS {
                    return Err(error(format!(
                        "cannot unlock published clipboard payload: {}",
                        windows::core::Error::from_win32()
                    )));
                }
            }
            self.0 = None;
        }
        Ok(())
    }
}
impl Drop for ReadLock {
    fn drop(&mut self) {
        let _ = self.unlock();
    }
}
