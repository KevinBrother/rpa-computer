//! Key and mouse-button name validation and mapping.
//!
//! Supported key names (case-insensitive), per `.agents/CONTRACT.md`:
//! `ctrl/control`, `shift`, `alt/option`, `meta/cmd/command/win`,
//! `enter/return`, `tab`, `space`, `backspace`, `delete`, `escape/esc`,
//! `up/down/left/right`, `home/end`, `pageup/pagedown`, `f1`-`f12`, and any
//! single ASCII alphanumeric character (mapped to a REAL key event, never
//! text injection, so chords like `ctrl+l` are genuine key presses).
//!
//! Types come from the `rpa-native-input` crate; this module is a pure
//! name→typed-key table with no dispatch of its own.

use super::BackendError;
use rpa_native_input::{Button, Key};

/// A validated key, ready for injection as a real key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParsedKey {
    /// A named (non-printable or modifier) key.
    Named(Key),
    /// A single ASCII alphanumeric character, always lowercase. Injected as
    /// `Key::Character(c)` which produces a real key press/release event via
    /// the current keyboard layout.
    Ascii(char),
}

impl ParsedKey {
    pub fn key(self) -> Key {
        match self {
            ParsedKey::Named(k) => k,
            ParsedKey::Ascii(c) => Key::Character(c),
        }
    }

    /// Canonical display name (lowercased input form) for diagnostics.
    pub fn name(self) -> String {
        match self {
            ParsedKey::Named(k) => k.name(),
            ParsedKey::Ascii(c) => c.to_string(),
        }
    }
}

/// Named keys table: (accepted aliases, native key). First alias is canonical.
const NAMED_KEYS: &[(&[&str], Key)] = &[
    (&["ctrl", "control"], Key::Control),
    (&["shift"], Key::Shift),
    (&["alt", "option"], Key::Alt),
    (&["meta", "cmd", "command", "win", "super"], Key::Meta),
    (&["enter", "return"], Key::Return),
    (&["tab"], Key::Tab),
    (&["space"], Key::Space),
    (&["backspace"], Key::Backspace),
    (&["delete"], Key::Delete),
    (&["escape", "esc"], Key::Escape),
    (&["up"], Key::UpArrow),
    (&["down"], Key::DownArrow),
    (&["left"], Key::LeftArrow),
    (&["right"], Key::RightArrow),
    (&["home"], Key::Home),
    (&["end"], Key::End),
    (&["pageup", "page_up"], Key::PageUp),
    (&["pagedown", "page_down"], Key::PageDown),
    (&["f1"], Key::F1),
    (&["f2"], Key::F2),
    (&["f3"], Key::F3),
    (&["f4"], Key::F4),
    (&["f5"], Key::F5),
    (&["f6"], Key::F6),
    (&["f7"], Key::F7),
    (&["f8"], Key::F8),
    (&["f9"], Key::F9),
    (&["f10"], Key::F10),
    (&["f11"], Key::F11),
    (&["f12"], Key::F12),
];

/// All accepted key names, for diagnostics. Lazily computed.
pub fn key_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for (aliases, _) in NAMED_KEYS {
        names.extend(aliases.iter().copied());
    }
    names
}

/// Static snapshot used by tests; kept in sync with [`key_names`].
pub const KEY_NAMES: &[&str] = &[
    "ctrl",
    "control",
    "shift",
    "alt",
    "option",
    "meta",
    "cmd",
    "command",
    "win",
    "super",
    "enter",
    "return",
    "tab",
    "space",
    "backspace",
    "delete",
    "escape",
    "esc",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pageup",
    "page_up",
    "pagedown",
    "page_down",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
];

pub const BUTTON_NAMES: &[&str] = &["left", "right", "middle"];

/// Validate and map a key name. Returns `invalid_key` on unknown names;
/// no input is emitted by callers when this fails.
pub fn parse_key(name: &str) -> Result<ParsedKey, BackendError> {
    let n = name.trim().to_ascii_lowercase();
    for (aliases, key) in NAMED_KEYS {
        if aliases.contains(&n.as_str()) {
            return Ok(ParsedKey::Named(*key));
        }
    }
    let mut chars = n.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphanumeric() {
            return Ok(ParsedKey::Ascii(c));
        }
    }
    Err(BackendError::new(
        "invalid_key",
        format!("unknown key name: {name:?}; supported: modifiers/navigation/f1-f12 or one ASCII alphanumeric character"),
    ))
}

/// Validate and map a mouse button name.
pub fn parse_button(name: &str) -> Result<Button, BackendError> {
    match name.trim().to_ascii_lowercase().as_str() {
        "left" => Ok(Button::Left),
        "right" => Ok(Button::Right),
        "middle" => Ok(Button::Middle),
        _ => Err(BackendError::new(
            "invalid_button",
            format!("unknown button name: {name:?}; supported: left/right/middle"),
        )),
    }
}

