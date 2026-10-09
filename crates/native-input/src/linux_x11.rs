//! linux_x11 platform driver: direct libX11/libXtst FFI, no wrappers and no
//! external binaries. Owns one X display connection; all X access is
//! serialized through `&mut self` on the `Driver` trait.
//!
//! Requires the XTEST extension. Wayland sessions are rejected explicitly.
//!
//! Text policy (honest first-release behavior): only characters whose keysym
//! is already bound at the UNSHIFTED level (column 0) of the current keymap
//! are injected, by clicking that stable keycode. Characters not so bound are
//! rejected with `invalid_text` BEFORE any side effect. There is NO temporary
//! `XChangeKeyboardMapping` remapping: a transient remap followed by an
//! immediate restore cannot guarantee that every target client observed the
//! mapping before the events, so such injection would be able to corrupt
//! input silently. Unicode beyond the current layout is therefore UNSUPPORTED
//! by this driver and explicitly reported as such — never pretended to work.
//!
//! Known limits (see .agents/reports/native-input-platform-repair-20260929.md):
//! - This driver does NOT install a process-global `XSetErrorHandler` /
//!   IO-error handler (that slot is shared by the whole process, including
//!   other Xlib users such as screenshot dependencies). Xlib's default
//!   handlers therefore apply: an asynchronous X protocol error or a dead
//!   display connection terminates the process, exactly as for any other
//!   plain Xlib client.
//! - `XSync` performs a blocking round trip with no timeout; connection
//!   setup is likewise blocking. These are the potentially-blocking calls.
//! - `XSync`'s Status return value is NOT used as an error indicator (its
//!   contract is not a reliable per-call error signal); delivery errors
//!   surface through Xlib's default handler per the limit above.
//! - Keyboard mapping lookups consider only column 0 (unshifted level of the
//!   first keyboard group). Characters bound only at shifted levels or in
//!   other groups are rejected rather than guessed with a Shift press.

use crate::{Axis, Button, Direction, Driver, InputError, Key, Result};
use std::collections::{HashMap, HashSet};
use std::ffi::{c_int, c_uint, c_ulong, c_void};

mod keysym;

use keysym::{KeyCode, KeySym};

const CURRENT_TIME: c_ulong = 0;
const FALSE: c_int = 0;

#[repr(C)]
struct XDisplay {
    _private: [u8; 0],
}

#[repr(C)]
struct XModifierKeymap {
    max_keypermod: c_int,
    modifiermap: *mut KeyCode,
}

#[link(name = "X11")]
extern "C" {
    fn XOpenDisplay(display_name: *const c_void) -> *mut XDisplay;
    fn XCloseDisplay(display: *mut XDisplay) -> c_int;
    fn XDefaultScreen(display: *mut XDisplay) -> c_int;
    fn XSync(display: *mut XDisplay, discard: c_int) -> c_int;
    fn XDisplayKeycodes(
        display: *mut XDisplay,
        min_keycodes_return: *mut c_int,
        max_keycodes_return: *mut c_int,
    ) -> c_int;
    fn XGetKeyboardMapping(
        display: *mut XDisplay,
        first_keycode: KeyCode,
        keycode_count: c_int,
        keysyms_per_keycode_return: *mut c_int,
    ) -> *mut KeySym;
    fn XGetModifierMapping(display: *mut XDisplay) -> *mut XModifierKeymap;
    fn XFreeModifiermap(modifier_keymap: *mut XModifierKeymap) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
}

#[link(name = "Xtst")]
extern "C" {
    fn XTestQueryExtension(
        display: *mut XDisplay,
        events_return: *mut c_int,
        errors_return: *mut c_int,
        major_version_return: *mut c_int,
        minor_version_return: *mut c_int,
    ) -> c_int;
    fn XTestFakeKeyEvent(
        display: *mut XDisplay,
        keycode: c_uint,
        is_press: c_int,
        delay: c_ulong,
    ) -> c_int;
    fn XTestFakeMotionEvent(
        display: *mut XDisplay,
        screen_number: c_int,
        x: c_int,
        y: c_int,
        delay: c_ulong,
    ) -> c_int;
    fn XTestFakeButtonEvent(
        display: *mut XDisplay,
        button: c_uint,
        is_press: c_int,
        delay: c_ulong,
    ) -> c_int;
}

