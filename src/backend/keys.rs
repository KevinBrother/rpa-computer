//! Key and mouse-button name validation and mapping.
//!
//! Supported key names (case-insensitive), per `.agents/CONTRACT.md`:
//! `ctrl/control`, `shift`, `alt/option`, `meta/cmd/command/win`,
//! `enter/return`, `tab`, `space`, `backspace`, `delete`, `escape/esc`,
//! `up/down/left/right`, `home/end`, `pageup/pagedown`, `f1`-`f12`, and any
//! single ASCII alphanumeric character (mapped to a REAL key event, never
//! text injection, so chords like `ctrl+l` are genuine key presses).

use super::{BackendError, Direction};

/// A validated key, ready for injection as a real key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParsedKey {
    /// A named (non-printable or modifier) key.
    Named(enigo::Key),
    /// A single ASCII alphanumeric character, always lowercase. Injected as
    /// `enigo::Key::Unicode(c)` which produces real key press/release events.
    Ascii(char),
}

impl ParsedKey {
    pub fn enigo_key(self) -> enigo::Key {
        match self {
            ParsedKey::Named(k) => k,
            ParsedKey::Ascii(c) => enigo::Key::Unicode(c),
        }
    }

    /// Canonical display name (lowercased input form) for diagnostics.
    pub fn name(self) -> String {
        match self {
            ParsedKey::Named(k) => format!("{k:?}"),
            ParsedKey::Ascii(c) => c.to_string(),
        }
    }
}

/// Named keys table: (accepted aliases, enigo key). First alias is canonical.
const NAMED_KEYS: &[(&[&str], enigo::Key)] = &[
    (&["ctrl", "control"], enigo::Key::Control),
    (&["shift"], enigo::Key::Shift),
    (&["alt", "option"], enigo::Key::Alt),
    (
        &["meta", "cmd", "command", "win", "super"],
        enigo::Key::Meta,
    ),
    (&["enter", "return"], enigo::Key::Return),
    (&["tab"], enigo::Key::Tab),
    (&["space"], enigo::Key::Space),
    (&["backspace"], enigo::Key::Backspace),
    (&["delete"], enigo::Key::Delete),
    (&["escape", "esc"], enigo::Key::Escape),
    (&["up"], enigo::Key::UpArrow),
    (&["down"], enigo::Key::DownArrow),
    (&["left"], enigo::Key::LeftArrow),
    (&["right"], enigo::Key::RightArrow),
    (&["home"], enigo::Key::Home),
    (&["end"], enigo::Key::End),
    (&["pageup", "page_up"], enigo::Key::PageUp),
    (&["pagedown", "page_down"], enigo::Key::PageDown),
    (&["f1"], enigo::Key::F1),
    (&["f2"], enigo::Key::F2),
    (&["f3"], enigo::Key::F3),
    (&["f4"], enigo::Key::F4),
    (&["f5"], enigo::Key::F5),
    (&["f6"], enigo::Key::F6),
    (&["f7"], enigo::Key::F7),
    (&["f8"], enigo::Key::F8),
    (&["f9"], enigo::Key::F9),
    (&["f10"], enigo::Key::F10),
    (&["f11"], enigo::Key::F11),
    (&["f12"], enigo::Key::F12),
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
pub fn parse_button(name: &str) -> Result<enigo::Button, BackendError> {
    match name.trim().to_ascii_lowercase().as_str() {
        "left" => Ok(enigo::Button::Left),
        "right" => Ok(enigo::Button::Right),
        "middle" => Ok(enigo::Button::Middle),
        _ => Err(BackendError::new(
            "invalid_button",
            format!("unknown button name: {name:?}; supported: left/right/middle"),
        )),
    }
}

/// Validate a text payload for `enigo::Keyboard::text` (rejects NUL, which
/// would truncate C-string based paths).
pub fn validate_text(text: &str) -> Result<(), BackendError> {
    if text.contains('\0') {
        return Err(BackendError::new(
            "invalid_text",
            "text contains a NUL character",
        ));
    }
    Ok(())
}

/// Map a contract [`Direction`] to an enigo direction for press/release.
/// (Never `Click`: press and release must stay individually observable so
/// held-state tracking and `release_all` stay correct.)
pub fn enigo_direction(d: Direction) -> enigo::Direction {
    match d {
        Direction::Press => enigo::Direction::Press,
        Direction::Release => enigo::Direction::Release,
    }
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
        assert_eq!(parse_key("ctrl").unwrap().enigo_key(), enigo::Key::Control);
        assert_eq!(parse_key("OPTION").unwrap().enigo_key(), enigo::Key::Alt);
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
        assert_eq!(parse_key("esc").unwrap().enigo_key(), enigo::Key::Escape);
        assert_eq!(parse_key("F12").unwrap().enigo_key(), enigo::Key::F12);
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
        assert_eq!(parse_button("left").unwrap(), enigo::Button::Left);
        assert_eq!(parse_button("Right").unwrap(), enigo::Button::Right);
        assert_eq!(parse_button("MIDDLE").unwrap(), enigo::Button::Middle);
        let err = parse_button("x1").unwrap_err();
        assert_eq!(err.code, "invalid_button");
        let err = parse_button("").unwrap_err();
        assert_eq!(err.code, "invalid_button");
    }

    #[test]
    fn text_validation_rejects_nul() {
        assert!(validate_text("hello 世界").is_ok());
        let err = validate_text("a\0b").unwrap_err();
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
