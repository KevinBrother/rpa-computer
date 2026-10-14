//! Pure dispatch tests: no desktop/clipboard access, including on Windows.
use super::BackendCore;
use crate::backend::dispatch::{Axis, NativeInput};
use crate::backend::{BackendError, Direction, InputEvent};
use rpa_native_input::{Button, Key};

#[derive(Debug, PartialEq)]
enum Call {
    Prepare(String),
    Verify,
    Key(Key, Direction),
}

#[derive(Default)]
struct Recording {
    calls: Vec<Call>,
    prepare_error: bool,
    verify_error: bool,
    fail_key_call: Option<usize>,
    key_calls: usize,
}

impl NativeInput for Recording {
    fn move_mouse(&mut self, _: i32, _: i32) -> Result<(), BackendError> {
        panic!("unexpected mouse")
    }
    fn scroll(&mut self, _: i32, _: Axis) -> Result<(), BackendError> {
        panic!("unexpected scroll")
    }
    fn button(&mut self, _: Button, _: Direction, _: u8) -> Result<(), BackendError> {
        Ok(())
    }
    fn key(&mut self, key: Key, direction: Direction) -> Result<(), BackendError> {
        self.calls.push(Call::Key(key, direction));
        self.key_calls += 1;
        if self.fail_key_call == Some(self.key_calls) {
            Err(BackendError::new("input_failed", "partial key dispatch"))
        } else {
            Ok(())
        }
    }
    fn text_scalar(&mut self, _: char) -> Result<(), BackendError> {
        panic!("paste must never use native scalar fallback")
    }
    fn prepare_text_paste(&mut self, text: &str) -> Result<(), BackendError> {
        self.calls.push(Call::Prepare(text.into()));
        if self.prepare_error {
            Err(BackendError::new(
                "input_failed",
                "preflight rejected; clipboard unchanged",
            ))
        } else {
            Ok(())
        }
    }
    fn verify_text_paste(&mut self) -> Result<(), BackendError> {
        self.calls.push(Call::Verify);
        if self.verify_error {
            Err(BackendError::new(
                "input_failed",
                "guard rejected; clipboard already modified",
            ))
        } else {
            Ok(())
        }
    }
}

fn paste(text: &str) -> InputEvent {
    InputEvent::TextPaste { text: text.into() }
}
fn chord() -> Vec<Call> {
    vec![
        Call::Key(Key::Control, Direction::Press),
        Call::Key(Key::Character('v'), Direction::Press),
        Call::Key(Key::Character('v'), Direction::Release),
        Call::Key(Key::Control, Direction::Release),
    ]
}

#[test]
fn whole_paste_uses_only_prepare_guard_and_tracked_chord() {
    let mut core = BackendCore::new(Recording::default());
    let text = "中文!🙂\tline\nsecond\r\nthird";
    core.inject_event(&paste(text)).unwrap();
    let mut expected = vec![Call::Prepare(text.into()), Call::Verify];
    expected.extend(chord());
    assert_eq!(core.input_mut().calls, expected);
    assert!(core.held_snapshot().is_empty());
}

#[test]
fn invalid_whole_payload_never_prepares_or_dispatches() {
    for text in [
        "prefix\0suffix".into(),
        "prefix\u{1}".into(),
        "prefix\u{7f}".into(),
        "🙂".repeat(4097),
    ] {
        let mut core = BackendCore::new(Recording::default());
        assert_eq!(
            core.inject_event(&paste(&text)).unwrap_err().code,
            "invalid_text"
        );
        assert!(core.input_mut().calls.is_empty());
    }
}

#[test]
fn scalar_limit_is_not_utf16_or_byte_limit() {
    let mut core = BackendCore::new(Recording::default());
    core.inject_event(&paste(&"🙂".repeat(4096))).unwrap();
    assert_eq!(core.input_mut().calls.len(), 6);
}

