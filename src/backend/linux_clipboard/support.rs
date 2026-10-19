//! A raw input recorder, never a native driver or substitute X11 clipboard.
use rpa_native_input::{Axis, Button, Direction, Driver, InputError, Key, Result};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
pub(super) struct Calls {
    pub events: Vec<&'static str>,
    pub fail_cleanup: bool,
}
#[derive(Clone, Default)]
pub(super) struct RecordingDriver(pub Rc<RefCell<Calls>>);
impl Driver for RecordingDriver {
    fn move_mouse(&mut self, _: i32, _: i32) -> Result<()> {
        self.0.borrow_mut().events.push("mouse");
        Ok(())
    }
    fn button(&mut self, _: Button, _: Direction, _: u8) -> Result<()> {
        self.0.borrow_mut().events.push("button");
        Ok(())
    }
    fn key(&mut self, _: Key, _: Direction) -> Result<()> {
        self.0.borrow_mut().events.push("key");
        Ok(())
    }
    fn scroll(&mut self, _: i32, _: Axis) -> Result<()> {
        self.0.borrow_mut().events.push("scroll");
        Ok(())
    }
    fn text_scalar(&mut self, _: char) -> Result<()> {
        self.0.borrow_mut().events.push("native");
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        let mut calls = self.0.borrow_mut();
        calls.events.push("cleanup");
        if calls.fail_cleanup {
            Err(InputError::new("input_failed", "native cleanup uncertain"))
        } else {
            Ok(())
        }
    }
}