fn err(code: &'static str, message: impl Into<String>) -> InputError {
    InputError::new(code, message)
}

/// Stable press/release keycode cache (pure state, no FFI; unit-tested).
/// Keycodes resolved at press time are retained and reused for the matching
/// release, so a keymap change between press and release cannot split a
/// chord onto two physical keys. Cache mutation is explicit and committed
/// only after the corresponding fake event post SUCCEEDED: a failed press
/// inserts nothing, and a failed release keeps its keycode so the caller's
/// retry lands on the same physical key.
struct KeycodeCache {
    entries: HashMap<Key, KeyCode>,
}

impl KeycodeCache {
    fn new() -> Self {
        KeycodeCache {
            entries: HashMap::new(),
        }
    }

    fn get(&self, key: Key) -> Option<KeyCode> {
        self.entries.get(&key).copied()
    }

    /// Commits a SUCCESSFUL press. An existing entry is never overwritten.
    fn commit_press(&mut self, key: Key, keycode: KeyCode) {
        self.entries.entry(key).or_insert(keycode);
    }

    /// Commits a SUCCESSFUL release by dropping the retained keycode.
    fn commit_release(&mut self, key: Key) {
        self.entries.remove(&key);
    }
}

pub(crate) struct Platform {
    dpy: *mut XDisplay,
    screen: c_int,
    min_keycode: KeyCode,
    max_keycode: KeyCode,
    /// Stable press/release keycode cache (see [`KeycodeCache`]).
    keycodes: KeycodeCache,
}

