//! Ignored Linux integration fixture; author NEVER executes it.
//!
//! CC must provision a fresh, private, authenticated, unchanged default Xvfb,
//! with an external timeout and cleanup trap. Do not point this at a user's X
//! server. Set RPA_NATIVE_BRIDGE_OPT_IN=fresh-private-authenticated-xvfb,
//! RPA_NATIVE_BRIDGE_OWNED_DISPLAY=<explicit :number>, and XAUTHORITY to its
//! owned mode0600 authority file. Run only the exact ignored test below,
//! --ignored --exact --nocapture --test-threads=1. No fallback to DISPLAY.
//!
//! Only mutations: create/destroy one UNMAPPED InputOnly window, select its
//! PropertyNotify events, and change its private property twice. No input,
//! mapping, hierarchy, focus, clipboard, selection, or global handler changes.
//! No XCB event APIs. The actual production device_keymap::read is exercised.
use super::{query, read};
use crate::linux_x11::{devices, xkb, KeySym, XDisplay, XOpenDisplay};
use std::cell::Cell;
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};
use std::os::unix::fs::MetadataExt;
use std::ptr;
use std::rc::Rc;

// X11/Xlib.h: XPropertyEvent and XEvent's long pad[24], not guessed buffers.
#[repr(C)]
#[derive(Clone, Copy)]
struct PropertyEvent {
    kind: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut XDisplay,
    window: c_ulong,
    atom: c_ulong,
    time: c_ulong,
    state: c_int,
}
#[repr(C)]
union Event {
    property: PropertyEvent,
    pad: [c_long; 24],
}
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<PropertyEvent>() == 64);
    assert!(std::mem::size_of::<Event>() == 192);
};
#[link(name = "X11")]
extern "C" {
    fn XCloseDisplay(dpy: *mut XDisplay) -> c_int;
    fn XDefaultRootWindow(dpy: *mut XDisplay) -> c_ulong;
    fn XCreateWindow(
        dpy: *mut XDisplay,
        parent: c_ulong,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
        border: c_uint,
        depth: c_int,
        class: c_uint,
        visual: *mut c_void,
        mask: c_ulong,
        attributes: *mut c_void,
    ) -> c_ulong;
    fn XDestroyWindow(dpy: *mut XDisplay, window: c_ulong) -> c_int;
    fn XSelectInput(dpy: *mut XDisplay, window: c_ulong, mask: c_long) -> c_int;
    fn XInternAtom(dpy: *mut XDisplay, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XChangeProperty(
        dpy: *mut XDisplay,
        window: c_ulong,
        property: c_ulong,
        kind: c_ulong,
        format: c_int,
        mode: c_int,
        data: *const u8,
        count: c_int,
    ) -> c_int;
    fn XSync(dpy: *mut XDisplay, discard: c_int) -> c_int;
    fn XEventsQueued(dpy: *mut XDisplay, mode: c_int) -> c_int;
    fn XCheckTypedWindowEvent(
        dpy: *mut XDisplay,
        window: c_ulong,
        kind: c_int,
        event: *mut Event,
    ) -> c_int;
    fn XGetInputFocus(dpy: *mut XDisplay, focus: *mut c_ulong, revert: *mut c_int) -> c_int;
    fn XGetKeyboardMapping(
        dpy: *mut XDisplay,
        first: u8,
        count: c_int,
        width: *mut c_int,
    ) -> *mut KeySym;
    fn XFree(value: *mut c_void) -> c_int;
}
extern "C" {
    fn getuid() -> c_uint;
}

struct Owned {
    dpy: *mut XDisplay,
    window: c_ulong,
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            if self.window != 0 {
                XDestroyWindow(self.dpy, self.window);
            }
            XSync(self.dpy, 0); // never discard someone else's queued events
            XCloseDisplay(self.dpy);
        }
    }
}
fn take_property(owned: &Owned, atom: c_ulong) -> PropertyEvent {
    let mut event = Event { pad: [0; 24] };
    assert_ne!(
        unsafe { XCheckTypedWindowEvent(owned.dpy, owned.window, 28, &mut event) },
        0,
        "queued PropertyNotify was lost during XCB request/reply"
    );
    let event = unsafe { event.property };
    assert_eq!(event.kind, 28);
    assert_eq!(event.window, owned.window);
    assert_eq!(event.atom, atom);
    assert_eq!(event.display, owned.dpy);
    assert_eq!(event.send_event, 0);
    assert_eq!(event.state, 0); // PropertyNewValue
    event
}
fn xlib_roundtrip(dpy: *mut XDisplay) {
    let (mut focus, mut revert) = (0, 0);
    assert_ne!(unsafe { XGetInputFocus(dpy, &mut focus, &mut revert) }, 0);
    assert!((0..=2).contains(&revert));
}

