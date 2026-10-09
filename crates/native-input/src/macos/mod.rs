//! macOS native input driver backed by direct CoreGraphics / CoreFoundation /
//! Carbon (TIS) FFI. No wrapper crates, no AppleScript, no shell, no clipboard.
//!
//! Events are posted on `kCGHIDEventTap` through a private HID event source.
//! Accessibility permission is checked with `AXIsProcessTrusted` (never
//! prompts) in `Platform::new`.
//!
//! Character keycodes resolved on `Key::Press` are cached in `Platform` and
//! reused for the matching `Key::Release`, so a keyboard-layout change between
//! press and release cannot split a chord onto two physical keys.

use std::collections::HashMap;

use crate::{Axis, Button, Direction, Driver, InputError, Key, Result};

mod ffi;
mod keymap;
#[cfg(test)]
mod mouse_tests;

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested, no event posting)
// ---------------------------------------------------------------------------

/// Bitmask of tracked held mouse buttons.
const HELD_LEFT: u8 = 0b001;
const HELD_RIGHT: u8 = 0b010;
const HELD_MIDDLE: u8 = 0b100;

fn held_bit(button: Button) -> u8 {
    match button {
        Button::Left => HELD_LEFT,
        Button::Right => HELD_RIGHT,
        Button::Middle => HELD_MIDDLE,
    }
}

/// (event type, CG mouse button id) for a move while `held` buttons are down.
fn move_event_type(held: u8) -> (u32, u32) {
    if held & HELD_LEFT != 0 {
        (ffi::EVENT_LEFT_MOUSE_DRAGGED, ffi::MOUSE_BUTTON_LEFT)
    } else if held & HELD_RIGHT != 0 {
        (ffi::EVENT_RIGHT_MOUSE_DRAGGED, ffi::MOUSE_BUTTON_RIGHT)
    } else if held & HELD_MIDDLE != 0 {
        (ffi::EVENT_OTHER_MOUSE_DRAGGED, ffi::MOUSE_BUTTON_CENTER)
    } else {
        (ffi::EVENT_MOUSE_MOVED, ffi::MOUSE_BUTTON_LEFT)
    }
}

/// A plain move has no click-state. Drag motion belongs to the single
/// press/up pair used for drag actions, never to a later multi-click pair.
fn move_event_spec(held: u8) -> (u32, u32, u8) {
    let (mouse_type, button_id) = move_event_type(held);
    (mouse_type, button_id, if held == 0 { 0 } else { 1 })
}

/// Construct a mouse event without posting it. Both production dispatch and
/// no-input API regressions use this seam, so tests can read the native
/// fields instead of merely checking a parallel Rust tuple.
fn create_mouse_event(
    source: ffi::CGEventSourceRef,
    mouse_type: u32,
    point: ffi::CGPoint,
    button_id: u32,
    click_state: u8,
    flags: u64,
) -> Result<ffi::Event> {
    let event = ffi::Event::take(unsafe {
        ffi::CGEventCreateMouseEvent(source, mouse_type, point, button_id)
    })?;
    unsafe {
        ffi::CGEventSetIntegerValueField(
            event.0,
            ffi::FIELD_MOUSE_CLICK_STATE,
            i64::from(click_state),
        );
        ffi::CGEventSetFlags(event.0, flags);
    }
    Ok(event)
}

/// (event type, CG mouse button id) for a button press or release.
fn button_event_type(button: Button, direction: Direction) -> (u32, u32) {
    match (button, direction) {
        (Button::Left, Direction::Press) => (ffi::EVENT_LEFT_MOUSE_DOWN, ffi::MOUSE_BUTTON_LEFT),
        (Button::Left, Direction::Release) => (ffi::EVENT_LEFT_MOUSE_UP, ffi::MOUSE_BUTTON_LEFT),
        (Button::Right, Direction::Press) => (ffi::EVENT_RIGHT_MOUSE_DOWN, ffi::MOUSE_BUTTON_RIGHT),
        (Button::Right, Direction::Release) => (ffi::EVENT_RIGHT_MOUSE_UP, ffi::MOUSE_BUTTON_RIGHT),
        (Button::Middle, Direction::Press) => {
            (ffi::EVENT_OTHER_MOUSE_DOWN, ffi::MOUSE_BUTTON_CENTER)
        }
        (Button::Middle, Direction::Release) => {
            (ffi::EVENT_OTHER_MOUSE_UP, ffi::MOUSE_BUTTON_CENTER)
        }
    }
}

