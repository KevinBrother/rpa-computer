//! Keycode resolution for macOS: Apple virtual keycodes for named keys and
//! TIS + UCKeyTranslate layout translation for `Key::Character`. Pure with
//! respect to injection: nothing here posts events.

use std::os::raw::c_ulong;
use std::sync::Mutex;

use super::ffi;
use crate::{InputError, Key, Result};

/// Process-local serialization for the ENTIRE TIS + UCKeyTranslate + CF
/// layout-query lifetime. TIS/TSM terminate the process when called from two
/// threads concurrently; production callers all funnel through
/// [`char_to_keycode`], which holds this lock across every layout query. The
/// mutex does not (and cannot) coordinate unrelated TIS users in the same
/// process; GUI embedding additionally requires main-thread TIS.
static TIS_LOCK: Mutex<()> = Mutex::new(());

// kUCKeyActionDisplay (HIToolbox UnicodeInput.h)
pub(super) const K_UC_KEY_ACTION_DISPLAY: u16 = 3;
// kUCKeyTranslateNoDeadKeysBit is bit 0; the option value is 1 << 0.
pub(super) const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS: u32 = 1;
// kUCKeyLayoutHeaderSize: the fixed header of a UCKeyboardLayout is 256 bytes
// (HIToolbox/UnicodeInput.h); any real layout data must be at least this long.
const K_UC_KEY_LAYOUT_HEADER_SIZE: c_ulong = 256;
// Keyboard layout translate buffer size.
pub(super) const UC_STRING_BUF_LEN: usize = 4;

// Apple virtual keycodes (kVK_*, HIToolbox/Events.h). The table mirrors the
// full kVK letter/digit set even where layout translation covers the lookup,
// so unused entries are expected.
#[allow(dead_code)]
pub(super) mod vk {
    pub const A: u16 = 0x00;
    pub const S: u16 = 0x01;
    pub const D: u16 = 0x02;
    pub const F: u16 = 0x03;
    pub const H: u16 = 0x04;
    pub const G: u16 = 0x05;
    pub const Z: u16 = 0x06;
    pub const X: u16 = 0x07;
    pub const C: u16 = 0x08;
    pub const V: u16 = 0x09;
    pub const B: u16 = 0x0B;
    pub const Q: u16 = 0x0C;
    pub const W: u16 = 0x0D;
    pub const E: u16 = 0x0E;
    pub const R: u16 = 0x0F;
    pub const Y: u16 = 0x10;
    pub const T: u16 = 0x11;
    pub const ONE: u16 = 0x12;
    pub const TWO: u16 = 0x13;
    pub const THREE: u16 = 0x14;
    pub const FOUR: u16 = 0x15;
    pub const SIX: u16 = 0x16;
    pub const FIVE: u16 = 0x17;
    pub const NINE: u16 = 0x19;
    pub const SEVEN: u16 = 0x1A;
    pub const EIGHT: u16 = 0x1C;
    pub const ZERO: u16 = 0x1D;
    pub const O: u16 = 0x1F;
    pub const U: u16 = 0x20;
    pub const I: u16 = 0x22;
    pub const P: u16 = 0x23;
    pub const L: u16 = 0x25;
    pub const J: u16 = 0x26;
    pub const K: u16 = 0x28;
    pub const N: u16 = 0x2D;
    pub const M: u16 = 0x2E;

    pub const RETURN: u16 = 0x24;
    pub const TAB: u16 = 0x30;
    pub const SPACE: u16 = 0x31;
    pub const DELETE: u16 = 0x33; // Backspace
    pub const ESCAPE: u16 = 0x35;
    pub const COMMAND: u16 = 0x37;
    pub const SHIFT: u16 = 0x38;
    pub const OPTION: u16 = 0x3A;
    pub const CONTROL: u16 = 0x3B;
    pub const FORWARD_DELETE: u16 = 0x75;
    pub const HOME: u16 = 0x73;
    pub const END: u16 = 0x77;
    pub const PAGE_UP: u16 = 0x74;
    pub const PAGE_DOWN: u16 = 0x79;
    pub const LEFT_ARROW: u16 = 0x7B;
    pub const RIGHT_ARROW: u16 = 0x7C;
    pub const DOWN_ARROW: u16 = 0x7D;
    pub const UP_ARROW: u16 = 0x7E;
    pub const F1: u16 = 0x7A;
    pub const F2: u16 = 0x78;
    pub const F3: u16 = 0x63;
    pub const F4: u16 = 0x76;
    pub const F5: u16 = 0x60;
    pub const F6: u16 = 0x61;
    pub const F7: u16 = 0x62;
    pub const F8: u16 = 0x64;
    pub const F9: u16 = 0x65;
    pub const F10: u16 = 0x6D;
    pub const F11: u16 = 0x67;
    pub const F12: u16 = 0x6F;
}

