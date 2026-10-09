//! Pure keysym mapping logic for X11 input — no FFI, no I/O.
//!
//! Everything here must be testable on any host platform.

use crate::{Axis, Button, Key};
use std::collections::HashSet;

/// Xlib's `KeySym` is `unsigned long` on all supported ABIs; mirroring that
/// (instead of a hard-coded `u64`) keeps the FFI and constants C-compatible.
pub type KeySym = std::os::raw::c_ulong;
pub type KeyCode = u8;

// Latin-1 / ASCII keysyms equal the codepoint directly for 0x20..=0xFF
// (with 0x80..=0x9F undefined). Unicode keysyms for larger codepoints
// use the 0x01000000 plane per keysymdef.h.

pub const XK_BACKSPACE: KeySym = 0xFF08;
pub const XK_TAB: KeySym = 0xFF09;
pub const XK_RETURN: KeySym = 0xFF0D;
pub const XK_ESCAPE: KeySym = 0xFF1B;
pub const XK_HOME: KeySym = 0xFF50;
pub const XK_LEFT: KeySym = 0xFF51;
pub const XK_UP: KeySym = 0xFF52;
pub const XK_RIGHT: KeySym = 0xFF53;
pub const XK_DOWN: KeySym = 0xFF54;
pub const XK_PAGE_UP: KeySym = 0xFF55;
pub const XK_PAGE_DOWN: KeySym = 0xFF56;
pub const XK_END: KeySym = 0xFF57;
pub const XK_SHIFT_L: KeySym = 0xFFE1;
pub const XK_CONTROL_L: KeySym = 0xFFE3;
pub const XK_ALT_L: KeySym = 0xFFE9;
/// Cross-platform Meta intent: the Linux Win/Super key maps XK_SUPER_L
/// (0xFFEB), so `Key::Meta` targets Super. The legacy Meta keysym (0xFFE7)
/// is bound on few real layouts and is deliberately not used.
pub const XK_SUPER_L: KeySym = 0xFFEB;
pub const XK_DELETE: KeySym = 0xFFFF;
pub const XK_SPACE: KeySym = 0x020;
pub const XK_F1: KeySym = 0xFFBE; // F1..F12 contiguous: 0xFFBE..=0xFFC9

/// Maps a contract `Key` to its X11 keysym. Letters are normalized to
/// lowercase so press/release stays on one physical keycode; uppercase
/// shift state is the caller's job (chord semantics).
pub fn key_to_keysym(key: &Key) -> Option<KeySym> {
    let ks = match key {
        Key::Character(c) => character_to_keysym(*c)?,
        Key::Control => XK_CONTROL_L,
        Key::Shift => XK_SHIFT_L,
        Key::Alt => XK_ALT_L,
        Key::Meta => XK_SUPER_L,
        Key::Return => XK_RETURN,
        Key::Tab => XK_TAB,
        Key::Space => XK_SPACE,
        Key::Backspace => XK_BACKSPACE,
        Key::Delete => XK_DELETE,
        Key::Escape => XK_ESCAPE,
        Key::UpArrow => XK_UP,
        Key::DownArrow => XK_DOWN,
        Key::LeftArrow => XK_LEFT,
        Key::RightArrow => XK_RIGHT,
        Key::Home => XK_HOME,
        Key::End => XK_END,
        Key::PageUp => XK_PAGE_UP,
        Key::PageDown => XK_PAGE_DOWN,
        Key::F1 => XK_F1,
        Key::F2 => XK_F1 + 1,
        Key::F3 => XK_F1 + 2,
        Key::F4 => XK_F1 + 3,
        Key::F5 => XK_F1 + 4,
        Key::F6 => XK_F1 + 5,
        Key::F7 => XK_F1 + 6,
        Key::F8 => XK_F1 + 7,
        Key::F9 => XK_F1 + 8,
        Key::F10 => XK_F1 + 9,
        Key::F11 => XK_F1 + 10,
        Key::F12 => XK_F1 + 11,
    };
    Some(ks)
}

/// `Key::Character` is a physical/layout key for chords, NOT Unicode text
/// injection. Only ASCII letters and digits are accepted; everything else
/// must be rejected before any effect.
pub fn character_to_keysym(c: char) -> Option<KeySym> {
    match c {
        'a'..='z' => Some(c as u32 as KeySym),
        'A'..='Z' => Some((c as u32 + 32) as KeySym),
        '0'..='9' => Some(c as u32 as KeySym),
        _ => None,
    }
}

