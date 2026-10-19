//! Protocol-width XI mapping replies on the existing native Xlib connection.
//!
//! libXi's XGetDeviceKeyMapping returns an ambiguously implemented KeySym*
//! allocation on some LP64 versions. Never use/reinterpret that allocation here.
//! Rust-xcb generates the XI request/reply from public protocol XML; its symbol
//! array is CARD32 (`u32`) irrespective of host unsigned-long width.
use super::{device_policy::keymap_read, err, KeySym, Result, XDisplay};
use std::ffi::c_int;
use xcb::{ffi::xcb_connection_t, xinput, Connection};

#[link(name = "X11-xcb")]
extern "C" {
    // Public X11/Xlib-xcb.h bridge; borrowed from Display, never disconnected.
    fn XGetXCBConnection(dpy: *mut XDisplay) -> *mut xcb_connection_t;
}
#[link(name = "X11")]
extern "C" {
    fn XFlush(dpy: *mut XDisplay) -> c_int;
}
#[link(name = "xcb")]
extern "C" {
    // Public xcb/xcb.h. Check before the no-drop constructor, which asserts
    // connection health. With no extensions to cache that constructor does no
    // IO between this check and its assertion; this Display is thread-confined.
    fn xcb_connection_has_error(connection: *mut xcb_connection_t) -> c_int;
}

/// Validate wire-width inputs before even the companion XI1 state query can
/// truncate a device ID. The queried map is bounded to the XKB keycode range.
pub(super) fn query(id: u16, first: u8, last: u8) -> Result<xinput::GetDeviceKeyMapping> {
    let device_id = u8::try_from(id).ok().filter(|id| *id != 0).ok_or_else(|| {
        err(
            "unsupported_session",
            "XI mapping device ID is not representable",
        )
    })?;
    let count = last as u16 + 1;
    let count = count
        .checked_sub(first as u16)
        .filter(|count| first >= 8 && (1..=248).contains(count))
        .and_then(|count| u8::try_from(count).ok())
        .ok_or_else(|| err("input_failed", "invalid XI mapping keycode range"))?;
    Ok(xinput::GetDeviceKeyMapping {
        device_id,
        first_keycode: first,
        count,
    })
}

/// Caller owns a live, thread-confined Display and holds the existing bounded
/// same-connection server/authority guard. No second connection, nested grab,
/// event-queue ownership change, or XCB event poll is performed.
pub(super) fn read(
    dpy: *mut XDisplay,
    query: &xinput::GetDeviceKeyMapping,
) -> Result<(Vec<KeySym>, usize)> {
    with_connection(dpy, |connection| {
        // send_request is checked for a request WITH reply. Do not use the
        // unchecked variant, which would send its errors to Xlib's event queue.
        let cookie = connection.send_request(query);
        let reply = connection
            .wait_for_reply(cookie)
            .map_err(|error| err("input_failed", &format!("XI mapping reply failed: {error}")))?;
        if reply.response_type() != 1 || reply.xi_reply_type() != 24 {
            return Err(err("input_failed", "unexpected XI mapping reply type"));
        }
        let width = reply.keysyms_per_keycode() as usize;
        // libxcb supplies an owned complete reply; the generated accessor exposes
        // its declared CARD32 payload as &[u32]. Constructing this typed slice does
        // not read its elements. The collector validates count/width/payload length
        // BEFORE invoking the reader; no symbol read precedes that geometry gate.
        let words = reply.keysyms();
        let symbols = keymap_read::collect_keysyms(
            query.count as usize,
            width,
            words.len(),
            |offset, bytes| {
                if bytes != 4 || offset % 4 != 0 {
                    return None;
                }
                words.get(offset / 4).copied().map(u64::from)
            },
        )
        .map_err(|_| err("input_failed", "invalid XI mapping reply geometry"))?
        .into_iter()
        .map(|symbol| {
            KeySym::try_from(symbol).map_err(|_| {
                err(
                    "input_failed",
                    "XI mapping symbol does not fit native KeySym",
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
        // reply owns/frees its allocation; connection's no-drop flag leaves the
        // shared transport open. Xlib continues to own all hierarchy events.
        Ok((symbols, width))
    })
}

/// Share only a scoped borrow with other bounded XI readers. Callers must not
/// take queue ownership, disconnect, or retain a handle beyond this borrow.
pub(super) fn with_connection<T>(
    dpy: *mut XDisplay,
    read: impl FnOnce(&Connection) -> Result<T>,
) -> Result<T> {
    // Commit all preceding Xlib requests before sending this XCB request.
    // XFlush is not an error-status or application-acknowledgment primitive.
    unsafe { XFlush(dpy) };
    let raw = unsafe { XGetXCBConnection(dpy) };
    if raw.is_null() {
        return Err(err(
            "unsupported_session",
            "Xlib has no public XCB connection",
        ));
    }
    if unsafe { xcb_connection_has_error(raw) } != 0 {
        return Err(err("input_failed", "native XCB connection is in error"));
    }
    // Empty extension lists avoid cache roundtrips/assertions. Generated request
    // encoding uses its own public extension descriptor; an extension error is
    // still returned (possibly as an unknown protocol error) by wait_for_reply.
    // This wrapper does NOT own raw. It stays local and dies before Display.
    let connection = unsafe { Connection::from_raw_conn_and_extensions_no_drop(raw, &[], &[]) };
    read(&connection)
}

#[cfg(test)]
#[path = "device_keymap_live_tests.rs"]
mod live_tests;