/// Full CGEvent mouse spec for a button event: (event type, CG mouse button
/// id, mouse click-state). The click-state is the caller-supplied
/// `click_count` — the native multi-click position of THIS press/up pair —
/// placed in `FIELD_MOUSE_CLICK_STATE` of the posted event. Invalid counts
/// are rejected before any event construction.
fn button_event_spec(
    button: Button,
    direction: Direction,
    click_count: u8,
) -> Result<(u32, u32, u8)> {
    if !crate::is_valid_click_count(click_count) {
        return Err(InputError::new(
            "invalid_button",
            format!("click_count {click_count} is out of range 1..=3"),
        ));
    }
    let (mouse_type, button_id) = button_event_type(button, direction);
    Ok((mouse_type, button_id, click_count))
}

/// (wheelCount, wheel1, wheel2, wheel3) for a scroll of `length` ticks on
/// `axis`. Contract: positive length means RIGHT (horizontal) / DOWN
/// (vertical), and CoreGraphics encodes "towards the user / down" as negative
/// wheel values. The unused wheel axes are always zero (wheelCount 1 or 2).
fn scroll_params(length: i32, axis: Axis) -> (u32, i32, i32, i32) {
    match axis {
        Axis::Vertical => (1, -length, 0, 0),
        Axis::Horizontal => (2, 0, -length, 0),
    }
}

/// CGEventFlags contributed by a held named modifier key (left-hand device
/// masks included).
fn modifier_event_flag(key: Key) -> Option<u64> {
    match key {
        Key::Control => Some(ffi::FLAG_CONTROL | ffi::FLAG_DEVICE_LEFT_CONTROL),
        Key::Shift => Some(ffi::FLAG_SHIFT | ffi::FLAG_DEVICE_LEFT_SHIFT),
        Key::Alt => Some(ffi::FLAG_ALTERNATE | ffi::FLAG_DEVICE_LEFT_ALT),
        Key::Meta => Some(ffi::FLAG_COMMAND | ffi::FLAG_DEVICE_LEFT_COMMAND),
        _ => None,
    }
}

/// Applies a modifier press/release to the running event-flag set.
fn apply_modifier_flags(flags: u64, key: Key, direction: Direction) -> u64 {
    match modifier_event_flag(key) {
        Some(mask) => {
            if direction == Direction::Press {
                flags | mask
            } else {
                flags & !mask
            }
        }
        None => flags,
    }
}

// ---------------------------------------------------------------------------
// Platform driver
// ---------------------------------------------------------------------------

/// Pluggable character-keycode resolution: production uses TIS layout
/// translation ([`keymap::char_to_keycode`]); pure tests inject a
/// deterministic fake so no live TIS is touched.
type CharResolver = Box<dyn Fn(char) -> Result<u16>>;

/// Pure keycode cache for `Key::Character`, separate from event posting so
/// cache mutation can be committed only after a successful post.
///
/// - Cache keys are normalized (ASCII letters lowercased), so `'a'` and `'A'`
///   alias the same physical key and cannot duplicate entries.
/// - A repeated press of an already-cached key RETAINS the original keycode
///   instead of re-resolving (and overwriting) it.
/// - A failed press inserts nothing; a failed release keeps the retained
///   code so the caller's retry after a layout switch still lands on the
///   same physical key.
struct CharacterKeycodeCache {
    entries: HashMap<Key, u16>,
    resolve: CharResolver,
}

impl CharacterKeycodeCache {
    fn new(resolve: CharResolver) -> Self {
        CharacterKeycodeCache {
            entries: HashMap::new(),
            resolve,
        }
    }

    fn normalized(key: Key) -> Key {
        match key {
            Key::Character(c) if c.is_ascii_uppercase() => Key::Character(c.to_ascii_lowercase()),
            other => other,
        }
    }

    /// Keycode for `key`, preferring the retained press-time code over a
    /// fresh (layout-dependent) resolution.
    fn code_for(&mut self, key: Key) -> Result<u16> {
        let key = Self::normalized(key);
        if let Some(&code) = self.entries.get(&key) {
            return Ok(code);
        }
        match key {
            Key::Character(ch) => (self.resolve)(ch),
            other => keymap::named_key_code(other).ok_or_else(|| {
                InputError::new(
                    "invalid_key",
                    format!("key {other:?} has no macOS keycode mapping"),
                )
            }),
        }
    }

