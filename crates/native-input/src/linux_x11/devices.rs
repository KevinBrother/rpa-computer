//! XInput inventory and per-device core projection/key-down observations.
//! Must be called inside the caller's same-connection authority/server grab.
//! Map writes to a master propagate to its attached key-capable devices,
//! including a key-capable slave pointer via the paired master pointer.
use super::{err, KeySym, Result, XDisplay};
use std::ffi::{c_int, c_ulong, c_void, CStr};
use std::ptr;

const MAX_DEVICES: usize = 256;
#[repr(C)]
struct AnyClass {
    kind: c_int,
    source: c_int,
}
#[repr(C)]
struct DeviceInfo {
    id: c_int,
    name: *mut i8,
    kind: c_int,
    attachment: c_int,
    enabled: c_int,
    num_classes: c_int,
    classes: *mut *mut AnyClass,
}
#[repr(C)]
struct Device {
    id: c_ulong,
    num_classes: c_int,
    classes: *mut c_void,
}
#[repr(C)]
struct DeviceState {
    id: c_ulong,
    num_classes: c_int,
    data: *mut u8,
}
#[repr(C)]
struct KeyState {
    kind: u8,
    length: u8,
    num_keys: i16,
    keys: [u8; 32],
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<DeviceInfo>() == 40);
    assert!(std::mem::size_of::<Device>() == 24);
    assert!(std::mem::size_of::<DeviceState>() == 24);
    assert!(std::mem::size_of::<KeyState>() == 36);
};

#[link(name = "Xi")]
extern "C" {
    fn XIQueryVersion(dpy: *mut XDisplay, major: *mut c_int, minor: *mut c_int) -> c_int;
    fn XIQueryDevice(dpy: *mut XDisplay, device: c_int, count: *mut c_int) -> *mut DeviceInfo;
    fn XIFreeDeviceInfo(info: *mut DeviceInfo);
    fn XOpenDevice(dpy: *mut XDisplay, id: c_ulong) -> *mut Device;
    fn XCloseDevice(dpy: *mut XDisplay, device: *mut Device) -> c_int;
    fn XQueryDeviceState(dpy: *mut XDisplay, device: *mut Device) -> *mut DeviceState;
    fn XFreeDeviceState(state: *mut DeviceState);
}
#[link(name = "X11")]
extern "C" {
    fn XQueryExtension(
        dpy: *mut XDisplay,
        name: *const i8,
        opcode: *mut c_int,
        event: *mut c_int,
        error: *mut c_int,
    ) -> c_int;
}

pub(super) use super::inventory::Identity;
use super::inventory::{self, Observation};

/// XIAllDevices includes disabled devices; the XKB server propagation loop
/// visits enabled devices. Retain enabled key-capable descendants and the
/// concrete core master, in master-first order. Unknown topology fails closed.
pub(super) fn inventory(dpy: *mut XDisplay, master: u16) -> Result<Vec<Identity>> {
    let observations = read_devices(dpy)?
        .into_iter()
        .map(|device| device.topology)
        .collect::<Vec<_>>();
    inventory::affected(master, &observations)
}

struct ObservedDevice {
    topology: Observation,
    key_sources: Vec<i32>,
}

/// The inventory and mutation-identity observer share the same bounded parser.
fn read_devices(dpy: *mut XDisplay) -> Result<Vec<ObservedDevice>> {
    require_xi2(dpy)?;
    let mut count = 0;
    let raw = unsafe { XIQueryDevice(dpy, 0, &mut count) };
    if raw.is_null() {
        return Err(err("input_failed", "XIQueryDevice failed"));
    }
    struct InfoGuard(*mut DeviceInfo);
    impl Drop for InfoGuard {
        fn drop(&mut self) {
            unsafe {
                XIFreeDeviceInfo(self.0);
            }
        }
    }
    let _guard = InfoGuard(raw);
    if count <= 0 || count as usize > MAX_DEVICES {
        return Err(err(
            "unsupported_session",
            "unbounded or empty XInput device inventory",
        ));
    }
    let infos = unsafe { std::slice::from_raw_parts(raw, count as usize) };
    let mut observations = Vec::with_capacity(infos.len());
    for info in infos {
        if info.num_classes < 0
            || info.num_classes > 256
            || (info.num_classes > 0 && info.classes.is_null())
        {
            return Err(err("input_failed", "unobservable XI device classes"));
        }
        let mut key_sources = Vec::new();
        for index in 0..info.num_classes as usize {
            let class = unsafe { *info.classes.add(index) };
            if class.is_null() {
                return Err(err("input_failed", "missing XI class"));
            }
            if unsafe { (*class).kind == 0 } {
                // XIKeyClass
                key_sources.push(unsafe { (*class).source });
            }
        }
        if info.name.is_null() {
            return Err(err("input_failed", "missing XI device name"));
        }
        observations.push(ObservedDevice {
            topology: Observation {
                id: info.id,
                kind: info.kind,
                attachment: info.attachment,
                enabled: info.enabled != 0,
                has_keys: !key_sources.is_empty(),
                name: unsafe { CStr::from_ptr(info.name).to_bytes().to_vec() },
            },
            key_sources,
        });
    }
    Ok(observations)
}