#[test]
fn empty_paste_is_noop_even_with_held_input() {
    let mut core = BackendCore::new(Recording::default());
    core.inject_event(&InputEvent::Key {
        key: "shift".into(),
        direction: Direction::Press,
    })
    .unwrap();
    core.input_mut().calls.clear();
    core.inject_event(&paste("")).unwrap();
    assert!(core.input_mut().calls.is_empty());
    assert_eq!(core.held_snapshot().len(), 1);
}

#[test]
fn held_key_or_button_rejects_before_clipboard_effects() {
    for event in [
        InputEvent::Key {
            key: "a".into(),
            direction: Direction::Press,
        },
        InputEvent::Button {
            button: "left".into(),
            direction: Direction::Press,
            click_count: 1,
        },
    ] {
        let mut core = BackendCore::new(Recording::default());
        core.inject_event(&event).unwrap();
        core.input_mut().calls.clear();
        let error = core.inject_event(&paste("payload")).unwrap_err();
        assert!(error.message.contains("held"));
        assert!(core.input_mut().calls.is_empty());
        assert_eq!(core.held_snapshot().len(), 1);
    }
}

#[test]
fn preparation_and_guard_errors_never_emit_a_chord() {
    for verify_error in [false, true] {
        let mut core = BackendCore::new(Recording {
            prepare_error: !verify_error,
            verify_error,
            ..Recording::default()
        });
        let error = core.inject_event(&paste("payload")).unwrap_err();
        assert!(error.message.contains("clipboard"));
        assert_eq!(
            core.input_mut().calls.len(),
            if verify_error { 2 } else { 1 }
        );
        assert!(core.held_snapshot().is_empty());
    }
}

#[test]
fn every_partial_chord_failure_keeps_attempted_keys_for_release_all() {
    for fail in 1..=4 {
        let mut core = BackendCore::new(Recording {
            fail_key_call: Some(fail),
            ..Recording::default()
        });
        let error = core.inject_event(&paste("payload")).unwrap_err();
        assert!(error.message.contains("clipboard already modified"));
        assert_eq!(
            core.held_snapshot().len(),
            if fail == 2 || fail == 3 { 2 } else { 1 }
        );
        let calls = &core.input_mut().calls;
        assert_eq!(&calls[2..], &chord()[..fail]);
        core.input_mut().calls.clear();
        core.release_all().unwrap();
        let released = &core.input_mut().calls;
        assert!(released
            .iter()
            .all(|call| matches!(call, Call::Key(_, Direction::Release))));
        assert_eq!(released.len(), if fail == 2 || fail == 3 { 2 } else { 1 });
        assert!(core.held_snapshot().is_empty());
    }
}

struct Unsupported;
impl NativeInput for Unsupported {
    fn move_mouse(&mut self, _: i32, _: i32) -> Result<(), BackendError> {
        panic!("unexpected effect")
    }
    fn scroll(&mut self, _: i32, _: Axis) -> Result<(), BackendError> {
        panic!("unexpected effect")
    }
    fn button(&mut self, _: Button, _: Direction, _: u8) -> Result<(), BackendError> {
        panic!("unexpected effect")
    }
    fn key(&mut self, _: Key, _: Direction) -> Result<(), BackendError> {
        panic!("unexpected effect")
    }
    fn text_scalar(&mut self, _: char) -> Result<(), BackendError> {
        panic!("unexpected fallback")
    }
}

#[test]
fn default_paste_guards_are_unsupported_without_effects() {
    assert_eq!(
        Unsupported.prepare_text_paste("text").unwrap_err().code,
        "unsupported_platform"
    );
    assert_eq!(
        Unsupported.verify_text_paste().unwrap_err().code,
        "unsupported_platform"
    );
    let mut core = BackendCore::new(Unsupported);
    core.inject_event(&paste("")).unwrap();
    assert_eq!(
        core.inject_event(&paste("text")).unwrap_err().code,
        "unsupported_platform"
    );
    assert!(core.held_snapshot().is_empty());
}