    /// Commits a SUCCESSFUL press. An existing entry is never overwritten.
    fn commit_press(&mut self, key: Key, code: u16) {
        let key = Self::normalized(key);
        self.entries.entry(key).or_insert(code);
    }

    /// Commits a SUCCESSFUL release by dropping the retained code.
    fn commit_release(&mut self, key: Key) {
        let key = Self::normalized(key);
        self.entries.remove(&key);
    }
}

pub(crate) struct Platform {
    source: ffi::EventSource,
    /// Running modifier flags (NON_COALESCED + held named modifiers).
    flags: u64,
    /// Mouse buttons currently held from our own `button` calls.
    held_buttons: u8,
    /// Character keycodes resolved at press time, reused at release so a
    /// layout change between press and release cannot split a chord.
    cache: CharacterKeycodeCache,
}

impl Platform {
    pub(crate) fn new() -> Result<Self> {
        // Never prompts: AXIsProcessTrusted only reports current trust state.
        if !unsafe { ffi::AXIsProcessTrusted() } {
            return Err(InputError::new(
                "permission_denied",
                "process is not trusted for accessibility (input injection denied)",
            ));
        }
        if unsafe { ffi::CGMainDisplayID() } == 0 {
            return Err(InputError::new(
                "no_display",
                "no main display is available for input injection",
            ));
        }
        let source = ffi::EventSource::create()?;
        Ok(Platform {
            source,
            flags: ffi::FLAG_NON_COALESCED,
            held_buttons: 0,
            cache: CharacterKeycodeCache::new(Box::new(keymap::char_to_keycode)),
        })
    }

    fn post_key_event(&self, keycode: u16, key_down: bool, flags: u64) -> Result<()> {
        let event = ffi::Event::take(unsafe {
            ffi::CGEventCreateKeyboardEvent(self.source.raw(), keycode, key_down)
        })?;
        unsafe {
            ffi::CGEventSetFlags(event.0, flags);
            ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, event.0);
        }
        Ok(())
    }

    fn post_mouse_event(
        &self,
        mouse_type: u32,
        point: ffi::CGPoint,
        button_id: u32,
        click_state: u8,
    ) -> Result<()> {
        let event = create_mouse_event(
            self.source.raw(),
            mouse_type,
            point,
            button_id,
            click_state,
            self.flags,
        )?;
        unsafe { ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, event.0) };
        Ok(())
    }

    fn cursor_point(&self) -> Result<ffi::CGPoint> {
        // A probe event carries the current mouse location without posting.
        let probe = ffi::Event::take(unsafe { ffi::CGEventCreate(self.source.raw()) })?;
        Ok(unsafe { ffi::CGEventGetLocation(probe.0) })
    }

    /// Injects one scalar by UTF-16 string on a synthetic keyboard event
    /// (down then up, both carrying the same unicode string payload).
    /// Modifier flags are cleared so text is unaffected by chords.
    fn post_unicode_char(&mut self, ch: char) -> Result<()> {
        let (units, unit_count) = keymap::encode_char_units(ch);
        let unit_count = unit_count as std::os::raw::c_ulong;
        let unit_ptr = units.as_ptr();

        let down = ffi::Event::take(unsafe {
            ffi::CGEventCreateKeyboardEvent(self.source.raw(), 0, true)
        })?;
        unsafe {
            ffi::CGEventKeyboardSetUnicodeString(down.0, unit_count, unit_ptr);
            ffi::CGEventSetFlags(down.0, ffi::FLAG_NON_COALESCED);
            ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, down.0);
        }

        let up = ffi::Event::take(unsafe {
            ffi::CGEventCreateKeyboardEvent(self.source.raw(), 0, false)
        });
        match up {
            Ok(up) => {
                unsafe {
                    ffi::CGEventKeyboardSetUnicodeString(up.0, unit_count, unit_ptr);
                    ffi::CGEventSetFlags(up.0, ffi::FLAG_NON_COALESCED);
                    ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, up.0);
                }
                Ok(())
            }
            Err(err) => {
                // The down event was already posted; attempt to clean it up by
                // posting a matching release. If the cleanup ALSO fails, both
                // failures are reported instead of silently dropping either.
                let cleanup = (|| -> Result<()> {
                    let cu = ffi::Event::take(unsafe {
                        ffi::CGEventCreateKeyboardEvent(self.source.raw(), 0, false)
                    })?;
                    unsafe {
                        ffi::CGEventKeyboardSetUnicodeString(cu.0, unit_count, unit_ptr);
                        ffi::CGEventSetFlags(cu.0, ffi::FLAG_NON_COALESCED);
                        ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, cu.0);
                    }
                    Ok(())
                })();
                match cleanup {
                    Ok(()) => Err(err),
                    Err(cleanup_err) => Err(InputError::new(
                        err.code,
                        format!(
                            "{err}; cleanup release for the already-posted down event also failed: {cleanup_err}"
                        ),
                    )),
                }
            }
        }
    }
}