/// Maps a Unicode scalar value to the X11 keysym encoding it, if one exists.
///
/// - U+0020..=U+007E and U+00A0..=U+00FF map to the codepoint itself.
/// - Codepoints >= 0x100 map to 0x01000000 + codepoint (keysymdef.h Unicode
///   encoding), which covers up to 0x1FFFFF.
/// - NUL, control characters, and the undefined Latin-1 gap (0x80..=0x9F)
///   have no keysym.
pub fn unicode_to_keysym(c: char) -> Option<KeySym> {
    let cp = c as u32;
    if cp == 0 {
        return None;
    }
    if (0x20..=0x7E).contains(&cp) || (0xA0..=0xFF).contains(&cp) {
        return Some(cp as KeySym);
    }
    if (0x100..=0x1F_FFFF).contains(&cp) {
        return Some(0x0100_0000 + cp as KeySym);
    }
    None
}

/// X11 wheel button numbers. Positive scroll length means RIGHT (horizontal)
/// or DOWN (vertical), matching the shared contract.
pub fn wheel_button(axis: Axis, positive: bool) -> u32 {
    match (axis, positive) {
        (Axis::Vertical, true) => 5,    // wheel down
        (Axis::Vertical, false) => 4,   // wheel up
        (Axis::Horizontal, true) => 7,  // wheel right
        (Axis::Horizontal, false) => 6, // wheel left
    }
}

/// Pointer buttons per X11 convention: 1=Left, 2=Middle, 3=Right.
pub fn pointer_button(button: Button) -> u32 {
    match button {
        Button::Left => 1,
        Button::Middle => 2,
        Button::Right => 3,
    }
}