/// Validate a text payload BEFORE any of it may be injected. Shared C0
/// policy (used by action preflight AND the backend injection boundary):
/// NUL, every other C0 control (U+0000–U+001F) and DEL (U+007F) are
/// rejected EXCEPT TAB (`\t`), LF (`\n`) and CR (`\r`), which the text path
/// maps to real Tab/Return clicks. Other control keys must go through key
/// actions; text is never clipboard, so there is no fallback path for
/// unmapped characters.
pub fn validate_text(text: &str) -> Result<(), BackendError> {
    for ch in text.chars() {
        let c = ch as u32;
        if (c < 0x20 && !matches!(ch, '\t' | '\n' | '\r')) || c == 0x7F {
            return Err(BackendError::new(
                "invalid_text",
                format!(
                    "text contains control character U+{c:04X}; only TAB/LF/CR are \
                     accepted in text (use key actions for other control keys)"
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_all_contract_modifier_aliases() {
        for name in [
            "ctrl", "control", "shift", "alt", "option", "meta", "cmd", "command", "win",
        ] {
            assert!(parse_key(name).is_ok(), "should accept {name}");
        }
        assert_eq!(parse_key("ctrl").unwrap().key(), Key::Control);
        assert_eq!(parse_key("OPTION").unwrap().key(), Key::Alt);
    }

    #[test]
    fn accepts_named_navigation_and_function_keys() {
        for name in [
            "enter",
            "return",
            "tab",
            "space",
            "backspace",
            "delete",
            "escape",
            "esc",
            "up",
            "down",
            "left",
            "right",
            "home",
            "end",
            "pageup",
            "pagedown",
            "f1",
            "f2",
            "f3",
            "f4",
            "f5",
            "f6",
            "f7",
            "f8",
            "f9",
            "f10",
            "f11",
            "f12",
        ] {
            assert!(parse_key(name).is_ok(), "should accept {name}");
        }
        assert_eq!(parse_key("esc").unwrap().key(), Key::Escape);
        assert_eq!(parse_key("F12").unwrap().key(), Key::F12);
    }

    #[test]
    fn accepts_single_ascii_alphanumeric_as_real_key() {
        match parse_key("l").unwrap() {
            ParsedKey::Ascii(c) => assert_eq!(c, 'l'),
            other => panic!("expected Ascii, got {other:?}"),
        }
        // Uppercase letters fold to lowercase: shift is an explicit modifier.
        match parse_key("L").unwrap() {
            ParsedKey::Ascii(c) => assert_eq!(c, 'l'),
            other => panic!("expected Ascii, got {other:?}"),
        }
        assert!(matches!(parse_key("7").unwrap(), ParsedKey::Ascii('7')));
    }

    #[test]
    fn rejects_unknown_keys_structured() {
        for name in [
            "", "ctrl+l", "f13", "page", "fn", "é", "ab", "😀", "volumeup",
        ] {
            let err = parse_key(name).unwrap_err();
            assert_eq!(err.code, "invalid_key", "for {name:?}");
        }
    }

    #[test]
    fn buttons_validate() {
        assert_eq!(parse_button("left").unwrap(), Button::Left);
        assert_eq!(parse_button("Right").unwrap(), Button::Right);
        assert_eq!(parse_button("MIDDLE").unwrap(), Button::Middle);
        let err = parse_button("x1").unwrap_err();
        assert_eq!(err.code, "invalid_button");
        let err = parse_button("").unwrap_err();
        assert_eq!(err.code, "invalid_button");
    }

    #[test]
    fn text_validation_rejects_c0_and_del_but_keeps_crlf_tab() {
        // Ordinary Unicode is fine.
        assert!(validate_text("hello 世界").is_ok());
        // Allowed controls: the three that map to real key clicks.
        assert!(validate_text("a\tb\nc\rd").is_ok());
        // NUL and every other C0 control is rejected...
        for bad in [
            "a\0b", "\u{1}", "\u{7}", "a\u{8}b", "a\u{b}b", "a\u{c}b", "a\u{1b}b", "a\u{1f}b",
        ] {
            let err = validate_text(bad).unwrap_err();
            assert_eq!(err.code, "invalid_text", "for {bad:?}");
        }
        // ...and DEL too.
        let err = validate_text("a\u{7f}b").unwrap_err();
        assert_eq!(err.code, "invalid_text");
    }

    #[test]
    fn key_names_snapshot_matches_table() {
        let mut dynamic = key_names();
        dynamic.sort_unstable();
        let mut snapshot = KEY_NAMES.to_vec();
        snapshot.sort_unstable();
        assert_eq!(dynamic, snapshot);
    }
}
