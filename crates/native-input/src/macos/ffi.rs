//! Raw CoreGraphics / CoreFoundation / ApplicationServices / Carbon FFI
//! declarations with Apple-SDK-verified type widths, plus CF/CGEvent RAII.
//!
//! Type widths verified against the installed Xcode SDK headers:
//! - `CGEventType`, `CGMouseButton`, `CGScrollEventUnit`, `CGEventTapLocation`,
//!   `CGEventField` are `CF_ENUM(uint32_t, ...)` (CGEventTypes.h:39-402);
//!   `kCGScrollEventUnitPixel = 0`, `kCGScrollEventUnitLine = 1`
//!   (CGEventTypes.h:47-48).
//! - `CGEventSourceStateID` is `CF_ENUM(int32_t, ...)` (CGEventTypes.h:480).
//! - `CGEventCreateScrollWheelEvent2` takes SIX parameters in total
//!   (`source, units, wheelCount, wheel1, wheel2, wheel3`), of which the last
//!   three are the int32 wheel deltas (CGEvent.h:107-109).
//! - `CGEventKeyboardSetUnicodeString` takes `UniCharCount` = `unsigned long`
//!   (CGEvent.h:205-206, CFBase.h:139).
//! - `UCKeyTranslate` takes `UniCharCount maxStringLength` and
//!   `UniCharCount *actualStringLength` (exported by CoreServices' CarbonCore
//!   framework; canonical Carbon declaration HIToolbox/UnicodeInput.h), i.e.
//!   `c_ulong` on 64-bit macOS.
//! - Event-flag masks come from IOKit/hidsystem/IOLLEvent.h:
//!   `NX_NONCOALSESCEDMASK = 0x00000100` (IOLLEvent.h:268), NOT a
//!   `0x0100_0000`-style mask; `NX_DEVICEL*KEYMASK` = 0x01/0x02/0x08/0x20
//!   (IOLLEvent.h:253-258).

#![allow(non_snake_case, non_upper_case_globals)]

use std::os::raw::{c_ulong, c_void};

use super::InputError;
use crate::Result;

// ---------------------------------------------------------------------------
// Opaque handle types
// ---------------------------------------------------------------------------

pub(super) type CGEventRef = *mut c_void;
pub(super) type CGEventSourceRef = *mut c_void;
pub(super) type CFDataRef = *const c_void;
pub(super) type TISInputSourceRef = *const c_void;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

// kCGHIDEventTap
pub(super) const CG_HID_EVENT_TAP: u32 = 0;
// kCGEventSourceStateHIDSystemState
pub(super) const CG_EVENT_SOURCE_STATE_HID_SYSTEM: i32 = 1;
// kCGScrollEventUnitLine (CGEventTypes.h:47-48: Pixel=0, Line=1).
pub(super) const CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;

// CGEventType values (CF_ENUM(uint32_t, CGEventType))
pub(super) const EVENT_LEFT_MOUSE_DOWN: u32 = 1;
pub(super) const EVENT_LEFT_MOUSE_UP: u32 = 2;
pub(super) const EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
pub(super) const EVENT_RIGHT_MOUSE_UP: u32 = 4;
pub(super) const EVENT_MOUSE_MOVED: u32 = 5;
pub(super) const EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
pub(super) const EVENT_RIGHT_MOUSE_DRAGGED: u32 = 7;
pub(super) const EVENT_OTHER_MOUSE_DOWN: u32 = 25;
pub(super) const EVENT_OTHER_MOUSE_UP: u32 = 26;
pub(super) const EVENT_OTHER_MOUSE_DRAGGED: u32 = 27;

// kCGMouseEventClickState
pub(super) const FIELD_MOUSE_CLICK_STATE: u32 = 1;
#[cfg(test)]
pub(super) const FIELD_MOUSE_BUTTON_NUMBER: u32 = 3;

