//! Shared value types, the [`Driver`] injection seam and the error contract.
//!
//! These types are the normative cross-platform interface from
//! `.agents/tasks/native-input-contract-20260929.md`. All enums are
//! `Copy, Debug, Eq, PartialEq, Hash`. No `Send`/`Sync` promises are made:
//! native input devices are thread-bound and must be created and used on the
//! same worker thread.

use std::fmt;

/// A key event target. `Character` is a PHYSICAL/layout key for chords and
/// must be a single ASCII letter or digit — it is never Unicode text
/// injection; use `Driver::text_scalar` for text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Character(char),
    Control,
    Shift,
    Alt,
    Meta,
    Return,
    Tab,
    Space,
    Backspace,
    Delete,
    Escape,
    UpArrow,
    DownArrow,
    LeftArrow,
    RightArrow,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

impl Key {
    /// True if `Character(c)` is a valid physical key (single ASCII letter or
    /// digit); named keys are always valid.
    pub fn is_valid(&self) -> bool {
        match self {
            Key::Character(c) => c.is_ascii_alphanumeric(),
            _ => true,
        }
    }

    /// Stable lowercase name for diagnostics and error messages.
    pub fn name(&self) -> String {
        match self {
            Key::Character(c) => c.to_string(),
            other => format!("{other:?}").to_lowercase(),
        }
    }
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// Scroll axis. Positive ticks scroll RIGHT (horizontal) / DOWN (vertical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Press or release direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Press,
    Release,
}

/// Structured failure. `code` is a stable machine-readable identifier drawn
/// from: `permission_denied`, `no_display`, `unsupported_session`,
/// `input_failed`, `invalid_key`, `invalid_text`.
#[derive(Debug, Clone)]
pub struct InputError {
    pub code: &'static str,
    pub message: String,
}

impl InputError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for InputError {}

pub type Result<T> = std::result::Result<T, InputError>;

/// Valid native click-state values for a button press/up pair (1..=3).
/// The count describes THIS pair's position in an OS multi-click sequence;
/// it is metadata on the event, never an instruction for a driver loop.
pub fn is_valid_click_count(click_count: u8) -> bool {
    (1..=3).contains(&click_count)
}

/// Raw native input device seam. Implementations are the per-platform
/// `Platform` types (`windows`, `macos`, `linux_x11` modules) and test mocks.
///
/// Contract semantics implementations must honor:
/// - Absolute desktop coordinates in native input units (Windows virtual-screen
///   pixels, macOS CoreGraphics points, X11 root pixels).
/// - Scroll: positive `length` = RIGHT (horizontal) / DOWN (vertical) ticks.
/// - No fake success: a driver reports real failures with structured codes and
///   never claims an event was delivered when it was not.
/// - No screenshots, no sleeps, no clipboard, no global key release inside
///   scalar injection.
pub trait Driver {
    fn move_mouse(&mut self, x: i32, y: i32) -> Result<()>;
    /// One atomic button press or release carrying the native click-state of
    /// THIS press/up pair (`click_count`, valid 1..=3, see
    /// [`is_valid_click_count`]). Invalid counts must be rejected with
    /// `invalid_button` BEFORE any native event is emitted. Platforms with a
    /// native count field (macOS) place the value in the event; platforms
    /// without one (Windows, X11) still emit the real atomic event and rely
    /// on OS time/location aggregation for multi-click recognition.
    fn button(&mut self, button: Button, direction: Direction, click_count: u8) -> Result<()>;
    fn key(&mut self, key: Key, direction: Direction) -> Result<()>;
    fn scroll(&mut self, length: i32, axis: Axis) -> Result<()>;
    /// Inject one Unicode scalar as text. `'\n'`/`'\r'` each produce ONE
    /// Return click, `'\t'` ONE Tab click; CRLF sequences are normalized by
    /// the CALLER so `'\r'` followed by `'\n'` never reaches the driver twice.
    /// NUL, other C0 controls and DEL are rejected by callers before dispatch.
    fn text_scalar(&mut self, ch: char) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_validation_accepts_named_keys_and_ascii_alphanumerics() {
        assert!(Key::Return.is_valid());
        assert!(Key::F12.is_valid());
        assert!(Key::Character('a').is_valid());
        assert!(Key::Character('Z').is_valid());
        assert!(Key::Character('7').is_valid());
    }

    #[test]
    fn key_validation_rejects_non_ascii_and_non_alphanumeric_characters() {
        assert!(!Key::Character('é').is_valid());
        assert!(!Key::Character('😀').is_valid());
        assert!(!Key::Character(' ').is_valid());
        assert!(!Key::Character('\n').is_valid());
        assert!(!Key::Character('\0').is_valid());
    }

    #[test]
    fn key_names_are_stable_and_lowercase() {
        assert_eq!(Key::Return.name(), "return");
        assert_eq!(Key::UpArrow.name(), "uparrow");
        assert_eq!(Key::Character('l').name(), "l");
    }

    #[test]
    fn error_display_and_codes() {
        let e = InputError::new("invalid_key", "bad key");
        assert_eq!(e.code, "invalid_key");
        assert_eq!(e.to_string(), "[invalid_key] bad key");
        let e = InputError::new("no_display", String::from("headless"));
        assert_eq!(e.message, "headless");
    }

    #[test]
    fn click_count_validation_bounds() {
        assert!(is_valid_click_count(1));
        assert!(is_valid_click_count(2));
        assert!(is_valid_click_count(3));
        assert!(!is_valid_click_count(0));
        assert!(!is_valid_click_count(4));
        assert!(!is_valid_click_count(255));
    }
}
