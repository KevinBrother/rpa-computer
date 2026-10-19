use super::*;
use crate::{Axis, Button, Direction, Driver, Input, Key};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Default)]
struct NativeCalls {
    calls: Vec<(u32, bool)>,
    failures: VecDeque<bool>,
}
impl NativeCalls {
    fn send(&mut self, button: u32, press: bool) -> Result<()> {
        self.calls.push((button, press));
        if self.failures.pop_front().unwrap_or(false) {
            Err(InputError::new(
                "input_failed",
                "injected XTest button failure",
            ))
        } else {
            Ok(())
        }
    }
}

#[test]
fn successful_retry_does_not_hide_the_first_release_failure() {
    let mut transient = TransientButtons::default();
    let mut native = NativeCalls {
        failures: [false, true, false].into(),
        ..Default::default()
    };
    let result = transient.click(4, |b, p| native.send(b, p));
    assert!(
        result.is_err(),
        "scroll must report the original release failure"
    );
    assert_eq!(native.calls, [(4, true), (4, false), (4, false)]);
    assert!(
        transient.pending_ids().is_empty(),
        "successful retry can confirm key-up, not erase error"
    );
}

#[test]
fn both_failed_releases_retain_the_exact_wheel_button_for_cleanup() {
    let mut transient = TransientButtons::default();
    let mut native = NativeCalls {
        failures: [false, true, true].into(),
        ..Default::default()
    };
    assert!(transient.click(7, |b, p| native.send(b, p)).is_err());
    assert_eq!(transient.pending_ids(), [7]);
    native.failures = [true].into();
    assert!(transient.cleanup(|b, p| native.send(b, p)).is_err());
    assert_eq!(transient.pending_ids(), [7]);
    transient.cleanup(|b, p| native.send(b, p)).unwrap();
    assert!(transient.pending_ids().is_empty());
    assert_eq!(&native.calls[3..], [(7, false), (7, false)]);
}

#[test]
fn uncertain_press_is_released_or_retained_never_forgotten() {
    let mut transient = TransientButtons::default();
    let mut native = NativeCalls {
        failures: [true, true, true].into(),
        ..Default::default()
    };
    assert!(transient.click(5, |b, p| native.send(b, p)).is_err());
    assert_eq!(
        transient.pending_ids(),
        [5],
        "failed native press and failed releases cannot discard cleanup responsibility"
    );
}

#[test]
fn new_tick_is_rejected_while_an_owned_release_remains_uncertain() {
    let mut transient = TransientButtons::default();
    let mut native = NativeCalls {
        failures: [false, true, true].into(),
        ..Default::default()
    };
    assert!(transient.click(4, |b, p| native.send(b, p)).is_err());
    native.calls.clear();
    assert!(transient.click(6, |b, p| native.send(b, p)).is_err());
    assert!(
        !native.calls.iter().any(|(_, press)| *press),
        "do not start more presses while cleanup is pending"
    );
}

struct WheelDriver {
    transient: TransientButtons,
    native: Rc<RefCell<NativeCalls>>,
}
impl Driver for WheelDriver {
    fn move_mouse(&mut self, _: i32, _: i32) -> Result<()> {
        Ok(())
    }
    fn key(&mut self, _: Key, _: Direction) -> Result<()> {
        Ok(())
    }
    fn button(&mut self, _: Button, _: Direction, _: u8) -> Result<()> {
        Ok(())
    }
    fn text_scalar(&mut self, _: char) -> Result<()> {
        Ok(())
    }
    fn scroll(&mut self, _: i32, _: Axis) -> Result<()> {
        self.transient
            .click(6, |b, p| self.native.borrow_mut().send(b, p))
    }
    fn cleanup(&mut self) -> Result<()> {
        self.transient
            .cleanup(|b, p| self.native.borrow_mut().send(b, p))
    }
}

#[test]
fn empty_wrapper_held_set_must_not_report_wheel_cleanup_released() {
    let native = Rc::new(RefCell::new(NativeCalls {
        failures: [false, true, true, true].into(),
        ..Default::default()
    }));
    let mut input = Input::from_driver(Box::new(WheelDriver {
        transient: TransientButtons::default(),
        native: native.clone(),
    }))
    .unwrap();
    assert!(input.scroll(1, Axis::Horizontal).is_err());
    assert!(
        input.release_all().is_err(),
        "driver pending wheel state must escape the empty wrapper held set"
    );
    native.borrow_mut().failures.clear();
    input.release_all().unwrap();
    assert_eq!(native.borrow().calls.last(), Some(&(6, false)));
}