// CGEventFlags (NX_* masks, IOKit/hidsystem/IOLLEvent.h)
pub(super) const FLAG_SHIFT: u64 = 0x0002_0000; // NX_SHIFTMASK
pub(super) const FLAG_CONTROL: u64 = 0x0004_0000; // NX_CONTROLMASK
pub(super) const FLAG_ALTERNATE: u64 = 0x0008_0000; // NX_ALTERNATEMASK
pub(super) const FLAG_COMMAND: u64 = 0x0010_0000; // NX_COMMANDMASK
pub(super) const FLAG_NON_COALESCED: u64 = 0x0000_0100; // NX_NONCOALSESCEDMASK
                                                        // NX_DEVICEL* device-dependent key masks (IOLLEvent.h:253-258)
pub(super) const FLAG_DEVICE_LEFT_CONTROL: u64 = 0x0000_0001;
pub(super) const FLAG_DEVICE_LEFT_SHIFT: u64 = 0x0000_0002;
pub(super) const FLAG_DEVICE_LEFT_COMMAND: u64 = 0x0000_0008;
pub(super) const FLAG_DEVICE_LEFT_ALT: u64 = 0x0000_0020;

// Mouse button ids for CGEventCreateMouseEvent (CF_ENUM(uint32_t, CGMouseButton))
pub(super) const MOUSE_BUTTON_LEFT: u32 = 0;
pub(super) const MOUSE_BUTTON_RIGHT: u32 = 1;
pub(super) const MOUSE_BUTTON_CENTER: u32 = 2;

// ---------------------------------------------------------------------------
// Structures
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CGPoint {
    pub(super) x: f64,
    pub(super) y: f64,
}

// ---------------------------------------------------------------------------
// CoreGraphics
// ---------------------------------------------------------------------------

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub(super) fn CGEventSourceCreate(stateID: i32) -> CGEventSourceRef;
    pub(super) fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    pub(super) fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    #[cfg(test)]
    pub(super) fn CGEventGetType(event: CGEventRef) -> u32;
    #[cfg(test)]
    pub(super) fn CGEventGetFlags(event: CGEventRef) -> u64;
    #[cfg(test)]
    pub(super) fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    pub(super) fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        keyCode: u16,
        keyDown: bool,
    ) -> CGEventRef;
    pub(super) fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouseType: u32,
        mouseCursorPosition: CGPoint,
        mouseButton: u32,
    ) -> CGEventRef;
    /// Six parameters total: source, units, wheelCount, wheel1, wheel2,
    /// wheel3 (CGEvent.h:107-109; wheel3 added in 10.13).
    pub(super) fn CGEventCreateScrollWheelEvent2(
        source: CGEventSourceRef,
        units: u32,
        wheelCount: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;
    pub(super) fn CGEventPost(tap: u32, event: CGEventRef);
    pub(super) fn CGEventSetFlags(event: CGEventRef, flags: u64);
    pub(super) fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub(super) fn CGEventKeyboardSetUnicodeString(
        event: CGEventRef,
        stringLength: c_ulong,
        unicodeString: *const u16,
    );
    pub(super) fn CGMainDisplayID() -> u32;
}

// ---------------------------------------------------------------------------
// CoreFoundation
// ---------------------------------------------------------------------------

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub(super) fn CFRelease(cf: *const c_void);
    pub(super) fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
    pub(super) fn CFDataGetLength(data: CFDataRef) -> c_ulong;
}

