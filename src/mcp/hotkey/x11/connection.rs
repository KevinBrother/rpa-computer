//! The only raw XCB boundary: binary-safe authentication and failure cleanup.
//! Everything after setup uses the typed xcb core protocol API.
use std::os::fd::{IntoRawFd, OwnedFd};
use xcb::ffi::xcb_connection_t;

#[repr(C)]
struct AuthInfo {
    name_len: libc::c_int,
    name: *mut libc::c_char,
    data_len: libc::c_int,
    data: *mut libc::c_char,
}

#[link(name = "xcb")]
extern "C" {
    fn xcb_connect_to_fd(fd: libc::c_int, auth: *mut AuthInfo) -> *mut xcb_connection_t;
    fn xcb_connection_has_error(connection: *mut xcb_connection_t) -> libc::c_int;
    fn xcb_disconnect(connection: *mut xcb_connection_t);
}

struct RawConnection(*mut xcb_connection_t);
impl Drop for RawConnection {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this guard exclusively owns the returned XCB connection.
            unsafe { xcb_disconnect(self.0) };
        }
    }
}

/// `interrupt` is a retained duplicate of `socket` in the calling registration:
/// shutdown on it interrupts even this blocking setup handshake. XCB owns and
/// closes the original fd. No other thread closes an integer borrowed from XCB.
pub(super) fn connect(socket: OwnedFd, cookie: Option<&[u8]>) -> Result<xcb::Connection, String> {
    let mut protocol = *b"MIT-MAGIC-COOKIE-1";
    let mut data = cookie.unwrap_or_default().to_vec();
    let mut auth = AuthInfo {
        name_len: protocol.len() as libc::c_int,
        name: protocol.as_mut_ptr().cast(),
        data_len: data.len() as libc::c_int,
        data: data.as_mut_ptr().cast(),
    };
    let info = if cookie.is_some() {
        &mut auth
    } else {
        std::ptr::null_mut()
    };
    // SAFETY: fd ownership moves to XCB, the repr(C) record and both buffers
    // remain alive throughout the call. Auth data is binary, NOT a CString.
    let mut raw = RawConnection(unsafe { xcb_connect_to_fd(socket.into_raw_fd(), info) });
    data.fill(0);
    if raw.0.is_null() {
        return Err("XCB could not allocate a connection".into());
    }
    // SAFETY: nonnull result of xcb_connect_to_fd; failed connections must ALSO
    // be disconnected. This guard handles both errors and constructor unwind.
    let error = unsafe { xcb_connection_has_error(raw.0) };
    if error != 0 {
        return Err(format!("XCB setup/authentication failed (connection error {error}); verify DISPLAY and inherited XAUTHORITY"));
    }
    // SAFETY: a valid owned XCB connection with no Xlib owner. The crate takes
    // ownership only on success. A timeout racing its internal error assertion
    // unwinds through RawConnection and still closes the transport.
    let connection = unsafe { xcb::Connection::from_raw_conn(raw.0) };
    raw.0 = std::ptr::null_mut();
    Ok(connection)
}

/// Cache XKB event/error metadata after the initial core probe. Transfer through
/// the same raw RAII guard so constructor unwind still disconnects exactly once.
pub(super) fn with_xkb(connection: xcb::Connection) -> Result<xcb::Connection, String> {
    let mut raw = RawConnection(connection.into_raw_conn());
    // SAFETY: sole ownership transferred above; optional extension avoids a
    // missing-extension assertion. Watchdog owns a dup of this same socket.
    let connection = unsafe {
        xcb::Connection::from_raw_conn_and_extensions(raw.0, &[], &[xcb::Extension::Xkb])
    };
    raw.0 = std::ptr::null_mut();
    if !connection
        .active_extensions()
        .any(|extension| extension == xcb::Extension::Xkb)
    {
        return Err("X11 server lacks XKB delivery validation support".into());
    }
    Ok(connection)
}
