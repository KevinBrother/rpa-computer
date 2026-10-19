//! Device-specific XTEST on the existing borrowed Xlib/XCB connection.
//! XI event codes are essential: core KeyPress ignores FakeInput.deviceid.
use super::{
    device_keymap, err, map_request::scalar_input::KeyRequest, KeySym, Platform, Result, XDisplay,
};
use xcb::{x, xinput, xtest, Xid};

#[derive(Clone, Copy)]
pub(super) struct Protocol {
    press: u8,
    release: u8,
}
impl Protocol {
    fn negotiate(dpy: *mut XDisplay) -> Result<Self> {
        device_keymap::with_connection(dpy, |connection| {
            let version = connection
                .wait_for_reply(connection.send_request(&xtest::GetVersion {
                    major_version: 2,
                    minor_version: 2,
                }))
                .map_err(|e| {
                    err(
                        "unsupported_session",
                        format!("XTEST version reply failed: {e}"),
                    )
                })?;
            if (version.major_version(), version.minor_version()) < (2, 2) {
                return Err(err(
                    "unsupported_session",
                    "native device input requires XTEST 2.2",
                ));
            }
            let xi = connection
                .wait_for_reply(connection.send_request(&x::QueryExtension {
                    name: b"XInputExtension",
                }))
                .map_err(|e| {
                    err(
                        "unsupported_session",
                        format!("XI event-base query failed: {e}"),
                    )
                })?;
            if !xi.present() || xi.first_event() < 64 {
                return Err(err(
                    "unsupported_session",
                    "native device input requires XI event codes",
                ));
            }
            Ok(Self {
                press: super::map_request::scalar_input::key_state_wire::device_key_event(
                    xi.first_event(),
                    true,
                )?,
                release: super::map_request::scalar_input::key_state_wire::device_key_event(
                    xi.first_event(),
                    false,
                )?,
            })
        })
    }
    pub fn emit(self, dpy: *mut XDisplay, request: KeyRequest) -> Result<()> {
        let deviceid = request
            .device_id()
            .and_then(|id| u8::try_from(id).ok())
            .filter(|id| (1..=127).contains(id))
            .ok_or_else(|| {
                err(
                    "input_failed",
                    "native key has no representable pinned device",
                )
            })?;
        if request.key < 8 {
            return Err(err("input_failed", "invalid native keycode"));
        }
        device_keymap::with_connection(dpy, |connection| {
            // No delay, axes, second connection or event queue takeover. This
            // checked request processes server protocol errors before success;
            // it is not application receipt. Key-up is separately observed.
            let cookie = connection.send_request_checked(&xtest::FakeInput {
                r#type: if request.press {
                    self.press
                } else {
                    self.release
                },
                detail: request.key,
                time: 0,
                root: x::Window::none(),
                root_x: 0,
                root_y: 0,
                deviceid,
            });
            connection.check_request(cookie).map_err(|e| {
                err(
                    "input_failed",
                    format!("device-specific XTEST input failed: {e}"),
                )
            })
        })
    }
}
impl Platform {
    pub(super) fn native_protocol(&mut self) -> Result<Protocol> {
        if let Some(protocol) = self.native_protocol {
            return Ok(protocol);
        }
        let protocol = Protocol::negotiate(self.dpy)?;
        // Event base/version are connection/server properties, not a target
        // lease. Device identity and lifetime are revalidated for each request.
        self.native_protocol = Some(protocol);
        Ok(protocol)
    }
}

/// Explicit-device observations also work for a pinned master: unlike
/// XOpenDevice, these protocol reads do not reject master devices up front.
/// QueryDeviceState is the pressed-bit protocol, not XIQueryDevice key classes.
/// Its access-control behavior has no reliable "state suppressed" reply flag:
/// zero bits are NOT an unconditional independent key-up proof under XACE.
/// Same-mask denial detection is a documented follow-up, not implemented here.
pub(super) fn observe(
    dpy: *mut XDisplay,
    id: u16,
    min: u8,
    max: u8,
) -> Result<(Vec<KeySym>, usize, [u8; 32])> {
    let query = device_keymap::query(id, min, max)?;
    let keys = device_keymap::with_connection(dpy, |connection| {
        let reply = connection
            .wait_for_reply(connection.send_request(&xinput::QueryDeviceState {
                device_id: query.device_id,
            }))
            .map_err(|e| err("input_failed", format!("XI key-state reply failed: {e}")))?;
        if reply.response_type() != 1 || reply.xi_reply_type() != 30 {
            return Err(err("input_failed", "invalid XI key-state reply type"));
        }
        let length = usize::try_from(reply.length())
            .ok()
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add(32))
            .filter(|n| (32..=1024).contains(n))
            .ok_or_else(|| err("input_failed", "unbounded XI key-state reply"))?;
        // Public generated Reply owns a complete wire allocation. Validate its
        // byte-record geometry before using any variable generated iterator;
        // no private Xlib layout, KeySym cast or raw connection ownership.
        let bytes = unsafe { std::slice::from_raw_parts(xcb::Reply::as_raw(&reply), length) };
        super::map_request::scalar_input::key_state_wire::pressed_keys(bytes, query.count)
    })?;
    let (map, width) = device_keymap::read(dpy, &query)?;
    Ok((map, width, keys))
}