/// Scans the current keymap for the keycode whose UNSHIFTED binding (column
/// 0, i.e. base level of the first keyboard group) is `keysym`, excluding
/// modifier-bound keycodes. Keycodes bound to `keysym` only at a shifted
/// level or in another group are NOT returned: clicking such a keycode
/// unshifted would produce a different character, so the caller must reject.
///
/// `keymap` is the flat array returned by XGetKeyboardMapping for the whole
/// keycode range; `per_code` is the keysyms-per-keycode width;
/// `first_keycode` is the keycode keymap[0] belongs to;
/// `modifier_keycodes` is the set from XGetModifierMapping.
pub fn find_unshifted_keycode(
    keymap: &[KeySym],
    per_code: usize,
    first_keycode: KeyCode,
    modifier_keycodes: &HashSet<KeyCode>,
    keysym: KeySym,
) -> Option<KeyCode> {
    if per_code == 0 || !keymap.len().is_multiple_of(per_code) {
        return None;
    }
    let rows = keymap.len() / per_code;
    for row in 0..rows {
        let keycode = first_keycode as usize + row;
        if keycode > u8::MAX as usize {
            break;
        }
        let keycode = keycode as KeyCode;
        if modifier_keycodes.contains(&keycode) {
            continue;
        }
        if keymap[row * per_code] == keysym {
            return Some(keycode);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_map_to_defined_keysyms() {
        assert_eq!(key_to_keysym(&Key::Return), Some(0xFF0D));
        assert_eq!(key_to_keysym(&Key::Tab), Some(0xFF09));
        assert_eq!(key_to_keysym(&Key::Space), Some(0x020));
        assert_eq!(key_to_keysym(&Key::Backspace), Some(0xFF08));
        assert_eq!(key_to_keysym(&Key::Delete), Some(0xFFFF));
        assert_eq!(key_to_keysym(&Key::Escape), Some(0xFF1B));
        assert_eq!(key_to_keysym(&Key::UpArrow), Some(0xFF52));
        assert_eq!(key_to_keysym(&Key::DownArrow), Some(0xFF54));
        assert_eq!(key_to_keysym(&Key::LeftArrow), Some(0xFF51));
        assert_eq!(key_to_keysym(&Key::RightArrow), Some(0xFF53));
        assert_eq!(key_to_keysym(&Key::Home), Some(0xFF50));
        assert_eq!(key_to_keysym(&Key::End), Some(0xFF57));
        assert_eq!(key_to_keysym(&Key::PageUp), Some(0xFF55));
        assert_eq!(key_to_keysym(&Key::PageDown), Some(0xFF56));
        assert_eq!(key_to_keysym(&Key::Control), Some(0xFFE3));
        assert_eq!(key_to_keysym(&Key::Shift), Some(0xFFE1));
        assert_eq!(key_to_keysym(&Key::Alt), Some(0xFFE9));
        assert_eq!(key_to_keysym(&Key::Meta), Some(0xFFEB));
    }

    #[test]
    fn function_keys_are_contiguous() {
        assert_eq!(key_to_keysym(&Key::F1), Some(0xFFBE));
        assert_eq!(key_to_keysym(&Key::F12), Some(0xFFC9));
    }

    #[test]
    fn character_accepts_only_ascii_alnum() {
        assert_eq!(character_to_keysym('a'), Some(0x061));
        assert_eq!(character_to_keysym('z'), Some(0x07A));
        assert_eq!(character_to_keysym('A'), Some(0x061)); // normalized lowercase keysym
        assert_eq!(character_to_keysym('0'), Some(0x030));
        assert_eq!(character_to_keysym('9'), Some(0x039));
        assert_eq!(character_to_keysym('!'), None);
        assert_eq!(character_to_keysym(' '), None); // Space is a named key
        assert_eq!(character_to_keysym('\t'), None);
        assert_eq!(character_to_keysym('\n'), None);
        assert_eq!(character_to_keysym('é'), None);
        assert_eq!(character_to_keysym('中'), None);
        assert_eq!(character_to_keysym('\u{1F600}'), None);
    }

    #[test]
    fn unicode_scalars_encode_to_keysyms() {
        assert_eq!(unicode_to_keysym('A'), Some(0x041));
        assert_eq!(unicode_to_keysym('~'), Some(0x07E));
        assert_eq!(unicode_to_keysym('é'), Some(0xE9)); // Latin-1 direct
        assert_eq!(unicode_to_keysym('ÿ'), Some(0xFF));
        assert_eq!(unicode_to_keysym('Ā'), Some(0x0100_0100));
        assert_eq!(unicode_to_keysym('中'), Some(0x0100_4E2D));
        assert_eq!(unicode_to_keysym('\u{10FFFF}'), Some(0x0110_FFFF));
        assert_eq!(unicode_to_keysym('\u{0}'), None);
        assert_eq!(unicode_to_keysym('\u{1}'), None);
        assert_eq!(unicode_to_keysym('\u{80}'), None); // undefined gap
        assert_eq!(unicode_to_keysym('\u{9F}'), None);
    }

    #[test]
    fn wheel_buttons_match_x11_convention() {
        assert_eq!(wheel_button(Axis::Vertical, true), 5);
        assert_eq!(wheel_button(Axis::Vertical, false), 4);
        assert_eq!(wheel_button(Axis::Horizontal, true), 7);
        assert_eq!(wheel_button(Axis::Horizontal, false), 6);
    }

    #[test]
    fn pointer_buttons_match_x11_convention() {
        assert_eq!(pointer_button(Button::Left), 1);
        assert_eq!(pointer_button(Button::Middle), 2);
        assert_eq!(pointer_button(Button::Right), 3);
    }

    fn sample_keymap(per_code: usize, rows: usize) -> Vec<KeySym> {
        let mut map = vec![0 as KeySym; per_code * rows];
        // Standard-ish layout: keycode 38 (row 30) = 'a' in col 0, shift 'A' col 1.
        if rows > 30 {
            map[30 * per_code] = 0x061;
            if per_code > 1 {
                map[30 * per_code + 1] = 0x041;
            }
        }
        map
    }

    #[test]
    fn unshifted_lookup_matches_column_zero_only() {
        let per_code = 2;
        let map = sample_keymap(per_code, 40);
        let mods: HashSet<KeyCode> = HashSet::new();
        assert_eq!(
            find_unshifted_keycode(&map, per_code, 8, &mods, 0x061),
            Some(38)
        );
        // 'A' (0x041) is bound only at the SHIFTED level of keycode 38 and
        // must NOT be resolved: clicking keycode 38 unshifted yields 'a'.
        assert_eq!(
            find_unshifted_keycode(&map, per_code, 8, &mods, 0x041),
            None,
            "shifted-level bindings are never used for unshifted injection"
        );
        assert_eq!(
            find_unshifted_keycode(&map, per_code, 8, &mods, 0x05A),
            None,
            "unmapped keysym not found"
        );
        // Modifier-bound keycode is excluded from the search.
        let mut mods2: HashSet<KeyCode> = HashSet::new();
        mods2.insert(38);
        assert_eq!(
            find_unshifted_keycode(&map, per_code, 8, &mods2, 0x061),
            None
        );
    }

    #[test]
    fn unshifted_lookup_rejects_bad_geometry() {
        let map = vec![0u64; 5];
        let mods: HashSet<KeyCode> = HashSet::new();
        assert_eq!(find_unshifted_keycode(&map, 0, 8, &mods, 1), None);
        assert_eq!(find_unshifted_keycode(&map, 2, 8, &mods, 1), None);
    }
}
