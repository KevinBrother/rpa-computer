//! Portable tests of the real standalone session cleanup path.
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct State {
    calls: Vec<String>,
    fail_release: bool,
    fail_cleanup: bool,
    resource_owned: bool,
}
struct RecordingDriver(Rc<RefCell<State>>);
impl Driver for RecordingDriver {
    fn move_mouse(&mut self, _: i32, _: i32) -> Result<()> {
        Ok(())
    }
    fn scroll(&mut self, _: i32, _: Axis) -> Result<()> {
        Ok(())
    }
    fn button(&mut self, _: Button, direction: Direction, count: u8) -> Result<()> {
        self.0
            .borrow_mut()
            .calls
            .push(format!("button:{direction:?}:{count}"));
        Ok(())
    }
    fn key(&mut self, _: Key, direction: Direction) -> Result<()> {
        let mut state = self.0.borrow_mut();
        state.calls.push(format!("key:{direction:?}"));
        if direction == Direction::Release && state.fail_release {
            return Err(InputError::new("input_failed", "owned release unconfirmed"));
        }
        Ok(())
    }
    fn text_scalar(&mut self, _: char) -> Result<()> {
        let mut state = self.0.borrow_mut();
        state.calls.push("scalar".into());
        state.resource_owned = true;
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        let mut state = self.0.borrow_mut();
        state.calls.push("cleanup".into());
        if state.fail_cleanup {
            return Err(InputError::new(
                "input_failed",
                "mapping restoration unconfirmed",
            ));
        }
        state.resource_owned = false;
        Ok(())
    }
}
fn fixture() -> (Input, Rc<RefCell<State>>) {
    let state = Rc::new(RefCell::new(State::default()));
    let input = Input::from_driver(Box::new(RecordingDriver(state.clone()))).unwrap();
    (input, state)
}

#[test]
fn cleanup_empty_held_set_still_reports_driver_failure_and_retries() {
    let (mut input, state) = fixture();
    state.borrow_mut().fail_cleanup = true;
    let error = input.release_all().unwrap_err();
    assert_eq!(error.code, "input_failed");
    assert!(error.message.contains("mapping restoration unconfirmed"));
    assert!(input.held_snapshot().is_empty());
    assert_eq!(state.borrow().calls, ["cleanup"]);
    state.borrow_mut().fail_cleanup = false;
    input.release_all().unwrap();
    assert_eq!(state.borrow().calls, ["cleanup", "cleanup"]);
}

#[test]
fn cleanup_follows_all_tracked_button_and_key_release_attempts() {
    let (mut input, state) = fixture();
    input.key(Key::Shift, Direction::Press).unwrap();
    input.button(Button::Left, Direction::Press, 3).unwrap();
    state.borrow_mut().calls.clear();
    input.release_all().unwrap();
    assert_eq!(
        state.borrow().calls,
        ["button:Release:3", "key:Release", "cleanup"]
    );
}

#[test]
fn cleanup_aggregates_driver_and_release_failures_without_forgetting_key() {
    let (mut input, state) = fixture();
    input.key(Key::Shift, Direction::Press).unwrap();
    input.button(Button::Left, Direction::Press, 2).unwrap();
    {
        let mut state = state.borrow_mut();
        state.calls.clear();
        state.fail_release = true;
        state.fail_cleanup = true;
    }
    let error = input.release_all().unwrap_err();
    assert!(error.message.contains("owned release unconfirmed"));
    assert!(error.message.contains("mapping restoration unconfirmed"));
    assert_eq!(
        state.borrow().calls,
        ["button:Release:2", "key:Release", "cleanup"]
    );
    assert_eq!(input.held_snapshot().len(), 1);
    {
        let mut state = state.borrow_mut();
        state.calls.clear();
        state.fail_release = false;
        state.fail_cleanup = false;
    }
    input.release_all().unwrap();
    assert_eq!(state.borrow().calls, ["key:Release", "cleanup"]);
    assert!(input.held_snapshot().is_empty());
}

#[test]
fn cleanup_retains_native_resources_on_failure_even_without_tracked_keys() {
    let (mut input, state) = fixture();
    input.text_scalar('中').unwrap();
    state.borrow_mut().fail_cleanup = true;
    assert!(input.release_all().is_err());
    assert!(state.borrow().resource_owned);
    state.borrow_mut().fail_cleanup = false;
    input.release_all().unwrap();
    assert!(!state.borrow().resource_owned);
}

#[test]
fn cleanup_success_allows_a_fresh_session_on_the_same_driver() {
    let (mut input, state) = fixture();
    input.text_scalar('中').unwrap();
    input.release_all().unwrap();
    input.text_scalar('文').unwrap();
    input.release_all().unwrap();
    assert_eq!(
        state.borrow().calls,
        ["scalar", "cleanup", "scalar", "cleanup"]
    );
    assert!(!state.borrow().resource_owned);
}

#[test]
fn cleanup_drop_retries_failed_explicit_cleanup_after_releasing_owned_keys() {
    let (mut input, state) = fixture();
    input.key(Key::Shift, Direction::Press).unwrap();
    state.borrow_mut().fail_release = true;
    state.borrow_mut().fail_cleanup = true;
    assert!(input.release_all().is_err());
    {
        let mut state = state.borrow_mut();
        state.calls.clear();
        state.fail_release = false;
        state.fail_cleanup = false;
    }
    drop(input);
    assert_eq!(state.borrow().calls, ["key:Release", "cleanup"]);
}
