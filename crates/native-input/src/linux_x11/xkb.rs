//! Minimal XKB ABI and owned per-key snapshots. Layouts match X11/XKBlib.h and
//! extensions/XKBstr.h. No process-global Xlib error handler is installed.
//! All requests use the driver's sole Display; no cross-connection waits.

use super::{err, KeyCode, KeySym, Result, XDisplay};
use std::cell::Cell;
use std::ffi::{c_int, c_uint, c_ulong, c_void};
use std::rc::Rc;

const CORE_KEYBOARD: c_uint = 0x100;
const ALL_MAP: c_uint = 0xff;

#[repr(C)]
#[derive(Default)]
struct State {
    group: u8,
    locked_group: u8,
    base_group: u16,
    latched_group: u16,
    mods: u8,
    base_mods: u8,
    latched_mods: u8,
    locked_mods: u8,
    compat_state: u8,
    grab_mods: u8,
    compat_grab_mods: u8,
    lookup_mods: u8,
    compat_lookup_mods: u8,
    ptr_buttons: u16,
}
#[repr(C)]
struct Mods {
    mask: u8,
    real_mods: u8,
    vmods: u16,
}
#[repr(C)]
struct KeyType {
    mods: Mods,
    num_levels: u8,
    map_count: u8,
    map: *mut c_void,
    preserve: *mut c_void,
    name: c_ulong,
    level_names: *mut c_ulong,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SymMap {
    types: [u8; 4],
    group_info: u8,
    width: u8,
    offset: u16,
}
#[repr(C)]
struct ClientMap {
    size_types: u8,
    num_types: u8,
    types: *mut KeyType,
    size_syms: u16,
    num_syms: u16,
    syms: *mut KeySym,
    sym_map: *mut SymMap,
    modmap: *mut u8,
}
#[repr(C)]
struct ServerMap {
    num_acts: u16,
    size_acts: u16,
    acts: *mut [u8; 8],
    behaviors: *mut [u8; 2],
    key_acts: *mut u16,
    explicit: *mut u8,
    vmods: [u8; 16],
    vmodmap: *mut u16,
}
#[repr(C)]
struct Controls {
    mk_dflt_btn: u8,
    num_groups: u8,
    groups_wrap: u8,
    internal: Mods,
    ignore_lock: Mods,
    enabled_ctrls: c_uint,
    repeat_delay: u16,
    repeat_interval: u16,
    slow_keys_delay: u16,
    debounce_delay: u16,
    mk_delay: u16,
    mk_interval: u16,
    mk_time_to_max: u16,
    mk_max_speed: u16,
    mk_curve: i16,
    ax_options: u16,
    ax_timeout: u16,
    axt_opts_mask: u16,
    axt_opts_values: u16,
    axt_ctrls_mask: c_uint,
    axt_ctrls_values: c_uint,
    per_key_repeat: [u8; 32],
}
#[repr(C)]
struct Desc {
    dpy: *mut XDisplay,
    flags: u16,
    device_spec: u16,
    min_key_code: u8,
    max_key_code: u8,
    ctrls: *mut Controls,
    server: *mut ServerMap,
    map: *mut ClientMap,
    indicators: *mut c_void,
    names: *mut c_void,
    compat: *mut c_void,
    geom: *mut c_void,
}
use super::definition::map_backend;
use super::map_request::{check_descriptor, Changes, NativeMapRequest};

// LP64 layouts also checked against the installed upstream C headers using
// compile-only _Static_asserts. These fail compilation on accidental ABI drift.
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<State>() == 18);
    assert!(std::mem::size_of::<KeyType>() == 40);
    assert!(std::mem::size_of::<SymMap>() == 8);
    assert!(std::mem::size_of::<ClientMap>() == 48);
    assert!(std::mem::size_of::<ServerMap>() == 64);
    assert!(std::mem::size_of::<Desc>() == 72);
    assert!(std::mem::size_of::<Controls>() == 84);
    assert!(std::mem::size_of::<Changes>() == 22);
};