impl Driver for Platform {
    fn move_mouse(&mut self, x: i32, y: i32) -> Result<()> {
        let (mouse_type, button_id, click_state) = move_event_spec(self.held_buttons);
        let point = ffi::CGPoint {
            x: x as f64,
            y: y as f64,
        };
        self.post_mouse_event(mouse_type, point, button_id, click_state)
    }

    fn button(&mut self, button: Button, direction: Direction, click_count: u8) -> Result<()> {
        let (mouse_type, button_id, click_state) =
            button_event_spec(button, direction, click_count)?;
        let point = self.cursor_point()?;
        self.post_mouse_event(mouse_type, point, button_id, click_state)?;
        match direction {
            Direction::Press => self.held_buttons |= held_bit(button),
            Direction::Release => self.held_buttons &= !held_bit(button),
        }
        Ok(())
    }

    fn key(&mut self, key: Key, direction: Direction) -> Result<()> {
        // Resolution happens before any effect so failures leave no state.
        let keycode = match key {
            Key::Character(_) => self.cache.code_for(key)?,
            named => keymap::named_key_code(named).ok_or_else(|| {
                InputError::new(
                    "invalid_key",
                    format!("key {named:?} has no macOS keycode mapping"),
                )
            })?,
        };
        let new_flags = apply_modifier_flags(self.flags, key, direction);
        let key_down = direction == Direction::Press;
        self.post_key_event(keycode, key_down, new_flags)?;
        // Cache mutation is committed ONLY after the post succeeded, so a
        // failed release retains the physical code for retry.
        if let Key::Character(_) = key {
            match direction {
                Direction::Press => self.cache.commit_press(key, keycode),
                Direction::Release => self.cache.commit_release(key),
            }
        }
        self.flags = new_flags;
        Ok(())
    }

