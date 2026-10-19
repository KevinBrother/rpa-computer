//! Persistent XIHierarchyChanged subscription on the driver's own Xlib Display.
//! All selection/fencing/draining happens under its existing short server grab.
//! A reply fence moves earlier server events into Xlib's queue; a bounded drain
//! advances the portable lifetime epoch. No application acknowledgment is used.
use super::inventory::HierarchyTransport;
use super::{devices, err, Result, XDisplay};
use std::cell::Cell;
use std::ffi::{c_int, c_long, c_uint, c_ulong, c_void};
use std::rc::Rc;

const GENERIC_EVENT: c_int = 35;
const HIERARCHY_CHANGED: c_int = 11;
const MAX_QUEUED_EVENTS: c_int = 4096;

#[repr(C)]
struct EventMask {
    device: c_int,
    len: c_int,
    mask: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Cookie {
    kind: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut XDisplay,
    extension: c_int,
    evtype: c_int,
    cookie: c_uint,
    data: *mut c_void,
}
#[repr(C)]
union Event {
    kind: c_int,
    cookie: Cookie,
    pad: [c_long; 24],
}
#[repr(C)]
struct HierarchyEvent {
    kind: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut XDisplay,
    extension: c_int,
    evtype: c_int,
    time: c_ulong,
    flags: c_int,
    num_info: c_int,
    info: *mut c_void,
}
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<EventMask>() == 16);
    assert!(std::mem::size_of::<Cookie>() == 56);
    assert!(std::mem::size_of::<Event>() == 192);
    assert!(std::mem::size_of::<HierarchyEvent>() == 64);
};

#[link(name = "Xi")]
extern "C" {
    fn XISelectEvents(
        dpy: *mut XDisplay,
        window: c_ulong,
        masks: *mut EventMask,
        count: c_int,
    ) -> c_int;
    fn XIGetSelectedEvents(
        dpy: *mut XDisplay,
        window: c_ulong,
        count: *mut c_int,
    ) -> *mut EventMask;
}
#[link(name = "X11")]
extern "C" {
    fn XRootWindow(dpy: *mut XDisplay, screen: c_int) -> c_ulong;
    fn XSync(dpy: *mut XDisplay, discard: c_int) -> c_int;
    fn XEventsQueued(dpy: *mut XDisplay, mode: c_int) -> c_int;
    fn XNextEvent(dpy: *mut XDisplay, event: *mut Event) -> c_int;
    fn XGetEventData(dpy: *mut XDisplay, cookie: *mut Cookie) -> c_int;
    fn XFreeEventData(dpy: *mut XDisplay, cookie: *mut Cookie);
    fn XFree(data: *mut c_void) -> c_int;
}