#[link(name = "X11")]
extern "C" {
    fn XkbQueryExtension(
        dpy: *mut XDisplay,
        opcode: *mut c_int,
        event: *mut c_int,
        error: *mut c_int,
        major: *mut c_int,
        minor: *mut c_int,
    ) -> c_int;
    fn XkbGetControls(dpy: *mut XDisplay, which: c_ulong, desc: *mut Desc) -> c_int;
    fn XkbGetMap(dpy: *mut XDisplay, which: c_uint, device: c_uint) -> *mut Desc;
    fn XkbFreeKeyboard(desc: *mut Desc, which: c_uint, free_desc: c_int);
    fn XkbGetState(dpy: *mut XDisplay, device: c_uint, state: *mut State) -> c_int;
    fn XkbTranslateKeyCode(
        desc: *mut Desc,
        key: KeyCode,
        mods: c_uint,
        consumed: *mut c_uint,
        symbol: *mut KeySym,
    ) -> c_int;
    fn XQueryKeymap(dpy: *mut XDisplay, keys: *mut u8) -> c_int;
    fn XGrabServer(dpy: *mut XDisplay) -> c_int;
    fn XUngrabServer(dpy: *mut XDisplay) -> c_int;
    fn XFlush(dpy: *mut XDisplay) -> c_int;
}

pub(super) fn require_extension(dpy: *mut XDisplay) -> Result<()> {
    let (mut opcode, mut event, mut error, mut major, mut minor) = (0, 0, 0, 1, 0);
    if unsafe {
        XkbQueryExtension(
            dpy,
            &mut opcode,
            &mut event,
            &mut error,
            &mut major,
            &mut minor,
        )
    } == 0
    {
        return Err(err(
            "unsupported_session",
            "native X11 text requires the XKB extension",
        ));
    }
    Ok(())
}

/// Refuse shortcuts, latches and non-base groups rather than releasing someone
/// else's keys or consuming their latched state. No modifier events are sent.
pub(super) fn require_neutral_state(dpy: *mut XDisplay, device: Option<u16>) -> Result<()> {
    let mut state = State::default();
    let device = device.map(c_uint::from).unwrap_or(CORE_KEYBOARD);
    if unsafe { XkbGetState(dpy, device, &mut state) } != 0 {
        return Err(err("input_failed", "XkbGetState failed before native text"));
    }
    if state.mods != 0
        || state.lookup_mods != 0
        || state.compat_state != 0
        || state.base_mods != 0
        || state.latched_mods != 0
        || state.locked_mods != 0
        || state.group != 0
        || state.base_group != 0
        || state.latched_group != 0
        || state.locked_group != 0
    {
        return Err(err("input_failed", "native text requires neutral modifiers and base keyboard group; existing user state was not changed"));
    }
    Ok(())
}

pub(super) fn pressed_keys(dpy: *mut XDisplay) -> Result<[u8; 32]> {
    let mut keys = [0; 32];
    if unsafe { XQueryKeymap(dpy, keys.as_mut_ptr()) } == 0 {
        return Err(err("input_failed", "XQueryKeymap failed"));
    }
    Ok(keys)
}
pub(super) fn is_pressed(keys: &[u8; 32], key: u8) -> bool {
    keys[key as usize / 8] & (1 << (key % 8)) != 0
}

/// Bounded same-connection transactions only: no sleeps, foreign-client waits
/// or whole-string/session grab. Nesting shares a depth counter so an inner
/// cleanup helper can NEVER ungrab its caller's authorization critical section.
/// Xlib round trips retain the documented blocking-I/O limitation.
pub(super) struct ServerGrab {
    dpy: *mut XDisplay,
    depth: Rc<Cell<usize>>,
}
impl ServerGrab {
    pub fn new(dpy: *mut XDisplay, depth: Rc<Cell<usize>>) -> Self {
        if depth.get() == 0 {
            unsafe {
                XGrabServer(dpy);
            }
        }
        depth.set(depth.get() + 1);
        Self { dpy, depth }
    }
}
impl Drop for ServerGrab {
    fn drop(&mut self) {
        self.depth.set(self.depth.get() - 1);
        if self.depth.get() == 0 {
            unsafe {
                XUngrabServer(self.dpy);
                XFlush(self.dpy);
            }
        }
    }
}

