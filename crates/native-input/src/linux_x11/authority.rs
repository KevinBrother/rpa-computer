//! Same-connection desktop authority verification. Caller holds ServerGrab;
//! checking the XCB monitor alone would leave a check/inject ownership race.
use super::{err, Result, XDisplay};
use crate::{X11Lease, X11_LEASE_NONCE_PROPERTY, X11_LEASE_SELECTION};
use std::ffi::{c_char, c_int, c_long, c_ulong, c_void, CString};
use std::ptr;

#[link(name = "X11")]
extern "C" {
    fn XInternAtom(display: *mut XDisplay, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XGetSelectionOwner(display: *mut XDisplay, selection: c_ulong) -> c_ulong;
    fn XGetWindowProperty(
        display: *mut XDisplay,
        window: c_ulong,
        property: c_ulong,
        offset: c_long,
        length: c_long,
        delete: c_int,
        requested_type: c_ulong,
        actual_type: *mut c_ulong,
        actual_format: *mut c_int,
        items: *mut c_ulong,
        bytes_after: *mut c_ulong,
        value: *mut *mut u8,
    ) -> c_int;
    fn XFree(value: *mut c_void) -> c_int;
}

pub(super) fn verify(dpy: *mut XDisplay, lease: &X11Lease) -> Result<()> {
    let reject = || {
        lease.revoke();
        err("input_failed", "X11 desktop writer authority lost or revoked; native operation not authorized; cleanup may remain pending")
    };
    if lease.is_revoked() {
        return Err(reject());
    }
    let (selection, owner, nonce) = lease.identity();
    if selection == 0 || owner == 0 {
        return Err(reject());
    }
    let selection_name = CString::new(X11_LEASE_SELECTION).expect("constant without NUL");
    let property_name = CString::new(X11_LEASE_NONCE_PROPERTY).expect("constant without NUL");
    // only_if_exists=1: an authority check never creates protocol resources.
    let canonical = unsafe { XInternAtom(dpy, selection_name.as_ptr(), 1) };
    if canonical == 0 || canonical != selection as c_ulong {
        return Err(reject());
    }
    if unsafe { XGetSelectionOwner(dpy, canonical) } != owner as c_ulong {
        return Err(reject());
    }
    let property = unsafe { XInternAtom(dpy, property_name.as_ptr(), 1) };
    if property == 0 {
        return Err(reject());
    }
    let (mut actual_type, mut format, mut items, mut after) = (0, 0, 0, 0);
    let mut value = ptr::null_mut();
    // XA_STRING=31. XGetWindowProperty length is in 32-bit units: four units
    // read exactly 16 nonce bytes; any extra bytes must reject, not truncate.
    let status = unsafe {
        XGetWindowProperty(
            dpy,
            owner as c_ulong,
            property,
            0,
            4,
            0,
            31,
            &mut actual_type,
            &mut format,
            &mut items,
            &mut after,
            &mut value,
        )
    };
    let matches = status == 0
        && actual_type == 31
        && format == 8
        && items == 16
        && after == 0
        && !value.is_null()
        && unsafe { std::slice::from_raw_parts(value, 16) == nonce };
    if !value.is_null() {
        unsafe {
            XFree(value.cast());
        }
    }
    // Monitor revocation during property reads must not authorize a new event.
    if !matches || lease.is_revoked() {
        return Err(reject());
    }
    Ok(())
}