pub(super) struct HierarchyWatch {
    dpy: *mut XDisplay,
    grab_depth: Rc<Cell<usize>>,
    opcode: Option<c_int>,
    fenced_queue: Option<c_int>,
}
impl HierarchyWatch {
    pub fn new(dpy: *mut XDisplay, grab_depth: Rc<Cell<usize>>) -> Self {
        Self {
            dpy,
            grab_depth,
            opcode: None,
            fenced_queue: None,
        }
    }
    fn require_grab(&self) -> Result<()> {
        if self.grab_depth.get() == 0 {
            return Err(err(
                "input_failed",
                "device lifetime fence requires the operation's server grab",
            ));
        }
        Ok(())
    }
    fn consume_hierarchy(&self, mut cookie: Cookie) -> Result<()> {
        if cookie.display != self.dpy
            || Some(cookie.extension) != self.opcode
            || cookie.evtype != HIERARCHY_CHANGED
        {
            return Err(err(
                "input_failed",
                "unexpected generic event in private hierarchy stream",
            ));
        }
        // Xlib/libXi own decoding and allocation. Claim each cookie BEFORE the
        // next XNextEvent, and free it on every path after a successful claim.
        if unsafe { XGetEventData(self.dpy, &mut cookie) } == 0 {
            return Err(err(
                "input_failed",
                "XI hierarchy cookie could not be decoded; history uncertain",
            ));
        }
        struct DataGuard(*mut XDisplay, Cookie);
        impl Drop for DataGuard {
            fn drop(&mut self) {
                unsafe {
                    XFreeEventData(self.0, &mut self.1);
                }
            }
        }
        let data = DataGuard(self.dpy, cookie);
        if data.1.data.is_null() {
            return Err(err("input_failed", "XI hierarchy cookie has no data"));
        }
        let hierarchy = unsafe { &*data.1.data.cast::<HierarchyEvent>() };
        if hierarchy.kind != GENERIC_EVENT
            || hierarchy.display != self.dpy
            || Some(hierarchy.extension) != self.opcode
            || hierarchy.evtype != HIERARCHY_CHANGED
            || hierarchy.num_info < 0
            || hierarchy.num_info > 256
            || (hierarchy.num_info > 0 && hierarchy.info.is_null())
        {
            return Err(err(
                "input_failed",
                "invalid XI hierarchy data; history uncertain",
            ));
        }
        // Retire all earlier leases on ANY hierarchy event. This deliberately
        // does not try to identify a replacement from potentially reused IDs.
        Ok(())
    }
}
impl HierarchyTransport for HierarchyWatch {
    fn subscribe(&mut self) -> Result<()> {
        self.require_grab()?;
        if self.opcode.is_some() {
            return Ok(());
        }
        let opcode = devices::require_xi2(self.dpy)?;
        let root = unsafe { XRootWindow(self.dpy, 0) };
        if root == 0 {
            return Err(err(
                "input_failed",
                "missing root for XI hierarchy subscription",
            ));
        }
        let mut bits = [0u8, 1u8 << (HIERARCHY_CHANGED % 8)];
        let mut mask = EventMask {
            device: 0,
            len: bits.len() as c_int,
            mask: bits.as_mut_ptr(),
        };
        // Hierarchy events require XIAllDevices, not one currently alive ID.
        if unsafe { XISelectEvents(self.dpy, root, &mut mask, 1) } != 0 {
            return Err(err(
                "input_failed",
                "XI hierarchy subscription request failed",
            ));
        }
        let mut count = 0;
        let selected = unsafe { XIGetSelectedEvents(self.dpy, root, &mut count) };
        if selected.is_null() {
            return Err(err(
                "input_failed",
                "XI hierarchy subscription could not be verified",
            ));
        }
        struct MaskGuard(*mut EventMask);
        impl Drop for MaskGuard {
            fn drop(&mut self) {
                unsafe {
                    XFree(self.0.cast());
                }
            }
        }
        let _guard = MaskGuard(selected);
        if count <= 0 || count > 256 {
            return Err(err("input_failed", "invalid XI selected-event mask count"));
        }
        let masks = unsafe { std::slice::from_raw_parts(selected, count as usize) };
        let verified = masks.iter().any(|mask| {
            mask.device == 0
                && mask.len >= 2
                && !mask.mask.is_null()
                && unsafe { *mask.mask.add(1) & bits[1] != 0 }
        });
        if !verified {
            return Err(err(
                "input_failed",
                "XI hierarchy subscription missing from server readback",
            ));
        }
        self.opcode = Some(opcode);
        Ok(())
    }

    fn reply_fence(&mut self) -> Result<()> {
        self.require_grab()?;
        if self.opcode.is_none() {
            return Err(err(
                "input_failed",
                "hierarchy subscription not established",
            ));
        }
        // XSync(false) is an ordering fence, not an error-status or application
        // receipt. Existing Xlib protocol/IO handler limitations still apply.
        unsafe {
            XSync(self.dpy, 0);
        }
        // QueuedAlready: do not poll indefinitely for later events while grabbed.
        let count = unsafe { XEventsQueued(self.dpy, 0) };
        if !(0..=MAX_QUEUED_EVENTS).contains(&count) {
            return Err(err(
                "input_failed",
                "hierarchy event backlog exceeds bounded drain; history uncertain",
            ));
        }
        self.fenced_queue = Some(count);
        Ok(())
    }

    fn drain_changes(&mut self) -> Result<u64> {
        self.require_grab()?;
        let count = self
            .fenced_queue
            .take()
            .ok_or_else(|| err("input_failed", "event drain has no reply fence"))?;
        let mut changes = 0;
        for _ in 0..count {
            // Sole consumer of this private Display. Only drain the captured
            // local queue length, so XNextEvent cannot wait for another client.
            let mut event = Event { pad: [0; 24] };
            unsafe {
                XNextEvent(self.dpy, &mut event);
            }
            if unsafe { event.kind } == GENERIC_EVENT {
                self.consume_hierarchy(unsafe { event.cookie })?;
                changes += 1;
            }
            // Core MappingNotify/XKB notifications are not used as caches:
            // keyboard_set re-queries core/XKB definitions on this connection.
        }
        Ok(changes)
    }
}