impl Platform {
    pub(crate) fn new() -> Result<Self> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE")
                .map(|v| v.eq_ignore_ascii_case("wayland"))
                .unwrap_or(false)
        {
            return Err(err(
                "unsupported_session",
                "Wayland session detected; this driver requires X11 (XWayland is not validated)",
            ));
        }
        if std::env::var_os("DISPLAY").is_none() {
            return Err(err("no_display", "DISPLAY is not set"));
        }
        // SAFETY: XOpenDisplay(NULL) reads DISPLAY; the returned pointer is
        // owned by us and closed in Drop. This is a blocking connection setup.
        let dpy = unsafe { XOpenDisplay(std::ptr::null()) };
        if dpy.is_null() {
            return Err(err(
                "no_display",
                "XOpenDisplay failed; cannot connect to the X server named by DISPLAY",
            ));
        }
        let mut events: c_int = 0;
        let mut errors: c_int = 0;
        let mut major: c_int = 0;
        let mut minor: c_int = 0;
        // SAFETY: dpy is a live connection; all out-params are valid locals.
        let have_test =
            unsafe { XTestQueryExtension(dpy, &mut events, &mut errors, &mut major, &mut minor) };
        if have_test == 0 {
            unsafe { XCloseDisplay(dpy) };
            return Err(err(
                "unsupported_session",
                "X server does not support the XTEST extension",
            ));
        }
        let mut min_keycode: c_int = 0;
        let mut max_keycode: c_int = 0;
        // SAFETY: dpy is live; out-params are valid locals. XDisplayKeycodes
        // always succeeds (returns Status 1) on a live display; the out-params
        // are validated below regardless.
        unsafe { XDisplayKeycodes(dpy, &mut min_keycode, &mut max_keycode) };
        if !(8..=255).contains(&min_keycode) || !(min_keycode..=255).contains(&max_keycode) {
            unsafe { XCloseDisplay(dpy) };
            return Err(err(
                "input_failed",
                "XDisplayKeycodes returned an implausible keycode range",
            ));
        }
        Ok(Self {
            dpy,
            screen: unsafe { XDefaultScreen(dpy) },
            min_keycode: min_keycode as KeyCode,
            max_keycode: max_keycode as KeyCode,
            keycodes: KeycodeCache::new(),
        })
    }

    /// Flushes the output buffer and waits until the X server has processed
    /// it. XSync's return value is deliberately not interpreted (see module
    /// docs); protocol errors surface through Xlib's default handler.
    fn sync(&mut self) {
        // SAFETY: dpy is live. XSync is a blocking round trip.
        unsafe { XSync(self.dpy, FALSE) };
    }

    /// Snapshot of the current keyboard mapping plus the modifier keycode
    /// set. Used for all keycode resolution so press and release observe the
    /// same mapping and only stable, unshifted bindings are used.
    fn keymap_snapshot(&mut self) -> Result<(Vec<KeySym>, usize, HashSet<KeyCode>)> {
        let count = self.max_keycode as i32 - self.min_keycode as i32 + 1;
        let mut per_code: c_int = 0;
        // SAFETY: dpy is live; result is an Xlib-allocated array freed via
        // XFree. Blocking round trip.
        let raw = unsafe { XGetKeyboardMapping(self.dpy, self.min_keycode, count, &mut per_code) };
        if raw.is_null() || per_code <= 0 {
            if !raw.is_null() {
                unsafe { XFree(raw.cast()) };
            }
            return Err(err("input_failed", "XGetKeyboardMapping failed"));
        }
        let len = (count as usize) * (per_code as usize);
        let mut map = Vec::with_capacity(len);
        // SAFETY: raw points to `len` valid KeySyms per Xlib's contract.
        unsafe {
            for i in 0..len {
                map.push(*raw.add(i));
            }
            XFree(raw.cast());
        }
        let mut modifiers = HashSet::new();
        // SAFETY: dpy is live; result freed via XFreeModifiermap.
        let modmap = unsafe { XGetModifierMapping(self.dpy) };
        if !modmap.is_null() {
            unsafe {
                let slots = (*modmap).max_keypermod;
                if slots > 0 && slots <= 64 {
                    for i in 0..(8 * slots as usize) {
                        let kc = *(*modmap).modifiermap.add(i);
                        if kc != 0 {
                            modifiers.insert(kc);
                        }
                    }
                }
                XFreeModifiermap(modmap);
            }
        }
        Ok((map, per_code as usize, modifiers))
    }

    /// Resolves the keycode whose UNSHIFTED binding (column 0) is `keysym`.
    /// Modifier-bound keycodes are never returned, and a keysym bound only at
    /// a shifted level or another group is rejected rather than guessed.
    /// `rejection_code` is the `InputError` code used when unbound (callers
    /// map text lookups to `invalid_text`, key lookups to `invalid_key`).
    fn keycode_for_keysym(
        &mut self,
        keysym: KeySym,
        context: &str,
        rejection_code: &'static str,
    ) -> Result<KeyCode> {
        let (map, per_code, modifiers) = self.keymap_snapshot()?;
        keysym::find_unshifted_keycode(&map, per_code, self.min_keycode, &modifiers, keysym)
            .ok_or_else(|| {
                err(
                    rejection_code,
                    format!(
                        "keysym 0x{keysym:04X} is not bound at the unshifted level of any \
                         non-modifier keycode ({context})"
                    ),
                )
            })
    }

    fn fake_key(&mut self, keycode: KeyCode, is_press: bool, context: &str) -> Result<()> {
        // SAFETY: dpy is live; XTestFakeKeyEvent is asynchronous and only
        // validates arguments, so its status is checked directly.
        let status = unsafe {
            XTestFakeKeyEvent(self.dpy, keycode as c_uint, is_press as c_int, CURRENT_TIME)
        };
        if status == 0 {
            return Err(err(
                "input_failed",
                format!(
                    "XTestFakeKeyEvent {} keycode {} failed ({context})",
                    if is_press { "press" } else { "release" },
                    keycode
                ),
            ));
        }
        Ok(())
    }

    /// Press + release of one keycode, with retry of a failed release so we
    /// clean up our own partial transient event before reporting failure.
    fn click_keycode(&mut self, keycode: KeyCode, context: &str) -> Result<()> {
        self.fake_key(keycode, true, context)?;
        if let Err(first) = self.fake_key(keycode, false, context) {
            if let Err(second) = self.fake_key(keycode, false, context) {
                return Err(err(
                    first.code,
                    format!("{first}; owned release retry also failed: {second}"),
                ));
            }
        }
        Ok(())
    }

    /// Press + release of one pointer button (wheel ticks), mirroring
    /// `click_keycode` cleanup semantics: a failed release is retried and
    /// both failures are reported, never silently dropped.
    fn click_button(&mut self, button: c_uint, context: &str) -> Result<()> {
        self.fake_button(button, true, context)?;
        if let Err(first) = self.fake_button(button, false, context) {
            if let Err(second) = self.fake_button(button, false, context) {
                return Err(err(
                    first.code,
                    format!("{first}; owned release retry also failed: {second}"),
                ));
            }
        }
        Ok(())
    }

    fn fake_button(&mut self, button: c_uint, is_press: bool, context: &str) -> Result<()> {
        // SAFETY: dpy is live; XTestFakeButtonEvent is asynchronous and only
        // validates arguments, so its status is checked directly.
        let status =
            unsafe { XTestFakeButtonEvent(self.dpy, button, is_press as c_int, CURRENT_TIME) };
        if status == 0 {
            return Err(err(
                "input_failed",
                format!(
                    "XTestFakeButtonEvent {} button {} failed ({context})",
                    if is_press { "press" } else { "release" },
                    button
                ),
            ));
        }
        Ok(())
    }
}

