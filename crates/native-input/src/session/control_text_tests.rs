//! Real Input wrapper regressions, with only the raw OS side replaced.
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct State {
    keys: Vec<(Key, Direction)>,
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
    fn button(&mut self, _: Button, _: Direction, _: u8) -> Result<()> {
        Ok(())
    }
    fn scroll(&mut self, _: i32, _: Axis) -> Result<()> {
        Ok(())
    }
    fn key(&mut self, key: Key, direction: Direction) -> Result<()> {
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
fn fixture(scalar_controls: bool) -> (Input, Rc<RefCell<State>>) {
    let state = Rc::new(RefCell::new(State::default()));
    let input = Input::from_driver(Box::new(RecordingDriver {
        state: state.clone(),
        scalar_controls,
    }))
    .unwrap();
    (input, state)
}

#[test]
fn held_return_or_tab_is_not_consumed_by_text_on_either_capability() {
    for scalar_controls in [false, true] {
        for (ch, key) in [('\r', Key::Return), ('\n', Key::Return), ('\t', Key::Tab)] {
            let (mut input, state) = fixture(scalar_controls);
            input.key(key, Direction::Press).unwrap();
            state.borrow_mut().keys.clear();
            let result = input.text_scalar(ch);
            assert!(result.is_err(), "held {key:?} must reject {ch:?}");
            assert!(
                input.held_keys.contains(&key),
                "prior hold must remain owned"
            );
            assert!(state.borrow().keys.is_empty());
            assert!(state.borrow().scalars.is_empty());
            input.release_all().unwrap();
            assert_eq!(state.borrow().keys, [(key, Direction::Release)]);
        }
    }
}

#[test]
fn opted_in_controls_reach_scalar_path_without_explicit_key_fallback() {
    let (mut input, state) = fixture(true);
    for ch in ['\r', '\n', '\t'] {
        input.text_scalar(ch).unwrap();
    }
    assert_eq!(state.borrow().scalars, ['\r', '\n', '\t']);
    assert!(state.borrow().keys.is_empty());
    assert!(input.held_keys.is_empty());
}

#[test]
fn scalar_safety_rejections_propagate_without_clicking_controls() {
    for reason in [
        "nonneutral modifier state",
        "nonbase group",
        "pending native release",
    ] {
        for ch in ['\r', '\n', '\t'] {
            let (mut input, state) = fixture(true);
            state.borrow_mut().scalar_error = Some(reason);
            let result = input.text_scalar(ch);
            assert!(result.is_err(), "must not bypass scalar safety: {reason}");
            assert_eq!(result.unwrap_err().message, reason);
            assert_eq!(state.borrow().scalars, [ch]);
            assert!(state.borrow().keys.is_empty());
        }
    }
}

#[test]
fn nonopted_in_platforms_keep_the_existing_control_click_semantics() {
    let (mut input, state) = fixture(false);
    for ch in ['\r', '\n', '\t'] {
        input.text_scalar(ch).unwrap();
    }
    assert_eq!(
        state.borrow().keys,
        [
            (Key::Return, Direction::Press),
            (Key::Return, Direction::Release),
            (Key::Return, Direction::Press),
            (Key::Return, Direction::Release),
            (Key::Tab, Direction::Press),
            (Key::Tab, Direction::Release),
        ]
    );
    assert!(state.borrow().scalars.is_empty());
    assert!(input.held_keys.is_empty());
}