    fn scroll(&mut self, length: i32, axis: Axis) -> Result<()> {
        if length == 0 {
            // Zero ticks: nothing to inject, and a zero-delta event is not
            // meaningful input, so success here is honest.
            return Ok(());
        }
        let (wheel_count, wheel1, wheel2, wheel3) = scroll_params(length, axis);
        let event = ffi::Event::take(unsafe {
            ffi::CGEventCreateScrollWheelEvent2(
                self.source.raw(),
                ffi::CG_SCROLL_EVENT_UNIT_LINE,
                wheel_count,
                wheel1,
                wheel2,
                wheel3,
            )
        })?;
        unsafe {
            ffi::CGEventSetFlags(event.0, self.flags);
            ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, event.0);
        }
        Ok(())
    }

    fn text_scalar(&mut self, ch: char) -> Result<()> {
        keymap::validate_text_char(ch)?;
        match ch {
            '\n' | '\r' => {
                self.key(Key::Return, Direction::Press)?;
                self.key(Key::Return, Direction::Release)
            }
            '\t' => {
                self.key(Key::Tab, Direction::Press)?;
                self.key(Key::Tab, Direction::Release)
            }
            _ => self.post_unicode_char(ch),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (pure / non-live: no CGEventPost, no desktop input)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::keymap::vk;
    use super::*;

    // -- scroll / mouse helpers ---------------------------------------------

    #[test]
    fn scroll_positive_means_down_and_right() {
        assert_eq!(scroll_params(3, Axis::Vertical), (1, -3, 0, 0));
        assert_eq!(scroll_params(2, Axis::Horizontal), (2, 0, -2, 0));
        assert_eq!(scroll_params(-1, Axis::Vertical), (1, 1, 0, 0));
        // Unused wheel axes stay zero even for large ticks.
        assert_eq!(scroll_params(5, Axis::Horizontal), (2, 0, -5, 0));
    }

    #[test]
    fn move_uses_dragged_type_only_while_our_button_is_held() {
        assert_eq!(
            move_event_type(0),
            (ffi::EVENT_MOUSE_MOVED, ffi::MOUSE_BUTTON_LEFT)
        );
        assert_eq!(
            move_event_type(HELD_LEFT),
            (ffi::EVENT_LEFT_MOUSE_DRAGGED, ffi::MOUSE_BUTTON_LEFT)
        );
        assert_eq!(
            move_event_type(HELD_RIGHT),
            (ffi::EVENT_RIGHT_MOUSE_DRAGGED, ffi::MOUSE_BUTTON_RIGHT)
        );
        assert_eq!(
            move_event_type(HELD_MIDDLE),
            (ffi::EVENT_OTHER_MOUSE_DRAGGED, ffi::MOUSE_BUTTON_CENTER)
        );
        assert_eq!(
            move_event_type(HELD_LEFT | HELD_RIGHT),
            (ffi::EVENT_LEFT_MOUSE_DRAGGED, ffi::MOUSE_BUTTON_LEFT)
        );
    }

    #[test]
    fn button_event_types_are_down_and_up() {
        assert_eq!(
            button_event_type(Button::Left, Direction::Press),
            (ffi::EVENT_LEFT_MOUSE_DOWN, ffi::MOUSE_BUTTON_LEFT)
        );
        assert_eq!(
            button_event_type(Button::Left, Direction::Release),
            (ffi::EVENT_LEFT_MOUSE_UP, ffi::MOUSE_BUTTON_LEFT)
        );
        assert_eq!(
            button_event_type(Button::Right, Direction::Press),
            (ffi::EVENT_RIGHT_MOUSE_DOWN, ffi::MOUSE_BUTTON_RIGHT)
        );
        assert_eq!(
            button_event_type(Button::Middle, Direction::Release),
            (ffi::EVENT_OTHER_MOUSE_UP, ffi::MOUSE_BUTTON_CENTER)
        );
    }

    /// The pure event spec must carry the caller's click_count into the
    /// event's mouse click-state. Native readback lives in `mouse_tests`.
    #[test]
    fn button_event_spec_carries_click_count_and_rejects_invalid() {
        for count in 1..=3u8 {
            let (_, _, click_state) =
                button_event_spec(Button::Left, Direction::Press, count).unwrap();
            assert_eq!(
                click_state, count,
                "posted CGEvent click-state must equal the requested click_count"
            );
        }
        for bad in [0u8, 4, 255] {
            let err = button_event_spec(Button::Left, Direction::Press, bad).unwrap_err();
            assert_eq!(err.code, "invalid_button");
        }
    }

    // -- modifier flags -----------------------------------------------------

    #[test]
    fn modifier_flags_accumulate_and_clear() {
        let base = ffi::FLAG_NON_COALESCED;
        let pressed = apply_modifier_flags(base, Key::Shift, Direction::Press);
        assert_ne!(pressed & ffi::FLAG_SHIFT, 0);
        let pressed = apply_modifier_flags(pressed, Key::Control, Direction::Press);
        assert_ne!(pressed & ffi::FLAG_CONTROL, 0);
        assert_ne!(pressed & ffi::FLAG_DEVICE_LEFT_CONTROL, 0);
        let released = apply_modifier_flags(pressed, Key::Shift, Direction::Release);
        assert_eq!(released & ffi::FLAG_SHIFT, 0);
        assert_eq!(released & ffi::FLAG_DEVICE_LEFT_SHIFT, 0);
        assert_ne!(released & ffi::FLAG_CONTROL, 0);
        // Non-modifier keys leave flags untouched.
        assert_eq!(
            apply_modifier_flags(pressed, Key::Character('a'), Direction::Press),
            pressed
        );
    }

    // -- press/release keycode cache (deterministic fake resolver) ----------

    /// Counting fake resolver: returns `code`, recording each call.
    fn fake_resolver(code: u16, calls: Rc<Cell<usize>>) -> CharResolver {
        Box::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(code)
        })
    }

    #[test]
    fn cache_reuses_press_code_for_release_across_resolver_change() {
        // The resolver simulates a layout switch: the first resolution yields
        // 42, any later fresh resolution would yield 99. The release must
        // still use the press-time code.
        let calls = Rc::new(Cell::new(0));
        let mut cache = CharacterKeycodeCache::new(Box::new({
            let calls = calls.clone();
            move |_ch| {
                calls.set(calls.get() + 1);
                Ok(if calls.get() == 1 { 42 } else { 99 })
            }
        }));

        let press = cache.code_for(Key::Character('a')).expect("press resolves");
        assert_eq!(press, 42);
        cache.commit_press(Key::Character('a'), press);

        let release = cache
            .code_for(Key::Character('a'))
            .expect("release reuses cached code");
        assert_eq!(release, 42, "release keeps the press-time physical code");
        cache.commit_release(Key::Character('a'));
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn failed_release_retains_code_and_retry_uses_it() {
        let calls = Rc::new(Cell::new(0));
        let mut cache = CharacterKeycodeCache::new(fake_resolver(7, calls.clone()));
        cache.commit_press(Key::Character('a'), 7);

        // Simulate the failed-release path: the driver resolves, the post
        // fails, and commit_release is therefore NEVER called. The retry
        // (and any fresh resolution) must return the retained code.
        assert_eq!(cache.code_for(Key::Character('a')).unwrap(), 7);
        // No commit: entry stays.
        assert_eq!(calls.get(), 0, "retained code requires no fresh resolve");
        cache.commit_release(Key::Character('a')); // only on eventual success
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn uncommitted_press_is_not_retained() {
        // Proves the commit-after-success contract: a press whose post failed
        // (code_for called, commit_press never) does not poison the cache.
        let calls = Rc::new(Cell::new(0));
        let mut cache = CharacterKeycodeCache::new(fake_resolver(5, calls.clone()));
        let _ = cache.code_for(Key::Character('a')).unwrap();
        assert!(cache.entries.is_empty(), "resolve alone must not commit");
        // The next press resolves again (previous resolve was not committed).
        let _ = cache.code_for(Key::Character('a')).unwrap();
        assert_eq!(calls.get(), 2);
        cache.commit_press(Key::Character('a'), 5);
        let _ = cache.code_for(Key::Character('a')).unwrap();
        assert_eq!(calls.get(), 2, "committed press stops further resolves");
    }

    #[test]
    fn repeated_press_retains_original_keycode() {
        let calls = Rc::new(Cell::new(0));
        let mut cache = CharacterKeycodeCache::new(Box::new({
            let calls = calls.clone();
            move |_| {
                calls.set(calls.get() + 1);
                Ok(if calls.get() == 1 { 42 } else { 99 })
            }
        }));
        let first = cache.code_for(Key::Character('a')).unwrap();
        cache.commit_press(Key::Character('a'), first);
        // Same logical key pressed again while held: no re-resolve/overwrite.
        let again = cache.code_for(Key::Character('a')).unwrap();
        assert_eq!(again, 42, "held key keeps its original keycode");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn ascii_case_aliases_one_physical_key() {
        let calls = Rc::new(Cell::new(0));
        let mut cache = CharacterKeycodeCache::new(fake_resolver(42, calls.clone()));
        let press = cache.code_for(Key::Character('a')).unwrap();
        cache.commit_press(Key::Character('a'), press);
        // Caller releases via the uppercase alias (e.g. chord with Shift):
        // normalized to the same physical-key entry.
        let release = cache.code_for(Key::Character('A')).unwrap();
        assert_eq!(release, 42);
        assert_eq!(calls.get(), 1, "uppercase alias hits the same entry");
        cache.commit_release(Key::Character('A'));
        assert!(cache.entries.is_empty(), "alias release removes the entry");
    }

    #[test]
    fn resolver_failure_leaves_cache_untouched() {
        let mut cache = CharacterKeycodeCache::new(Box::new(|_| {
            Err(InputError::new("invalid_key", "layout unavailable"))
        }));
        let err = cache.code_for(Key::Character('a')).unwrap_err();
        assert_eq!(err.code, "invalid_key");
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn named_keys_resolve_statically_without_cache() {
        let calls = Rc::new(Cell::new(0));
        let mut cache = CharacterKeycodeCache::new(fake_resolver(1, calls.clone()));
        assert_eq!(cache.code_for(Key::Return).unwrap(), vk::RETURN);
        assert_eq!(calls.get(), 0, "named keys never consult the resolver");
        cache.commit_press(Key::Return, vk::RETURN);
        cache.commit_release(Key::Return);
        assert!(cache.entries.is_empty(), "named keys are not cached");
    }
}