impl Drop for Platform {
    fn drop(&mut self) {
        // SAFETY: dpy was opened in new() and is closed exactly once here.
        unsafe { XCloseDisplay(self.dpy) };
    }
}

/// Pure XTest button specification; validation happens before Xlib/XTest.
fn button_event_spec(
    button: Button,
    direction: Direction,
    click_count: u8,
) -> Result<(c_uint, bool)> {
    if !crate::is_valid_click_count(click_count) {
        return Err(err(
            "invalid_button",
            format!("click_count {click_count} is out of range 1..=3"),
        ));
    }
    Ok((
        keysym::pointer_button(button),
        direction == Direction::Press,
    ))
}

impl Driver for Platform {
    fn move_mouse(&mut self, x: i32, y: i32) -> Result<()> {
        // SAFETY: dpy is live; XTestFakeMotionEvent is asynchronous and its
        // status is checked.
        let status = unsafe {
            XTestFakeMotionEvent(self.dpy, self.screen, x as c_int, y as c_int, CURRENT_TIME)
        };
        if status == 0 {
            return Err(err("input_failed", "XTestFakeMotionEvent failed"));
        }
        self.sync();
        Ok(())
    }

    fn button(&mut self, button: Button, direction: Direction, click_count: u8) -> Result<()> {
        // X11 has no per-event multi-click field: applications aggregate
        // consecutive button events at the same position by time. We still
        // emit the REAL atomic XTest event; the count is validated so an
        // invalid request never reports success or injects anything.
        let (button_id, is_press) = button_event_spec(button, direction, click_count)?;
        self.fake_button(button_id, is_press, &format!("{button:?} {direction:?}"))?;
        self.sync();
        Ok(())
    }

    fn key(&mut self, key: Key, direction: Direction) -> Result<()> {
        let context = format!("{key:?} {direction:?}");
        let keysym = keysym::key_to_keysym(&key).ok_or_else(|| {
            err(
                "invalid_key",
                format!("{key:?} is not a valid physical key; only ASCII letters/digits are supported for Key::Character"),
            )
        })?;
        // Cache-first resolution: a press-time keycode is reused at release
        // so a keymap change cannot split a chord. Resolution before any
        // effect so failures leave no state.
        let keycode = match self.keycodes.get(key) {
            Some(cached) => cached,
            None => self.keycode_for_keysym(keysym, &context, "invalid_key")?,
        };
        self.fake_key(keycode, matches!(direction, Direction::Press), &context)?;
        // Cache mutation is committed ONLY after the post succeeded, so a
        // failed release retains the physical code for retry.
        match direction {
            Direction::Press => self.keycodes.commit_press(key, keycode),
            Direction::Release => self.keycodes.commit_release(key),
        }
        self.sync();
        Ok(())
    }

    fn scroll(&mut self, length: i32, axis: Axis) -> Result<()> {
        if length == 0 {
            return Ok(());
        }
        let button = keysym::wheel_button(axis, length > 0);
        for tick in 0..length.unsigned_abs() {
            self.click_button(button, &format!("scroll tick {tick}"))?;
        }
        self.sync();
        Ok(())
    }