#[path = "target_observation.rs"]
mod target_observation;
pub(super) use target_observation::mutation_inventory;

/// XI1 native handles are needed only for attached non-master devices. No
/// events are selected and no input is generated by opening/querying them.
pub(super) fn observe(
    dpy: *mut XDisplay,
    id: u16,
    min: u8,
    max: u8,
) -> Result<(Vec<KeySym>, usize, [u8; 32])> {
    let mapping_query = super::device_keymap::query(id, min, max)?;
    let raw = unsafe { XOpenDevice(dpy, id as c_ulong) };
    if raw.is_null() {
        return Err(err(
            "input_failed",
            "XOpenDevice failed; affected state unsaved",
        ));
    }
    struct Handle(*mut XDisplay, *mut Device);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                XCloseDevice(self.0, self.1);
            }
        }
    }
    let _handle = Handle(dpy, raw);
    let state = unsafe { XQueryDeviceState(dpy, raw) };
    if state.is_null() {
        return Err(err("input_failed", "XQueryDeviceState failed"));
    }
    struct StateGuard(*mut DeviceState);
    impl Drop for StateGuard {
        fn drop(&mut self) {
            unsafe {
                XFreeDeviceState(self.0);
            }
        }
    }
    let _state = StateGuard(state);
    let state = unsafe { &*state };
    if state.num_classes <= 0 || state.num_classes > 256 || state.data.is_null() {
        return Err(err("input_failed", "missing affected-device key state"));
    }
    let mut cursor = state.data;
    let mut keys = None;
    for _ in 0..state.num_classes {
        let (kind, length) = unsafe { (*cursor, *cursor.add(1) as usize) };
        if length < 2 {
            return Err(err("input_failed", "invalid XI state record"));
        }
        if kind == 0 {
            if length < std::mem::size_of::<KeyState>() {
                return Err(err("input_failed", "short XI key state"));
            }
            let value = unsafe { ptr::read_unaligned(cursor.cast::<KeyState>()) };
            if keys.is_some() || value.num_keys < 0 || value.num_keys > 256 {
                return Err(err("input_failed", "invalid XI key-state geometry"));
            }
            keys = Some(value.keys);
        }
        cursor = unsafe { cursor.add(length) };
    }
    let keys = keys.ok_or_else(|| err("input_failed", "XI device has no observable keys"))?;
    let (map, width) = super::device_keymap::read(dpy, &mapping_query)?;
    // Same bit numbering as XQueryKeymap; retain each device's independent state.
    Ok((map, width, keys))
}

/// Negotiate XI2 on this Display before event selection or device queries.
pub(super) fn require_xi2(dpy: *mut XDisplay) -> Result<c_int> {
    let (mut opcode, mut event, mut error) = (0, 0, 0);
    if unsafe {
        XQueryExtension(
            dpy,
            c"XInputExtension".as_ptr(),
            &mut opcode,
            &mut event,
            &mut error,
        )
    } == 0
    {
        return Err(err(
            "unsupported_session",
            "native mapping leases require XInput device inventory",
        ));
    }
    let (mut major, mut minor) = (2, 0);
    if unsafe { XIQueryVersion(dpy, &mut major, &mut minor) } != 0 || major < 2 {
        return Err(err(
            "unsupported_session",
            "native mapping leases require XI2 device inventory",
        ));
    }
    Ok(opcode)
}
