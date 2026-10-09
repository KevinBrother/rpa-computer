//! Runtime -> real BackendCore -> recording NativeInput integration.
//! No native platform initialization, desktop input or screenshots.
use super::*;
use crate::backend::dispatch::{Axis, BackendCore, NativeInput};
use crate::backend::{Backend, BackendError, Capture, Direction, Geometry};
use rpa_native_input::{Button, Key};
use std::cell::RefCell;
use std::rc::Rc;

type ButtonCalls = Rc<RefCell<Vec<(Button, Direction, u8)>>>;

struct RecordingInput {
    calls: ButtonCalls,
    fail_once: Option<(Direction, u8)>,
}

impl NativeInput for RecordingInput {
    fn move_mouse(&mut self, _x: i32, _y: i32) -> Result<(), BackendError> {
        Ok(())
    }
    fn scroll(&mut self, _length: i32, _axis: Axis) -> Result<(), BackendError> {
        Ok(())
    }
    fn key(&mut self, _key: Key, _direction: Direction) -> Result<(), BackendError> {
        Ok(())
    }
    fn text_scalar(&mut self, _ch: char) -> Result<(), BackendError> {
        Ok(())
    }
    fn button(
        &mut self,
        button: Button,
        direction: Direction,
        count: u8,
    ) -> Result<(), BackendError> {
        self.calls.borrow_mut().push((button, direction, count));
        if self.fail_once == Some((direction, count)) {
            self.fail_once = None;
            Err(be("input_failed", "delivery uncertain"))
        } else {
            Ok(())
        }
    }
}

struct CoreBackend {
    core: BackendCore<RecordingInput>,
    screen: FakeBackend,
    inject_calls: usize,
    cleanup_calls: usize,
}

impl Backend for CoreBackend {
    fn platform(&self) -> &'static str {
        "recording"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.screen.geometry()
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        self.screen.capture()
    }
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        self.inject_calls += 1;
        self.core.inject_event(event)
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        self.cleanup_calls += 1;
        self.core.release_all()
    }
}

fn recording_backend(
    fail_once: Option<(Direction, u8)>,
) -> (Session, CoreBackend, ButtonCalls, Arc<AtomicBool>) {
    let (session, screen, cancel) = setup(32, 32);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let core = BackendCore::new(RecordingInput {
        calls: calls.clone(),
        fail_once,
    });
    (
        session,
        CoreBackend {
            core,
            screen,
            inject_calls: 0,
            cleanup_calls: 0,
        },
        calls,
        cancel,
    )
}

fn run_step(
    s: &mut Session,
    b: &mut CoreBackend,
    cancel: &Arc<AtomicBool>,
    action: serde_json::Value,
    cancel_after_injects: Option<usize>,
) -> StepRecord {
    let obs = capture_observation(s, b, 0, cancel).unwrap();
    let mut ctx = StepContext {
        backend: b,
        cancel: cancel.clone(),
        config: crate::runtime::execute::ExecutionConfig {
            settle_delay: Duration::from_millis(1),
            cancel_after_injects,
            ..crate::runtime::execute::ExecutionConfig::production()
        },
    };
    execute_step(s, &mut ctx, "click", &obs.meta.observation_id, &action)
}

fn pairs(button: Button, count: u8) -> Vec<(Button, Direction, u8)> {
    (1..=count)
        .flat_map(|c| {
            [
                (button, Direction::Press, c),
                (button, Direction::Release, c),
            ]
        })
        .collect()
}

#[test]
fn runtime_counts_one_two_three_reach_native_seam_and_drag_stays_one() {
    for count in 1..=3 {
        let (mut s, mut b, calls, cancel) = recording_backend(None);
        let record = run_step(
            &mut s,
            &mut b,
            &cancel,
            json!({"kind":"click", "position":[1,1], "button":"right", "count":count}),
            None,
        );
        assert_eq!(record.input_outcome, InputOutcome::Dispatched);
        assert_eq!(*calls.borrow(), pairs(Button::Right, count));
        assert!(b.core.held_snapshot().is_empty());
        assert_eq!(b.cleanup_calls, 0);
    }
    let (mut s, mut b, calls, cancel) = recording_backend(None);
    let record = run_step(
        &mut s,
        &mut b,
        &cancel,
        json!({"kind":"drag", "path":[[1,1],[2,2]], "button":"middle", "duration_ms":1}),
        None,
    );
    assert_eq!(record.input_outcome, InputOutcome::Dispatched);
    assert_eq!(*calls.borrow(), pairs(Button::Middle, 1));
    assert!(b.core.held_snapshot().is_empty());
}

#[test]
fn cancellation_during_each_click_or_gap_releases_only_pressed_pairs() {
    // After down1, up1 (gap), down2, up2 (gap), down3 respectively.
    for (threshold, pairs_completed) in [(2, 1), (3, 1), (4, 2), (5, 2), (6, 3)] {
        let (mut s, mut b, calls, cancel) = recording_backend(None);
        let record = run_step(
            &mut s,
            &mut b,
            &cancel,
            json!({"kind":"click", "position":[1,1], "count":3}),
            Some(threshold),
        );
        assert!(record.cancelled);
        assert_eq!(record.input_outcome, InputOutcome::Partial);
        assert_eq!(record.cleanup_outcome, CleanupOutcome::Released);
        assert_eq!(
            *calls.borrow(),
            pairs(Button::Left, pairs_completed),
            "cancel after injection {threshold} must not release a never-pressed later pair"
        );
        assert_eq!(b.cleanup_calls, 1);
        assert!(b.core.held_snapshot().is_empty());
    }
}

#[test]
fn uncertain_press_or_release_cleanup_keeps_pair_metadata_and_skips_later_pairs() {
    for direction in [Direction::Press, Direction::Release] {
        for count in 1..=3 {
            let (mut s, mut b, calls, cancel) = recording_backend(Some((direction, count)));
            let record = run_step(
                &mut s,
                &mut b,
                &cancel,
                json!({"kind":"click", "position":[1,1], "count":3}),
                None,
            );
            assert_eq!(record.input_outcome, InputOutcome::Partial);
            assert_eq!(record.cleanup_outcome, CleanupOutcome::Released);
            assert_eq!(record.error.as_ref().unwrap().code, codes::INPUT_ERROR);
            let mut expected = pairs(Button::Left, count);
            if direction == Direction::Release {
                expected.push((Button::Left, Direction::Release, count));
            }
            assert_eq!(
                *calls.borrow(),
                expected,
                "uncertain {direction:?} for pair {count}"
            );
            assert_eq!(b.cleanup_calls, 1);
            assert!(b.core.held_snapshot().is_empty());
        }
    }
}

#[test]
fn invalid_action_counts_reject_before_any_backend_injection() {
    for count in [0, 4] {
        let (mut s, mut b, calls, cancel) = recording_backend(None);
        let record = run_step(
            &mut s,
            &mut b,
            &cancel,
            json!({"kind":"click", "position":[1,1], "count":count}),
            None,
        );
        assert_eq!(record.input_outcome, InputOutcome::NotStarted);
        assert!(record.error.is_some());
        assert_eq!(b.inject_calls, 0);
        assert!(calls.borrow().is_empty());
        assert!(b.core.held_snapshot().is_empty());
    }
}
