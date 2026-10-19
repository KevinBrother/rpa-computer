//! Minimal test-only libXtst input. No SendEvent, map edits, or all-key reset.
use std::{
    ffi::{c_char, c_int, c_uint, c_ulong, c_void, CString},
    ptr,
};

#[link(name = "X11")]
extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
}
#[link(name = "Xtst")]
extern "C" {
    fn XTestQueryExtension(
        display: *mut c_void,
        event: *mut c_int,
        error: *mut c_int,
        major: *mut c_int,
        minor: *mut c_int,
    ) -> c_int;
    fn XTestFakeKeyEvent(display: *mut c_void, key: c_uint, press: c_int, delay: c_ulong) -> c_int;
}

pub(super) struct Input {
    display: *mut c_void,
    held: Vec<u8>,
}
impl Input {
    pub fn open(display: &str) -> Self {
        let name = CString::new(display).unwrap();
        // SAFETY: explicit checked local display and live NUL-terminated string.
        let display = unsafe { XOpenDisplay(name.as_ptr()) };
        assert!(!display.is_null(), "cannot open fixture XTEST connection");
        let result = Self {
            display,
            held: Vec::new(),
        };
        let (mut event, mut error, mut major, mut minor) = (0, 0, 0, 0);
        // SAFETY: owned Display and valid integer outputs, used on this thread only.
        assert_ne!(
            unsafe { XTestQueryExtension(display, &mut event, &mut error, &mut major, &mut minor) },
            0,
            "XTEST is required"
        );
        result
    }
    pub fn press(&mut self, key: u8) {
        assert!(
            !self.held.contains(&key),
            "fixture attempted duplicate held key"
        );
        // Record before submitting: unwind cleanup releases even a failed submit.
        self.held.push(key);
        assert!(self.send(key, true), "XTEST key-down failed");
        self.sync();
    }
    pub fn release_all(&mut self) {
        while let Some(key) = self.held.pop() {
            let _ = self.send(key, false);
        }
        self.sync();
    }
    pub fn tap(&mut self, key: u8) {
        self.press(key);
        let _ = self.send(key, false);
        self.held.pop();
        self.sync();
    }
    fn send(&self, key: u8, press: bool) -> bool {
        // SAFETY: live exclusively owned Display; protocol keycode is bounded.
        unsafe { XTestFakeKeyEvent(self.display, u32::from(key), i32::from(press), 0) != 0 }
    }
    pub fn sync(&self) {
        // SAFETY: live Display, do not discard any queued events.
        unsafe {
            XSync(self.display, 0);
        }
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        if self.display != ptr::null_mut() {
            self.release_all();
            // SAFETY: exclusive ownership, released once after owned-key cleanup.
            unsafe {
                XCloseDisplay(self.display);
            }
            self.display = ptr::null_mut();
        }
    }
}
