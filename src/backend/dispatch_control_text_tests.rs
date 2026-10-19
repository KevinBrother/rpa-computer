//! Exercise the actual BackendCore -> DriverInput -> raw Driver chain.
//! RED does not depend on adding NativeInput's capability method first.
use super::*;
use crate::backend::dispatch::DriverInput;
use rpa_native_input::{
    Axis as RawAxis, Button, Direction as RawDirection, Driver, InputError, Result,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct State {
    keys: Vec<(Key, RawDirection)>,
    scalars: Vec<char>,
    scalar_error: Option<&'static str>,
}
struct RecordingDriver {
    state: Rc<RefCell<State>>,
    scalar_controls: bool,
}
impl Driver for RecordingDriver {
    fn control_text_uses_scalar(&self) -> bool {
        self.scalar_controls
    }
    fn move_mouse(&mut self, _: i32, _: i32) -> Result<()> {
        Ok(())
    }
    fn button(&mut self, _: Button, _: RawDirection, _: u8) -> Result<()> {
        Ok(())
    }
    fn scroll(&mut self, _: i32, _: RawAxis) -> Result<()> {
        Ok(())
    }
    fn key(&mut self, key: Key, direction: RawDirection) -> Result<()> {
        self.state.borrow_mut().keys.push((key, direction));
        Ok(())
    }
    fn text_scalar(&mut self, ch: char) -> Result<()> {
        let mut state = self.state.borrow_mut();
        state.scalars.push(ch);
        match state.scalar_error {
            Some(message) => Err(InputError::new("input_failed", message)),
            None => Ok(()),
        }
    }
}
fn fixture(scalar_controls: bool) -> (BackendCore<DriverInput>, Rc<RefCell<State>>) {
    let state = Rc::new(RefCell::new(State::default()));
    let core = BackendCore::new(DriverInput::new(Box::new(RecordingDriver {
        state: state.clone(),
        scalar_controls,
    })));
    (core, state)
}
fn text(core: &mut BackendCore<DriverInput>, value: &str) -> std::result::Result<(), BackendError> {
    core.inject_event(&InputEvent::Text { text: value.into() })
}

#[test]
fn held_return_or_tab_is_not_consumed_by_host_text_on_either_capability() {
    for scalar_controls in [false, true] {
        for (value, key, name) in [
            ("\r", Key::Return, "return"),
            ("\n", Key::Return, "return"),
            ("\t", Key::Tab, "tab"),
        ] {
            let (mut core, state) = fixture(scalar_controls);
            core.inject_event(&InputEvent::Key {
                key: name.into(),
                direction: Direction::Press,
            })
            .unwrap();
            let held = core.held_snapshot();
            state.borrow_mut().keys.clear();
            assert!(
                text(&mut core, value).is_err(),
                "held {key:?} must reject control text"
            );
            assert_eq!(core.held_snapshot(), held);
            assert!(state.borrow().keys.is_empty());
            assert!(state.borrow().scalars.is_empty());
            core.release_all().unwrap();
            assert_eq!(state.borrow().keys, [(key, RawDirection::Release)]);
        }
    }
}

#[test]
fn opted_in_controls_reach_raw_scalar_with_crlf_normalized_once() {
    let (mut core, state) = fixture(true);
    text(&mut core, "\r\n\r\n\t").unwrap();
    // Backend normalization preserves CR for each CRLF; no pair may double count.
    assert_eq!(state.borrow().scalars, ['\r', '\r', '\t']);
    assert!(state.borrow().keys.is_empty());
    assert!(core.held_snapshot().is_empty());
}

#[test]
fn raw_scalar_safety_rejections_escape_host_without_key_fallback() {
    for reason in [
        "nonneutral modifier state",
        "nonbase group",
        "pending native release",
    ] {
        for value in ["\r", "\n", "\t"] {
            let (mut core, state) = fixture(true);
            state.borrow_mut().scalar_error = Some(reason);
            let result = text(&mut core, value);
            assert!(result.is_err(), "must not bypass scalar safety: {reason}");
            assert!(result.unwrap_err().message.contains(reason));
            assert_eq!(state.borrow().scalars.len(), 1);
            assert!(state.borrow().keys.is_empty());
        }
    }
}

#[test]
fn nonopted_in_platforms_keep_explicit_clicks_and_crlf_normalization() {
    let (mut core, state) = fixture(false);
    text(&mut core, "\r\n\t").unwrap();
    assert_eq!(
        state.borrow().keys,
        [
            (Key::Return, RawDirection::Press),
            (Key::Return, RawDirection::Release),
            (Key::Tab, RawDirection::Press),
            (Key::Tab, RawDirection::Release),
        ]
    );
    assert!(state.borrow().scalars.is_empty());
    assert!(core.held_snapshot().is_empty());
}