#[test]
#[ignore = "owned authenticated default Xvfb only; explicit opt-in, external timeout, serial CC execution"]
fn owned_xvfb_safe_bridge_preserves_xlib_queue_and_connection() {
    assert_eq!(
        std::env::var("RPA_NATIVE_BRIDGE_OPT_IN").as_deref(),
        Ok("fresh-private-authenticated-xvfb"),
        "refuse implicit live X server access"
    );
    let display =
        std::env::var("RPA_NATIVE_BRIDGE_OWNED_DISPLAY").expect("explicit owned DISPLAY required");
    assert!(display.starts_with(':') && display.len() <= 16);
    assert!(
        !display[1..].is_empty() && display[1..].bytes().all(|b| b.is_ascii_digit()),
        "only an explicit local fixture display number is allowed"
    );
    let auth = std::env::var_os("XAUTHORITY").expect("private XAUTHORITY required");
    let metadata = std::fs::symlink_metadata(&auth).expect("authority file metadata");
    assert!(metadata.is_file() && metadata.len() > 0 && metadata.len() <= 65536);
    let uid = unsafe { getuid() };
    assert_ne!(uid, 0, "fixture runtime must stay nonroot");
    assert_eq!(metadata.uid(), uid);
    assert_eq!(
        metadata.mode() & 0o777,
        0o600,
        "authority must be owned mode0600 regular file"
    );
    // Never print the authority path/content, credentials, or inherited env.
    let name = CString::new(display).unwrap();
    let dpy = unsafe { XOpenDisplay(name.as_ptr().cast()) };
    assert!(
        !dpy.is_null(),
        "cannot connect to explicit authenticated owned fixture"
    );
    let mut owned = Owned { dpy, window: 0 };
    let depth = Rc::new(Cell::new(0));
    let (master_id, range, identities) = {
        let _guard = xkb::ServerGrab::new(dpy, depth.clone());
        xkb::require_extension(dpy).unwrap();
        let master = xkb::Snapshot::get(dpy).unwrap();
        let identities = devices::inventory(dpy, master.device_id()).unwrap();
        (master.device_id(), master.range(), identities)
    };
    assert!(
        (2..=32).contains(&identities.len()),
        "need master and actual slaves, bounded fixture"
    );
    assert_eq!(range, (8, 255), "requires unchanged default Xvfb range");
    let master_query = query(master_id, range.0, range.1).unwrap();
    let master_map = {
        let _guard = xkb::ServerGrab::new(dpy, depth.clone());
        let typed = read(dpy, &master_query).unwrap();
        // Independent core Xlib API widens its result correctly; not the retired
        // XI libXi API. Compare every column, including trailing NoSymbol.
        let mut width = 0;
        let raw = unsafe { XGetKeyboardMapping(dpy, range.0, 248, &mut width) };
        assert!(!raw.is_null());
        struct CoreMap(*mut KeySym);
        impl Drop for CoreMap {
            fn drop(&mut self) {
                unsafe {
                    XFree(self.0.cast());
                }
            }
        }
        let _core = CoreMap(raw);
        assert!((1..=255).contains(&width));
        let core = unsafe { std::slice::from_raw_parts(raw, 248 * width as usize) };
        assert_eq!(typed.1, width as usize);
        assert_eq!(typed.0, core);
        typed
    };
    assert_eq!(master_map.0.len(), 248 * master_map.1);
    assert!(
        master_map
            .0
            .chunks_exact(master_map.1)
            .any(|row| row.iter().all(|s| *s == 0)),
        "default fixture should contain vacancies; not a policy eligibility verdict"
    );
    assert!(master_map
        .0
        .chunks_exact(master_map.1)
        .any(|row| row.starts_with(&[112, 80])));
    assert!(master_map
        .0
        .chunks_exact(master_map.1)
        .any(|row| row.starts_with(&[97, 65])));
    for identity in &identities {
        let request = query(identity.id, range.0, range.1).unwrap();
        let mut baseline = None;
        for iteration in 0..3 {
            let actual = {
                let _guard = xkb::ServerGrab::new(dpy, depth.clone());
                let snapshot = xkb::Snapshot::get_device(dpy, identity.id as _).unwrap();
                assert_eq!(snapshot.range(), range);
                let actual = read(dpy, &request).unwrap();
                assert_eq!(actual.0.len(), 248 * actual.1);
                // Validate selected rows independently against this device's
                // XKB map, not a guessed/common master layout. State0 equals
                // core column0 for these ordinary default printable keys.
                for key in [33u8, 38] {
                    let symbol = snapshot.effective_symbol(key).unwrap();
                    assert_eq!(actual.0[(key - range.0) as usize * actual.1], symbol);
                }
                actual
            };
            assert!(actual.0.iter().all(|symbol| u32::try_from(*symbol).is_ok()));
            if let Some(previous) = &baseline {
                assert_eq!(
                    &actual, previous,
                    "device {} changed between reads at round {}",
                    identity.id, iteration
                );
            } else {
                baseline = Some(actual);
            }
            xlib_roundtrip(dpy);
        }
        let baseline = baseline.unwrap();
        // Report genuine cross-device divergence; do not normalize it or confuse
        // it with a reader failure. Policy eligibility is a separate gate.
        eprintln!(
            "safe_bridge device={} rows=248 width={} repeated_reads=3 equals_master={}",
            identity.id,
            baseline.1,
            baseline == master_map
        );
    }
    // XIAllDevices(0) is not a real device ID accepted by XI1 GetDeviceKeyMapping.
    // Bypass only the production input validator to exercise its ACTUAL checked
    // reply error path. No private connection/event handler/duplicate reader.
    let invalid = xcb::xinput::GetDeviceKeyMapping {
        device_id: 0,
        first_keycode: 8,
        count: 1,
    };
    {
        let _guard = xkb::ServerGrab::new(dpy, depth.clone());
        let error =
            read(dpy, &invalid).expect_err("invalid device must produce checked protocol error");
        assert!(error.to_string().contains("XI mapping reply failed"));
        assert_eq!(read(dpy, &master_query).unwrap(), master_map);
    }
    xlib_roundtrip(dpy); // also exposes any unchecked error left for Xlib

    owned.window = unsafe {
        XCreateWindow(
            dpy,
            XDefaultRootWindow(dpy),
            0,
            0,
            1,
            1,
            0,
            0,
            2,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
        ) // InputOnly, CopyFromParent visual
    };
    assert_ne!(owned.window, 0);
    let atom = unsafe { XInternAtom(dpy, c"_RPA_NATIVE_SAFE_BRIDGE_FIXTURE".as_ptr(), 0) };
    assert_ne!(atom, 0);
    unsafe {
        XSelectInput(dpy, owned.window, 1 << 22); // PropertyChangeMask
        XChangeProperty(dpy, owned.window, atom, 31, 8, 0, b"first".as_ptr(), 5);
        XSync(dpy, 0); // materialize the first event in Xlib's own queue
    }
    let queued = unsafe { XEventsQueued(dpy, 0) }; // QueuedAlready: no socket read
    assert!((1..=64).contains(&queued));
    {
        let _guard = xkb::ServerGrab::new(dpy, depth.clone());
        assert_eq!(read(dpy, &master_query).unwrap(), master_map);
    }
    assert!(unsafe { XEventsQueued(dpy, 0) } >= queued);
    let first = take_property(&owned, atom);
    unsafe {
        XChangeProperty(dpy, owned.window, atom, 31, 8, 0, b"second".as_ptr(), 6);
    }
    // No XSync here: production XFlush + checked XCB reply must coexist with
    // an incoming event as well as a pre-existing Xlib queued event.
    {
        let _guard = xkb::ServerGrab::new(dpy, depth.clone());
        assert_eq!(read(dpy, &master_query).unwrap(), master_map);
    }
    xlib_roundtrip(dpy);
    let second = take_property(&owned, atom);
    assert!(second.serial > first.serial);
    let mut extra = Event { pad: [0; 24] };
    assert_eq!(
        unsafe { XCheckTypedWindowEvent(dpy, owned.window, 28, &mut extra) },
        0
    );
    assert_eq!(depth.get(), 0, "no lingering nested server grab");
    // Owned drops window then Display. External CC trap also reaps Xvfb on
    // panic/abort/timeout; in-process cleanup alone is not a transport deadline.
    drop(owned);
    eprintln!("safe_bridge checked_error_recovered=true xlib_roundtrip=true queued_property_events=2 owned_window_display_closed=true");
}
