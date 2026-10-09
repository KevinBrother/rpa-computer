use super::{clear_error, hresult, last_error, verify_pmv2};
use crate::{
    frame::allocate_zeroed, owner::Owner, DisplayError, RgbaFrame, RowOrder, SourcePermit,
};
use rpa_display_topology::DisplayDescriptor;
use std::{ffi::c_void, mem::size_of, ptr};
use windows::Win32::{
    Foundation::{HANDLE, HWND},
    Graphics::Gdi::*,
};

type Resource<T> = Owner<T, fn(T) -> bool>;
fn release_screen(dc: HDC) -> bool {
    // SAFETY: Only wraps a successful GetDC(NULL), on its owning thread.
    unsafe { ReleaseDC(HWND::default(), dc) != 0 }
}
fn delete_dc(dc: HDC) -> bool {
    // SAFETY: Only wraps a successfully created compatible DC, not a screen DC.
    unsafe { DeleteDC(dc).as_bool() }
}
fn delete_bitmap(bitmap: HBITMAP) -> bool {
    // SAFETY: Exclusive bitmap owner. Selection restores before this resource;
    // memory DC drops first on error so no owned live DC retains the bitmap.
    unsafe { DeleteObject(bitmap).as_bool() }
}
struct Selection<'a> {
    dc: &'a Resource<HDC>,
    _bitmap: &'a Resource<HBITMAP>,
    old: HGDIOBJ,
    active: bool,
}
impl<'a> Selection<'a> {
    fn new(dc: &'a Resource<HDC>, bitmap: &'a Resource<HBITMAP>) -> Result<Self, DisplayError> {
        // SAFETY: Both borrowed handles remain owned/alive through the guard.
        clear_error();
        let old = unsafe { SelectObject(dc.get(), bitmap.get()) };
        if old.is_invalid() {
            return Err(last_error("SelectObject(bitmap)"));
        }
        Ok(Self {
            dc,
            _bitmap: bitmap,
            old,
            active: true,
        })
    }
    fn restore(&mut self) -> Result<(), DisplayError> {
        if self.active {
            // SAFETY: old object was returned from this DC's successful selection
            // and remains valid while the DC exists; no intervening selections.
            if unsafe { SelectObject(self.dc.get(), self.old) }.is_invalid() {
                return Err(DisplayError::Cleanup {
                    resource: "restore original GDI object",
                });
            }
            self.active = false;
        }
        Ok(())
    }
}
impl Drop for Selection<'_> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub(super) fn capture(
    display: &DisplayDescriptor,
    permit: SourcePermit,
) -> Result<RgbaFrame, DisplayError> {
    verify_pmv2()?;
    permit.require(display.capture_size)?;
    let width = i32::try_from(display.capture_size.width).map_err(|_| {
        DisplayError::UnsupportedDisplay {
            reason: "GDI width exceeds signed LONG",
        }
    })?;
    let height = i32::try_from(display.capture_size.height).map_err(|_| {
        DisplayError::UnsupportedDisplay {
            reason: "GDI height exceeds signed LONG",
        }
    })?;
    let dib_bytes =
        u32::try_from(permit.bytes()).map_err(|_| DisplayError::UnsupportedDisplay {
            reason: "DIB image length exceeds DWORD",
        })?;
    // SAFETY: NULL obtains a screen DC for the virtual desktop; explicit PMv2
    // makes source coordinates physical, including negative monitor origins.
    clear_error();
    let screen_handle = unsafe { GetDC(HWND::default()) };
    if screen_handle.is_invalid() {
        return Err(last_error("GetDC(screen)"));
    }
    let mut screen = Resource::new(screen_handle, release_screen);
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: dib_bytes,
        ..Default::default()
    };
    let mut bits: *mut c_void = ptr::null_mut();
    // SAFETY: Initialized top-down 32-bit BI_RGB header; no file mapping handle.
    // Bounds/byte budget already validated BEFORE allocating the native DIB.
    let bitmap_handle = unsafe {
        CreateDIBSection(
            screen.get(),
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            HANDLE::default(),
            0,
        )
    }
    .map_err(|e| hresult("CreateDIBSection", e))?;
    // Declare bitmap BEFORE compatible DC: reverse Drop order always destroys
    // the DC before deleting the bitmap, even if selection restoration fails.
    let mut bitmap = Resource::new(bitmap_handle, delete_bitmap);
    if bitmap_handle.is_invalid() || bits.is_null() {
        return Err(DisplayError::UnsupportedDisplay {
            reason: "CreateDIBSection returned invalid handle or pixels",
        });
    }
    validate_dib(bitmap.get(), bits, width, height, dib_bytes)?;
    // SAFETY: Verified, exclusively owned DIB allocation, not yet selected or
    // used by any GDI command. Never expose unspecified initial bitmap storage
    // if the OS clips a blit; successful capture still requires all OS checks.
    unsafe {
        ptr::write_bytes(bits.cast::<u8>(), 0, dib_bytes as usize);
    }

    // SAFETY: Borrowed live screen DC. The new memory DC has its own lifetime.
    clear_error();
    let memory_handle = unsafe { CreateCompatibleDC(screen.get()) };
    if memory_handle.is_invalid() {
        return Err(last_error("CreateCompatibleDC"));
    }
    let mut memory = Resource::new(memory_handle, delete_dc);
    {
        let mut selected = Selection::new(&memory, &bitmap)?;
        // SAFETY: Destination DIB selected into owned compatible DC, matching
        // physical dimensions. Source uses physical global bounds, NOT *scale.
        unsafe {
            BitBlt(
                memory.get(),
                0,
                0,
                width,
                height,
                screen.get(),
                display.native_bounds.x,
                display.native_bounds.y,
                SRCCOPY | CAPTUREBLT,
            )
        }
        .map_err(|e| hresult("BitBlt", e))?;
        // SAFETY: Required by CreateDIBSection before CPU access to GDI writes.
        clear_error();
        if !unsafe { GdiFlush() }.as_bool() {
            return Err(last_error("GdiFlush"));
        }
        selected.restore()?;
    }
    let mut bytes = allocate_zeroed(permit.bytes())?;
    // SAFETY: GetObject verified packed 32-bit DIB geometry/pointer/stride and
    // permit bounds. Bitmap owns the entire readable allocation; GdiFlush has
    // completed, no DC selects it, and source/destination allocations are disjoint.
    unsafe {
        ptr::copy_nonoverlapping(bits.cast::<u8>(), bytes.as_mut_ptr(), bytes.len());
    }
    let frame = RgbaFrame::from_bgra32(
        display.capture_size,
        u64::from(display.capture_size.width) * 4,
        bytes,
        RowOrder::TopDown,
    )?;
    // Successful capture also requires explicit cleanup checks. Try all closes
    // even if one fails; Owner retains failed handles for a best-effort Drop retry.
    let memory_ok = memory.close();
    let bitmap_ok = bitmap.close();
    let screen_ok = screen.close();
    if !memory_ok {
        return Err(DisplayError::Cleanup {
            resource: "DeleteDC",
        });
    }
    if !bitmap_ok {
        return Err(DisplayError::Cleanup {
            resource: "DeleteObject(DIB)",
        });
    }
    if !screen_ok {
        return Err(DisplayError::Cleanup {
            resource: "ReleaseDC(screen)",
        });
    }
    Ok(frame)
}
fn validate_dib(
    bitmap: HBITMAP,
    bits: *mut c_void,
    width: i32,
    height: i32,
    bytes: u32,
) -> Result<(), DisplayError> {
    let mut section = DIBSECTION::default();
    // SAFETY: Writable initialized DIBSECTION of exactly the supplied size;
    // HBITMAP came from CreateDIBSection and is still exclusively owned.
    clear_error();
    let copied = unsafe {
        GetObjectW(
            bitmap,
            size_of::<DIBSECTION>() as i32,
            Some((&mut section as *mut DIBSECTION).cast()),
        )
    };
    if copied != size_of::<DIBSECTION>() as i32 {
        return Err(last_error("GetObjectW(DIBSECTION)"));
    }
    let row = i64::from(width) * 4;
    if section.dsBm.bmWidth != width
        || section.dsBm.bmHeight != height
        || i64::from(section.dsBm.bmWidthBytes) != row
        || section.dsBm.bmBitsPixel != 32
        || section.dsBm.bmPlanes != 1
        || section.dsBm.bmBits != bits
        || section.dsBmih.biWidth != width
        || section.dsBmih.biHeight.unsigned_abs() != height as u32
        || section.dsBmih.biBitCount != 32
        || section.dsBmih.biCompression != BI_RGB.0
        || row * i64::from(height) != i64::from(bytes)
    {
        return Err(DisplayError::UnsupportedDisplay {
            reason: "actual DIB dimensions, stride, format or pointer mismatch",
        });
    }
    Ok(())
}