/// keycode for named (non-character) keys.
pub(super) fn named_key_code(key: Key) -> Option<u16> {
    Some(match key {
        Key::Control => vk::CONTROL,
        Key::Shift => vk::SHIFT,
        Key::Alt => vk::OPTION,
        Key::Meta => vk::COMMAND,
        Key::Return => vk::RETURN,
        Key::Tab => vk::TAB,
        Key::Space => vk::SPACE,
        Key::Backspace => vk::DELETE,
        Key::Delete => vk::FORWARD_DELETE,
        Key::Escape => vk::ESCAPE,
        Key::UpArrow => vk::UP_ARROW,
        Key::DownArrow => vk::DOWN_ARROW,
        Key::LeftArrow => vk::LEFT_ARROW,
        Key::RightArrow => vk::RIGHT_ARROW,
        Key::Home => vk::HOME,
        Key::End => vk::END,
        Key::PageUp => vk::PAGE_UP,
        Key::PageDown => vk::PAGE_DOWN,
        Key::F1 => vk::F1,
        Key::F2 => vk::F2,
        Key::F3 => vk::F3,
        Key::F4 => vk::F4,
        Key::F5 => vk::F5,
        Key::F6 => vk::F6,
        Key::F7 => vk::F7,
        Key::F8 => vk::F8,
        Key::F9 => vk::F9,
        Key::F10 => vk::F10,
        Key::F11 => vk::F11,
        Key::F12 => vk::F12,
        _ => return None,
    })
}

/// Contract: `Key::Character` only carries ASCII letters/digits (a
/// physical/layout key for chords). Everything else is rejected before any
/// effect.
pub(super) fn character_allowed(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
}

/// UTF-16 code units for text injection (supplementary planes yield a
/// surrogate pair). Returns the buffer and the number of used units.
pub(super) fn encode_char_units(ch: char) -> ([u16; 2], usize) {
    let mut buf = [0u16; 2];
    let len = ch.encode_utf16(&mut buf).len();
    (buf, len)
}

/// Validates a scalar for `text_scalar`. NUL, other C0 controls and DEL are
/// rejected; `\n`, `\r` and `\t` are accepted and mapped to Return/Tab clicks
/// by the driver. This mirrors the shared-caller validation and is repeated
/// here as a raw-driver backstop.
pub(super) fn validate_text_char(ch: char) -> Result<()> {
    let cp = ch as u32;
    if cp == 0x7F || cp <= 0x1F {
        if matches!(ch, '\n' | '\r' | '\t') {
            return Ok(());
        }
        return Err(InputError::new(
            "invalid_text",
            format!("control character U+{cp:04X} is not valid text input"),
        ));
    }
    Ok(())
}