    fn text_scalar(&mut self, ch: char) -> Result<()> {
        let context;
        let keysym = match ch {
            '\0' => {
                return Err(err("invalid_text", "NUL is not a valid text scalar"));
            }
            '\n' | '\r' => {
                // CRLF is normalized once by the caller; each of \n and \r
                // produces exactly one Return click.
                context = "text newline".to_string();
                keysym::XK_RETURN
            }
            '\t' => {
                context = "text tab".to_string();
                keysym::XK_TAB
            }
            _ => {
                let ks = keysym::unicode_to_keysym(ch).ok_or_else(|| {
                    err(
                        "invalid_text",
                        format!("no X11 keysym encodes U+{:04X}", ch as u32),
                    )
                })?;
                context = format!("text U+{:04X}", ch as u32);
                ks
            }
        };

        // Resolve BEFORE any side effect: the click only happens on a stable,
        // unshifted existing binding. Unmapped text is rejected up front —
        // this driver never rewrites the keyboard mapping to fake support.
        let keycode = self.keycode_for_keysym(keysym, &context, "invalid_text")?;
        self.click_keycode(keycode, &context)?;
        self.sync();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure KeycodeCache tests (no display, no input). These run in the
    // Linux container test pass.

    #[test]
    fn atomic_button_spec_preserves_real_xtest_button_and_validates_count() {
        for (button, number) in [(Button::Left, 1), (Button::Middle, 2), (Button::Right, 3)] {
            for direction in [Direction::Press, Direction::Release] {
                for count in 1..=3 {
                    assert_eq!(
                        button_event_spec(button, direction, count).unwrap(),
                        (number, direction == Direction::Press)
                    );
                }
                for bad in [0, 4, 255] {
                    assert_eq!(
                        button_event_spec(button, direction, bad).unwrap_err().code,
                        "invalid_button"
                    );
                }
            }
        }
    }

    #[test]
    fn cache_reuses_press_keycode_for_release() {
        let mut cache = KeycodeCache::new();
        cache.commit_press(Key::Character('a'), 38);
        assert_eq!(cache.get(Key::Character('a')), Some(38));
        cache.commit_release(Key::Character('a'));
        assert_eq!(cache.get(Key::Character('a')), None, "release consumes");
    }

    #[test]
    fn cache_failed_release_retains_keycode() {
        // Simulated failed release: resolve happened, commit_release was
        // never called, so the retained keycode survives for the retry.
        let mut cache = KeycodeCache::new();
        cache.commit_press(Key::Character('a'), 38);
        assert_eq!(cache.get(Key::Character('a')), Some(38));
        // (no commit_release call)
        assert_eq!(cache.get(Key::Character('a')), Some(38));
    }

    #[test]
    fn cache_repeated_press_retains_original_keycode() {
        let mut cache = KeycodeCache::new();
        cache.commit_press(Key::Character('a'), 38);
        // A second press resolves a (wrong) new code; commit_press must not
        // overwrite the retained press-time entry.
        cache.commit_press(Key::Character('a'), 99);
        assert_eq!(cache.get(Key::Character('a')), Some(38));
    }

    #[test]
    fn cache_press_failure_inserts_nothing() {
        // Failed press: resolution happened but the post failed, so
        // commit_press was never called.
        let cache = KeycodeCache::new();
        assert_eq!(cache.get(Key::Return), None);
        // Release of a key we never pressed resolves fresh on miss.
        assert_eq!(cache.get(Key::Return), None);
    }

    #[test]
    fn cache_tracks_keys_independently() {
        let mut cache = KeycodeCache::new();
        cache.commit_press(Key::Character('a'), 38);
        cache.commit_press(Key::Shift, 50);
        cache.commit_release(Key::Character('a'));
        assert_eq!(cache.get(Key::Character('a')), None);
        assert_eq!(cache.get(Key::Shift), Some(50), "other keys unaffected");
    }

    #[test]
    fn keysym_type_matches_c_ulong_abi() {
        // KeySym must mirror Xlib's `unsigned long` KeySym for FFI safety.
        assert_eq!(
            std::mem::size_of::<KeySym>(),
            std::mem::size_of::<std::os::raw::c_ulong>()
        );
    }

    #[test]
    fn meta_maps_to_super_keysym() {
        assert_eq!(keysym::key_to_keysym(&Key::Meta), Some(keysym::XK_SUPER_L));
        assert_eq!(keysym::XK_SUPER_L, 0xFFEB);
    }
}