pub(super) use super::definition::Definition;

pub(super) struct Snapshot(*mut Desc);
impl Snapshot {
    pub fn device_id(&self) -> u16 {
        unsafe { (*self.0).device_spec }
    }

    pub fn get(dpy: *mut XDisplay) -> Result<Self> {
        Self::get_device(dpy, CORE_KEYBOARD)
    }
    pub fn range(&self) -> (u8, u8) {
        unsafe { ((*self.0).min_key_code, (*self.0).max_key_code) }
    }
    pub fn get_device(dpy: *mut XDisplay, device: c_uint) -> Result<Self> {
        let raw = unsafe { XkbGetMap(dpy, ALL_MAP, device) };
        if raw.is_null() {
            return Err(err("input_failed", "XkbGetMap failed"));
        }
        let value = Self(raw);
        if unsafe { XkbGetControls(dpy, 1 << 30, raw) } != 0 || unsafe { (*raw).ctrls.is_null() } {
            return Err(err(
                "input_failed",
                "XKB per-key repeat controls could not be saved",
            ));
        }
        if device != CORE_KEYBOARD && value.device_id() as c_uint != device {
            return Err(err(
                "input_failed",
                "XKB device identity changed during snapshot",
            ));
        }
        if value.range().0 < 8 || value.range().0 > value.range().1 {
            return Err(err("input_failed", "invalid XKB keycode range"));
        }
        // SAFETY: descriptor and component arrays belong to this Xlib snapshot.
        unsafe {
            let d = &*raw;
            if d.map.is_null() || d.server.is_null() {
                return Err(err("input_failed", "incomplete XKB map"));
            }
            let (m, s) = (&*d.map, &*d.server);
            if m.types.is_null()
                || m.sym_map.is_null()
                || m.modmap.is_null()
                || m.syms.is_null()
                || s.key_acts.is_null()
                || s.behaviors.is_null()
                || s.explicit.is_null()
                || s.vmodmap.is_null()
            {
                return Err(err("input_failed", "incomplete XKB component arrays"));
            }
        }
        Ok(value)
    }
    fn check_key(&self, key: u8) -> Result<()> {
        let d = unsafe { &*self.0 };
        if key < d.min_key_code || key > d.max_key_code {
            return Err(err(
                "input_failed",
                "XKB keycode range changed; mapping ownership unknown",
            ));
        }
        Ok(())
    }
    pub fn require_one_level_type(&self) -> Result<()> {
        unsafe {
            if (*(*self.0).ctrls).num_groups == 0 {
                return Err(err(
                    "unsupported_session",
                    "native lease would change global XKB group controls",
                ));
            }
            let map = &*(*self.0).map;
            if map.num_types == 0 {
                return Err(err("input_failed", "XKB has no ONE_LEVEL type"));
            }
            let t = &*map.types;
            if t.num_levels != 1
                || t.mods.mask != 0
                || t.mods.real_mods != 0
                || t.mods.vmods != 0
                || t.map_count != 0
            {
                return Err(err(
                    "input_failed",
                    "XKB ONE_LEVEL type was changed; cannot safely map native text",
                ));
            }
        }
        Ok(())
    }
    pub fn definition(&self, key: u8) -> Result<Definition> {
        self.check_key(key)?;
        unsafe {
            let (m, s) = (&*(*self.0).map, &*(*self.0).server);
            let sym = *m.sym_map.add(key as usize);
            let count = (sym.group_info & 0xf) as usize * sym.width as usize;
            if sym.group_info & 0xf > 4 || sym.offset as usize + count > m.num_syms as usize {
                return Err(err("input_failed", "invalid XKB symbol geometry"));
            }
            let symbols = if count == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(m.syms.add(sym.offset as usize), count).to_vec()
            };
            let action_offset = *s.key_acts.add(key as usize) as usize;
            let actions = if action_offset == 0 || count == 0 {
                Vec::new()
            } else {
                if s.acts.is_null() || action_offset + count > s.num_acts as usize {
                    return Err(err("input_failed", "invalid XKB action geometry"));
                }
                std::slice::from_raw_parts(s.acts.add(action_offset), count).to_vec()
            };
            Ok(Definition {
                types: sym.types,
                group_info: sym.group_info,
                width: sym.width,
                symbols,
                repeat: Some(is_pressed(&(*(*self.0).ctrls).per_key_repeat, key)),
                actions,
                behavior: *s.behaviors.add(key as usize),
                explicit: *s.explicit.add(key as usize),
                modmap: *m.modmap.add(key as usize),
                vmodmap: *s.vmodmap.add(key as usize),
            })
        }
    }
    /// No new X request: extract groups from this owned GetMap/GetControls
    /// snapshot. The shared journal policy, not this adapter, decides safety.
    pub fn group_observation(
        &self,
    ) -> Result<super::map_request::lease_journal::map_groups::GroupObservation> {
        let range = self.range();
        let groups_by_key = (range.0..=range.1)
            .map(|key| self.definition(key).map(|d| d.group_info & 15))
            .collect::<Result<Vec<_>>>()?;
        // SAFETY: get_device checked ctrls; it belongs to this live snapshot.
        let num_groups = unsafe { (*(*self.0).ctrls).num_groups };
        Ok(super::map_request::lease_journal::map_groups::GroupObservation {
            device_id: self.device_id(),
            range,
            num_groups,
            groups_by_key,
        })
    }
    pub fn effective_symbol(&self, key: u8) -> Result<KeySym> {
        self.check_key(key)?;
        let (mut consumed, mut symbol) = (0, 0);
        if unsafe { XkbTranslateKeyCode(self.0, key, 0, &mut consumed, &mut symbol) } == 0 {
            return Err(err(
                "invalid_text",
                "XKB resolves candidate key to NoSymbol",
            ));
        }
        Ok(symbol)
    }
    fn check_request(&self, request: &NativeMapRequest, explicit_only: bool) -> Result<()> {
        check_descriptor(request, self.device_id(), self.range(), explicit_only)
    }

    /// Explicit-only final restoration keeps its distinct single-field request.
    /// The public writer never requests compatibility/action recomputation.
    pub fn write_explicit(
        &mut self,
        dpy: *mut XDisplay,
        request: NativeMapRequest,
        explicit: u8,
    ) -> Result<()> {
        self.check_request(&request, true)?;
        map_backend::submit(
            &mut CheckedMapWriter { dpy },
            &request,
            map_backend::map_wire::Payload::Explicit(explicit),
        )
    }
    /// Serialize the validated owned value, not a mutated Xlib allocation.
    /// Callers still read every recipient back before confirming/retiring a key.
    pub fn write_key(
        &mut self,
        dpy: *mut XDisplay,
        request: NativeMapRequest,
        def: &Definition,
    ) -> Result<()> {
        self.check_request(&request, false)?;
        map_backend::submit(
            &mut CheckedMapWriter { dpy },
            &request,
            map_backend::map_wire::Payload::Full(def),
        )
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        unsafe {
            XkbFreeKeyboard(self.0, 0, 1);
        }
    }
}

/// Actual checked writer on the sole guarded Display. No Xlib map-write
/// fallback, new connection, queue ownership transfer or nested server grab.
struct CheckedMapWriter {
    dpy: *mut XDisplay,
}
impl map_backend::Transport for CheckedMapWriter {
    fn set_map(&mut self, prepared: &map_backend::map_wire::PreparedMap<'_>) -> Result<()> {
        map_backend::map_wire::xcb::write(self.dpy, prepared)
    }
}