// ---------------------------------------------------------------------------
// ApplicationServices / Carbon (TIS + UCKeyTranslate)
// ---------------------------------------------------------------------------

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub(super) fn AXIsProcessTrusted() -> bool;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub(super) fn TISCopyCurrentKeyboardLayoutInputSource() -> TISInputSourceRef;
    pub(super) static kTISPropertyUnicodeKeyLayoutData: *const c_void;
    pub(super) fn TISGetInputSourceProperty(
        inputSource: TISInputSourceRef,
        propertyKey: *const c_void,
    ) -> CFDataRef;
    /// OSStatus return (SInt32). `maxStringLength`/`actualStringLength` are
    /// `UniCharCount` = `unsigned long` — NOT UInt32. `keyLayoutPtr` points at
    /// the CFData bytes of `kTISPropertyUnicodeKeyLayoutData`.
    pub(super) fn UCKeyTranslate(
        keyLayoutPtr: *const u8,
        virtualKeyCode: u16,
        keyAction: u16,
        modifierKeyState: u32,
        keyboardType: u32,
        keyTranslateOptions: u32,
        deadKeyState: *mut u32,
        maxStringLength: c_ulong,
        actualStringLength: *mut c_ulong,
        unicodeString: *mut u16,
    ) -> i32;
    pub(super) fn LMGetKbdType() -> u8;
}

// ---------------------------------------------------------------------------
// RAII
// ---------------------------------------------------------------------------

/// Owned CF object reference; releases on drop.
pub(super) struct CfRef(pub(super) *const c_void);

impl CfRef {
    pub(super) fn retain(ptr: *const c_void) -> Option<CfRef> {
        if ptr.is_null() {
            None
        } else {
            Some(CfRef(ptr))
        }
    }
}

impl Drop for CfRef {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

/// Owned CGEvent reference; releases on drop.
pub(super) struct Event(pub(super) CGEventRef);

impl Event {
    pub(super) fn take(ptr: CGEventRef) -> Result<Event> {
        if ptr.is_null() {
            Err(InputError::new(
                "input_failed",
                "CoreGraphics failed to create the input event",
            ))
        } else {
            Ok(Event(ptr))
        }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0 as *const c_void) };
    }
}

pub(super) struct EventSource(pub(super) CfRef);

impl EventSource {
    pub(super) fn create() -> Result<EventSource> {
        let ptr = unsafe { CGEventSourceCreate(CG_EVENT_SOURCE_STATE_HID_SYSTEM) };
        match CfRef::retain(ptr) {
            Some(cf) => Ok(EventSource(cf)),
            None => Err(InputError::new(
                "input_failed",
                "failed to create the CoreGraphics event source",
            )),
        }
    }

    pub(super) fn raw(&self) -> CGEventSourceRef {
        self.0 .0 as CGEventSourceRef
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression: these constants were previously wrong (scroll unit 0 =
    // kCGScrollEventUnitPixel; non-coalesced 0x0100_0000 instead of
    // IOLLEvent.h's NX_NONCOALSESCEDMASK). The asserted values are copied
    // from the installed SDK headers, not from the constants themselves.
    #[test]
    fn sdk_constants_match_installed_headers() {
        assert_eq!(CG_SCROLL_EVENT_UNIT_LINE, 1); // CGEventTypes.h:48
        assert_eq!(FLAG_NON_COALESCED, 0x0000_0100); // IOLLEvent.h:268
        assert_eq!(FLAG_SHIFT, 0x0002_0000); // IOLLEvent.h NX_SHIFTMASK
        assert_eq!(FLAG_CONTROL, 0x0004_0000); // NX_CONTROLMASK
        assert_eq!(FLAG_ALTERNATE, 0x0008_0000); // NX_ALTERNATEMASK
        assert_eq!(FLAG_COMMAND, 0x0010_0000); // NX_COMMANDMASK
        assert_eq!(FLAG_DEVICE_LEFT_CONTROL, 0x01); // IOLLEvent.h:253
        assert_eq!(FLAG_DEVICE_LEFT_SHIFT, 0x02); // IOLLEvent.h:254
        assert_eq!(FLAG_DEVICE_LEFT_COMMAND, 0x08); // IOLLEvent.h:256
        assert_eq!(FLAG_DEVICE_LEFT_ALT, 0x20); // IOLLEvent.h:258
    }
}