/// Resolves the physical keycode that produces `ch` (unshifted) on the current
/// keyboard layout via TIS + UCKeyTranslate.
///
/// There is deliberately NO fallback to a hard-coded ANSI layout: guessing a
/// keycode from a different layout would inject the wrong physical key
/// silently. When the layout cannot be queried, or the character is not
/// produced by any keycode, an explicit error is returned.
///
/// THREADING: the whole TIS + UCKeyTranslate + CF source lifetime runs under
/// one process-local mutex. TIS/TSM abort the process when called from two
/// threads concurrently (observed as `HIToolbox` ABORT in real crash logs).
/// The mutex serializes only users of THIS library — it cannot coordinate
/// other TIS users in the same process, and GUI embedding additionally
/// requires all TIS calls to happen on the application main thread.
pub(super) fn char_to_keycode(ch: char) -> Result<u16> {
    if !character_allowed(ch) {
        return Err(InputError::new(
            "invalid_key",
            format!("character {ch:?} is not a supported ASCII letter or digit"),
        ));
    }
    let lower = ch.to_ascii_lowercase();

    let _tis = TIS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // SAFETY: TISCopyCurrentKeyboardLayoutInputSource follows the copy rule;
    // the source is released by `source`.
    let layout_source = unsafe { ffi::TISCopyCurrentKeyboardLayoutInputSource() };
    let source = ffi::CfRef::retain(layout_source).ok_or_else(|| {
        InputError::new(
            "input_failed",
            "current keyboard layout input source is unavailable; \
             character keycodes cannot be resolved without guessing a layout",
        )
    })?;

    // SAFETY: source is a live TISInputSourceRef; the property key is a
    // CoreFoundation constant string.
    let data =
        unsafe { ffi::TISGetInputSourceProperty(source.0, ffi::kTISPropertyUnicodeKeyLayoutData) };
    if data.is_null() {
        return Err(InputError::new(
            "input_failed",
            "current keyboard layout has no Unicode key layout data; \
             character keycodes cannot be resolved without guessing a layout",
        ));
    }

    // SAFETY: data is a live CFData owned by the input source (get-rule: use
    // while `source` is alive, never release). Byte pointer and length are
    // validated BEFORE any UCKeyTranslate call touches the layout bytes.
    let (ptr, data_len) = unsafe { (ffi::CFDataGetBytePtr(data), ffi::CFDataGetLength(data)) };
    if ptr.is_null() || data_len < K_UC_KEY_LAYOUT_HEADER_SIZE {
        return Err(InputError::new(
            "input_failed",
            "current keyboard layout data is empty or truncated",
        ));
    }

    // SAFETY: LMGetKbdType reads the low-memory global keyboard type.
    let keyboard_type = unsafe { ffi::LMGetKbdType() } as u32;
    for keycode in 0u16..=127 {
        let mut buf = [0u16; UC_STRING_BUF_LEN];
        let mut len: c_ulong = 0;
        let mut dead_state: u32 = 0;
        // SAFETY: ptr is a valid UCKeyboardLayout of at least
        // K_UC_KEY_LAYOUT_HEADER_SIZE bytes; all out-params are valid locals.
        let status = unsafe {
            ffi::UCKeyTranslate(
                ptr,
                keycode,
                K_UC_KEY_ACTION_DISPLAY,
                0,
                keyboard_type,
                K_UC_KEY_TRANSLATE_NO_DEAD_KEYS,
                &mut dead_state,
                UC_STRING_BUF_LEN as c_ulong,
                &mut len,
                buf.as_mut_ptr(),
            )
        };
        if status == 0 && len > 0 && buf[0] == lower as u16 {
            return Ok(keycode);
        }
    }
    Err(InputError::new(
        "invalid_key",
        format!("character {ch:?} not found in the current keyboard layout"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_code<T: std::fmt::Debug>(res: Result<T>) -> &'static str {
        res.expect_err("expected error").code
    }

    #[test]
    fn named_keycodes_match_apple_constants() {
        assert_eq!(named_key_code(Key::Return), Some(0x24));
        assert_eq!(named_key_code(Key::Tab), Some(0x30));
        assert_eq!(named_key_code(Key::Space), Some(0x31));
        assert_eq!(named_key_code(Key::Backspace), Some(0x33));
        assert_eq!(named_key_code(Key::Delete), Some(0x75));
        assert_eq!(named_key_code(Key::Escape), Some(0x35));
        assert_eq!(named_key_code(Key::UpArrow), Some(0x7E));
        assert_eq!(named_key_code(Key::DownArrow), Some(0x7D));
        assert_eq!(named_key_code(Key::LeftArrow), Some(0x7B));
        assert_eq!(named_key_code(Key::RightArrow), Some(0x7C));
        assert_eq!(named_key_code(Key::Home), Some(0x73));
        assert_eq!(named_key_code(Key::End), Some(0x77));
        assert_eq!(named_key_code(Key::PageUp), Some(0x74));
        assert_eq!(named_key_code(Key::PageDown), Some(0x79));
        assert_eq!(named_key_code(Key::F1), Some(0x7A));
        assert_eq!(named_key_code(Key::F12), Some(0x6F));
        assert_eq!(named_key_code(Key::Control), Some(0x3B));
        assert_eq!(named_key_code(Key::Shift), Some(0x38));
        assert_eq!(named_key_code(Key::Alt), Some(0x3A));
        assert_eq!(named_key_code(Key::Meta), Some(0x37));
        assert_eq!(named_key_code(Key::Character('a')), None);
    }

    #[test]
    fn every_named_key_has_a_keycode() {
        let named = [
            Key::Control,
            Key::Shift,
            Key::Alt,
            Key::Meta,
            Key::Return,
            Key::Tab,
            Key::Space,
            Key::Backspace,
            Key::Delete,
            Key::Escape,
            Key::UpArrow,
            Key::DownArrow,
            Key::LeftArrow,
            Key::RightArrow,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::F1,
            Key::F2,
            Key::F3,
            Key::F4,
            Key::F5,
            Key::F6,
            Key::F7,
            Key::F8,
            Key::F9,
            Key::F10,
            Key::F11,
            Key::F12,
        ];
        for key in named {
            assert!(named_key_code(key).is_some(), "missing mapping for {key:?}");
        }
    }

    #[test]
    fn character_keycodes_resolve_layout_aware() {
        // Pure layout translation: reads current layout, injects nothing.
        // Concurrency is safe: char_to_keycode serializes on TIS_LOCK.
        let a = char_to_keycode('a').expect("'a' must resolve");
        assert_eq!(char_to_keycode('A').expect("'A' must resolve"), a);
        let one = char_to_keycode('1').expect("'1' must resolve");
        assert_ne!(a, one);
        assert!(char_to_keycode('z').is_ok());
        assert!(char_to_keycode('0').is_ok());
    }

    #[test]
    fn character_outside_ascii_letters_digits_is_rejected() {
        assert_eq!(err_code(char_to_keycode('中')), "invalid_key");
        assert_eq!(err_code(char_to_keycode(' ')), "invalid_key");
        assert_eq!(err_code(char_to_keycode('\n')), "invalid_key");
        assert_eq!(err_code(char_to_keycode('\0')), "invalid_key");
        assert_eq!(err_code(char_to_keycode('é')), "invalid_key");
    }

    #[test]
    fn unmappable_character_fails_without_effects() {
        // All invalid inputs fail resolution before any CGEventPost could run.
        assert_eq!(err_code(char_to_keycode('/')), "invalid_key");
    }

    #[test]
    fn text_scalar_accepts_printable_and_crlf_tab_only() {
        assert_eq!(err_code(validate_text_char('\0')), "invalid_text");
        assert_eq!(err_code(validate_text_char('\u{1}')), "invalid_text");
        assert_eq!(err_code(validate_text_char('\u{1B}')), "invalid_text");
        assert_eq!(err_code(validate_text_char('\u{7F}')), "invalid_text"); // DEL
        validate_text_char('\n').unwrap();
        validate_text_char('\r').unwrap();
        validate_text_char('\t').unwrap();
        validate_text_char(' ').unwrap();
        validate_text_char('é').unwrap();
        validate_text_char('中').unwrap();
        validate_text_char('\u{1F600}').unwrap();
    }

    #[test]
    fn utf16_encoding_handles_supplementary_planes() {
        assert_eq!(encode_char_units('A'), ([0x0041, 0], 1));
        assert_eq!(encode_char_units('é'), ([0x00E9, 0], 1));
        assert_eq!(encode_char_units('中'), ([0x4E2D, 0], 1));
        // U+1F600 is a surrogate pair: D83D DE00
        assert_eq!(encode_char_units('\u{1F600}'), ([0xD83D, 0xDE00], 2));
    }
}
