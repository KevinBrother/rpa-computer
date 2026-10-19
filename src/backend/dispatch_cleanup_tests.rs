//! Tests the real BackendCore -> DriverInput -> raw Driver cleanup chain.
//! No native platform device, clipboard or desktop is opened by these tests.
use super::{BackendCore, DriverInput};
use crate::backend::{Direction, InputEvent};
use rpa_native_input::{Axis, Button, Direction as RawDirection, Driver, InputError, Key, Result};
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
    fn button(&mut self, _: Button, direction: RawDirection, count: u8) -> Result<()> {
        self.0
            .borrow_mut()
            .calls
            .push(format!("button:{direction:?}:{count}"));
        Ok(())
    }
    fn key(&mut self, _: Key, direction: RawDirection) -> Result<()> {
        let mut state = self.0.borrow_mut();
        state.calls.push(format!("key:{direction:?}"));
        if direction == RawDirection::Release && state.fail_release {
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
fn fixture() -> (BackendCore<DriverInput>, Rc<RefCell<State>>) {
    let state = Rc::new(RefCell::new(State::default()));
    let core = BackendCore::new(DriverInput::new(Box::new(RecordingDriver(state.clone()))));
    (core, state)
}
fn press_key(core: &mut BackendCore<DriverInput>) {
    core.inject_event(&InputEvent::Key {
        key: "shift".into(),
        direction: Direction::Press,
    })
    .unwrap();
}
fn press_button(core: &mut BackendCore<DriverInput>, count: u8) {
    core.inject_event(&InputEvent::Button {
        button: "left".into(),
        direction: Direction::Press,
        click_count: count,
    })
    .unwrap();
}

#[test]
fn cleanup_empty_host_held_set_still_forwards_and_reports_native_failure() {
    let (mut core, state) = fixture();
    state.borrow_mut().fail_cleanup = true;
    let error = core.release_all().unwrap_err();
    assert_eq!(error.code, "input_failed");
    assert!(error.message.contains("mapping restoration unconfirmed"));
    assert!(core.held_snapshot().is_empty());
    assert_eq!(state.borrow().calls, ["cleanup"]);
    state.borrow_mut().fail_cleanup = false;
    core.release_all().unwrap();
    assert_eq!(state.borrow().calls, ["cleanup", "cleanup"]);
}

#[test]
fn cleanup_host_orders_button_then_key_then_raw_cleanup() {
    let (mut core, state) = fixture();
    press_key(&mut core);
    press_button(&mut core, 3);
    state.borrow_mut().calls.clear();
    core.release_all().unwrap();
    assert_eq!(
        state.borrow().calls,
        ["button:Release:3", "key:Release", "cleanup"]
    );
}

#[test]
fn cleanup_host_aggregates_release_and_mapping_failure_and_preserves_retry() {
    let (mut core, state) = fixture();
    press_key(&mut core);
    press_button(&mut core, 2);
    {
        let mut state = state.borrow_mut();
        state.calls.clear();
        state.fail_release = true;
        state.fail_cleanup = true;
    }
    let error = core.release_all().unwrap_err();
    assert!(error.message.contains("owned release unconfirmed"));
    assert!(error.message.contains("mapping restoration unconfirmed"));
    assert_eq!(
        state.borrow().calls,
        ["button:Release:2", "key:Release", "cleanup"]
    );
    assert_eq!(core.held_snapshot().len(), 1);
    {
        let mut state = state.borrow_mut();
        state.calls.clear();
        state.fail_release = false;
        state.fail_cleanup = false;
    }
    core.release_all().unwrap();
    assert_eq!(state.borrow().calls, ["key:Release", "cleanup"]);
    assert!(core.held_snapshot().is_empty());
}

#[test]
fn cleanup_native_only_resources_retry_even_with_empty_host_key_tracking() {
    let (mut core, state) = fixture();
    core.inject_event(&InputEvent::Text { text: "中".into() })
        .unwrap();
    state.borrow_mut().fail_cleanup = true;
    assert!(core.release_all().is_err());
    assert!(core.held_snapshot().is_empty());
    assert!(state.borrow().resource_owned);
    state.borrow_mut().fail_cleanup = false;
    core.release_all().unwrap();
    assert!(!state.borrow().resource_owned);
}

#[test]
fn cleanup_host_driver_remains_reusable_for_the_next_session() {
    let (mut core, state) = fixture();
    for text in ["中", "文"] {
        core.inject_event(&InputEvent::Text { text: text.into() })
            .unwrap();
        core.release_all().unwrap();
    }
    assert_eq!(
        state.borrow().calls,
        ["scalar", "cleanup", "scalar", "cleanup"]
    );
    assert!(!state.borrow().resource_owned);
}

#[test]
fn cleanup_host_drop_retries_native_cleanup_after_releasing_keys() {
    let (mut core, state) = fixture();
    press_key(&mut core);
    state.borrow_mut().fail_release = true;
    state.borrow_mut().fail_cleanup = true;
    assert!(core.release_all().is_err());
    {
        let mut state = state.borrow_mut();
        state.calls.clear();
        state.fail_release = false;
        state.fail_cleanup = false;
    }
    drop(core);
    assert_eq!(state.borrow().calls, ["key:Release", "cleanup"]);
}
