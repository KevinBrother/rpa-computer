//! Low-level native input dispatch, isolated behind the [`NativeInput`] trait
//! so held-state tracking, scroll translation and release semantics can be
//! regression-tested with a recording mock (no real desktop input).
//!
//! Contract: all coordinates/directions arriving here are already in native
//! backend input units; scroll deltas follow the contract convention
//! **positive = right/down**, which is exactly the `rpa-native-input` crate's
//! documented `Driver::scroll` convention, so deltas pass through unchanged.
//!
//! Single held-state authority: the production dispatcher wraps the raw
//! `rpa_native_input::Driver` (from [`rpa_native_input::native_driver`]) and
//! does NOT use the crate's `Input` session wrapper — `BackendCore` below is
//! the one component that tracks held keys/buttons in this process. `Input`
//! remains the higher-level API for standalone users of the crate.

use super::{BackendError, Direction};
use rpa_native_input::{Button, Driver, InputError, Key};

/// Native input device abstraction. Production uses [`DriverInput`]; tests
/// substitute a recording implementation. NOT required to be `Send`: it is
/// constructed and used on the backend worker thread only.
pub(crate) trait NativeInput {
    fn move_mouse(&mut self, x: i32, y: i32) -> Result<(), BackendError>;
    /// `length` in wheel ticks, positive = right (Horizontal) / down
    /// (Vertical). Passed through to the driver unchanged.
    fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), BackendError>;
    /// `click_count` is the native click-state of THIS press/up pair
    /// (valid 1..=3, validated upstream by `BackendCore` and again by the
    /// driver); passed through to the driver unchanged.
    fn button(
        &mut self,
        button: Button,
        direction: Direction,
        click_count: u8,
    ) -> Result<(), BackendError>;
    fn key(&mut self, key: Key, direction: Direction) -> Result<(), BackendError>;
    /// One Unicode scalar on the native text path. `'\n'`/`'\r'`→Return and
    /// `'\t'`→Tab mapping is applied by `BackendCore` (so the clicks go
    /// through its held-state machine); everything else lands here.
    fn text_scalar(&mut self, ch: char) -> Result<(), BackendError>;
}

/// Scroll axis (mirrors `rpa_native_input::Axis` without leaking it into the seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

/// Production dispatcher: thin adapter over the raw platform `Driver`.
///
/// No translation happens here on purpose:
/// - Scroll deltas: the crate documents positive = down/right; any native
///   wheel-delta sign handling is a platform detail inside the driver.
///   Negating here would invert the user's scroll direction.
/// - `Key::Character(c)`: the driver maps the character to a real key event
///   via the current keyboard layout. We only ever pass single ASCII
///   alphanumerics.
pub(crate) struct DriverInput {
    driver: Box<dyn Driver>,
}

impl DriverInput {
    pub(crate) fn new(driver: Box<dyn Driver>) -> Self {
        Self { driver }
    }
}

impl NativeInput for DriverInput {
    fn move_mouse(&mut self, x: i32, y: i32) -> Result<(), BackendError> {
        self.driver
            .move_mouse(x, y)
            .map_err(|e| input_error("mouse move", e))
    }

    fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), BackendError> {
        let axis = match axis {
            Axis::Horizontal => rpa_native_input::Axis::Horizontal,
            Axis::Vertical => rpa_native_input::Axis::Vertical,
        };
        self.driver
            .scroll(length, axis)
            .map_err(|e| input_error("scroll", e))
    }

    fn button(
        &mut self,
        button: Button,
        direction: Direction,
        click_count: u8,
    ) -> Result<(), BackendError> {
        let d = match direction {
            Direction::Press => rpa_native_input::Direction::Press,
            Direction::Release => rpa_native_input::Direction::Release,
        };
        self.driver
            .button(button, d, click_count)
            .map_err(|e| input_error("mouse button", e))
    }

    fn key(&mut self, key: Key, direction: Direction) -> Result<(), BackendError> {
        let d = match direction {
            Direction::Press => rpa_native_input::Direction::Press,
            Direction::Release => rpa_native_input::Direction::Release,
        };
        self.driver.key(key, d).map_err(|e| input_error("key", e))
    }

    fn text_scalar(&mut self, ch: char) -> Result<(), BackendError> {
        self.driver
            .text_scalar(ch)
            .map_err(|e| input_error("text input", e))
    }
}

fn input_error(context: &str, e: InputError) -> BackendError {
    // Preserve the driver's diagnostic code (permission_denied, no_display,
    // unsupported_session, invalid_text, ...) rather than flattening every
    // failure into input_failed; the protocol mapper in runtime/error.rs
    // maps unknown codes safely. Diagnostic context stays in the message.
    BackendError::new(e.code.to_owned(), format!("{context}: {e}"))
}

pub(crate) use crate::backend::dispatch_core::BackendCore;

#[cfg(test)]
mod tests {
    //! Regression tests for the input state machine, driven through a
    //! recording mock dispatcher. No real desktop input is performed.

    use super::{Axis, BackendCore, DriverInput, NativeInput};
    use crate::backend::keys::{parse_button, parse_key};
    use crate::backend::{BackendError, Direction, HeldItem, InputEvent};
    use rpa_native_input::{Button, Driver, InputError, Key};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    /// One recorded low-level dispatch call.
    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Move(i32, i32),
        Scroll(i32, Axis),
        Button(Button, Direction, u8),
        Key(Key, Direction),
        Scalar(char),
    }

    /// Recording dispatcher; can be told to fail selected calls. The call
    /// log is shared so it can be inspected even after the owning
    /// `BackendCore` (and its Drop) has run.
    struct MockInput {
        calls: Rc<RefCell<Vec<Call>>>,
        /// Release failures are injected for these held items.
        fail_release: HashSet<HeldItem>,
        /// When set, the next press of this item fails (simulating a
        /// partially delivered press).
        fail_press: Option<HeldItem>,
    }

    impl MockInput {
        fn new() -> (Self, Rc<RefCell<Vec<Call>>>) {
            let calls = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    calls: Rc::clone(&calls),
                    fail_release: HashSet::new(),
                    fail_press: None,
                },
                calls,
            )
        }

        fn held_item_for_key(k: Key) -> HeldItem {
            // Recover the ParsedKey by matching on the native key the state
            // machine would have used.
            for name in ["ctrl", "shift", "a", "l", "enter", "tab"] {
                if let Ok(pk) = parse_key(name) {
                    if pk.key() == k {
                        return HeldItem::Key(pk);
                    }
                }
            }
            panic!("mock does not know key {k:?}")
        }

        fn maybe_fail(&mut self, item: HeldItem, direction: Direction) -> Result<(), BackendError> {
            match direction {
                Direction::Release if self.fail_release.contains(&item) => Err(BackendError::new(
                    "input_failed",
                    format!("mock release failure for {item}"),
                )),
                Direction::Press if self.fail_press == Some(item) => {
                    self.fail_press = None;
                    Err(BackendError::new(
                        "input_failed",
                        format!("mock press failure for {item}"),
                    ))
                }
                _ => Ok(()),
            }
        }

        /// Stop failing releases for `item` (the "device recovers" case).
        fn heal_release(&mut self, item: HeldItem) {
            self.fail_release.remove(&item);
        }
    }

    impl NativeInput for MockInput {
        fn move_mouse(&mut self, x: i32, y: i32) -> Result<(), BackendError> {
            self.calls.borrow_mut().push(Call::Move(x, y));
            Ok(())
        }

        fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), BackendError> {
            self.calls.borrow_mut().push(Call::Scroll(length, axis));
            Ok(())
        }

        fn button(
            &mut self,
            button: Button,
            direction: Direction,
            click_count: u8,
        ) -> Result<(), BackendError> {
            self.calls
                .borrow_mut()
                .push(Call::Button(button, direction, click_count));
            self.maybe_fail(HeldItem::Button(button), direction)
        }

        fn key(&mut self, key: Key, direction: Direction) -> Result<(), BackendError> {
            self.calls.borrow_mut().push(Call::Key(key, direction));
            self.maybe_fail(Self::held_item_for_key(key), direction)
        }

        fn text_scalar(&mut self, ch: char) -> Result<(), BackendError> {
            self.calls.borrow_mut().push(Call::Scalar(ch));
            Ok(())
        }
    }

    fn core() -> (BackendCore<MockInput>, Rc<RefCell<Vec<Call>>>) {
        let (input, calls) = MockInput::new();
        (BackendCore::new(input), calls)
    }

    // -----------------------------------------------------------------
    // Scroll translation (P1: direction must NOT be inverted)
    // -----------------------------------------------------------------

    #[test]
    fn scroll_passes_deltas_through_unchanged() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Scroll { x: 3, y: -5 }).unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Scroll(3, Axis::Horizontal),
                Call::Scroll(-5, Axis::Vertical),
            ],
            "positive deltas must scroll right/down per the driver contract; \
             negating here would invert the user's scroll"
        );
    }

    #[test]
    fn scroll_skips_zero_axes_and_keeps_signs() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Scroll { x: 0, y: 2 }).unwrap();
        assert_eq!(*calls.borrow(), vec![Call::Scroll(2, Axis::Vertical)]);
        calls.borrow_mut().clear();
        c.inject_event(&InputEvent::Scroll { x: -4, y: 0 }).unwrap();
        assert_eq!(*calls.borrow(), vec![Call::Scroll(-4, Axis::Horizontal)]);
    }

    // -----------------------------------------------------------------
    // Held-state: press tracking
    // -----------------------------------------------------------------

    #[test]
    fn successful_press_and_release_update_tracking() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Key {
            key: "ctrl".into(),
            direction: Direction::Press,
        })
        .unwrap();
        assert_eq!(
            c.held_snapshot(),
            vec![HeldItem::Key(parse_key("ctrl").unwrap())]
        );

        c.inject_event(&InputEvent::Key {
            key: "ctrl".into(),
            direction: Direction::Release,
        })
        .unwrap();
        assert!(c.held_snapshot().is_empty());
        // Both events reached the dispatcher in order.
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Key(Key::Control, Direction::Press),
                Call::Key(Key::Control, Direction::Release),
            ]
        );
    }

    #[test]
    fn failed_press_still_tracks_item_for_later_release() {
        let (mut c, _calls) = core();
        let ctrl = HeldItem::Key(parse_key("ctrl").unwrap());
        c.input_mut().fail_press = Some(ctrl);
        let err = c
            .inject_event(&InputEvent::Key {
                key: "ctrl".into(),
                direction: Direction::Press,
            })
            .unwrap_err();
        assert_eq!(err.code, "input_failed");
        // Partial delivery is possible: the item MUST remain tracked.
        assert_eq!(c.held_snapshot(), vec![ctrl]);
        // And release_all releases it.
        c.release_all().unwrap();
        assert!(c.held_snapshot().is_empty());
    }

    // -----------------------------------------------------------------
    // Held-state: release must retain on failure (P1)
    // -----------------------------------------------------------------

    #[test]
    fn failed_release_keeps_item_tracked_and_release_is_retried() {
        let (mut c, calls) = core();
        let left = parse_button("left").unwrap();
        let item = HeldItem::Button(left);
        c.inject_event(&InputEvent::Button {
            button: "left".into(),
            click_count: 1,
            direction: Direction::Press,
        })
        .unwrap();

        // First release attempt fails: item must remain tracked.
        c.input_mut().fail_release.insert(item);
        let err = c
            .inject_event(&InputEvent::Button {
                button: "left".into(),
                click_count: 1,
                direction: Direction::Release,
            })
            .unwrap_err();
        assert_eq!(err.code, "input_failed");
        assert_eq!(
            c.held_snapshot(),
            vec![item],
            "a failed release must NOT forget the held item"
        );

        // Retry now succeeds (the "device recovered") and clears tracking.
        c.input_mut().heal_release(item);
        c.inject_event(&InputEvent::Button {
            button: "left".into(),
            click_count: 1,
            direction: Direction::Release,
        })
        .unwrap();
        assert!(c.held_snapshot().is_empty());
        // The dispatcher saw press + two release attempts (failed + retry).
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(left, Direction::Press, 1),
                Call::Button(left, Direction::Release, 1),
                Call::Button(left, Direction::Release, 1),
            ]
        );
    }

    #[test]
    fn release_all_preserves_failures_and_later_retry_actually_attempts_them() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Key {
            key: "ctrl".into(),
            direction: Direction::Press,
        })
        .unwrap();
        c.inject_event(&InputEvent::Key {
            key: "shift".into(),
            direction: Direction::Press,
        })
        .unwrap();
        c.inject_event(&InputEvent::Button {
            button: "left".into(),
            click_count: 1,
            direction: Direction::Press,
        })
        .unwrap();

        // Make one release fail; release_all must still try every item.
        let shift = HeldItem::Key(parse_key("shift").unwrap());
        c.input_mut().fail_release.insert(shift);
        calls.borrow_mut().clear();
        let err = c.release_all().unwrap_err();
        assert_eq!(err.code, "input_failed");
        assert!(
            err.message.contains("shift"),
            "error names the failed item: {}",
            err.message
        );
        // All three items were attempted (button first, then keys by name).
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Left, Direction::Release, 1),
                Call::Key(Key::Control, Direction::Release),
                Call::Key(Key::Shift, Direction::Release),
            ]
        );
        // Only the failed item remains tracked.
        assert_eq!(c.held_snapshot(), vec![shift]);

        // A later retry MUST actually attempt the failed item again.
        calls.borrow_mut().clear();
        c.input_mut().heal_release(shift);
        c.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Key(Key::Shift, Direction::Release)],
            "retry must dispatch a real release attempt for previously failed items"
        );
        assert!(c.held_snapshot().is_empty());
    }

    #[test]
    fn release_all_with_empty_held_set_is_noop_success() {
        let (mut c, calls) = core();
        c.release_all().unwrap();
        assert!(calls.borrow().is_empty());
    }

    // -----------------------------------------------------------------
    // Validation before any dispatch
    // -----------------------------------------------------------------

    #[test]
    fn invalid_names_fail_before_any_dispatch() {
        let (mut c, calls) = core();
        let err = c
            .inject_event(&InputEvent::Key {
                key: "f13".into(),
                direction: Direction::Press,
            })
            .unwrap_err();
        assert_eq!(err.code, "invalid_key");
        let err = c
            .inject_event(&InputEvent::Button {
                button: "aux".into(),
                click_count: 1,
                direction: Direction::Press,
            })
            .unwrap_err();
        assert_eq!(err.code, "invalid_button");
        let err = c
            .inject_event(&InputEvent::Text {
                text: "a\0b".into(),
            })
            .unwrap_err();
        assert_eq!(err.code, "invalid_text");
        assert!(
            calls.borrow().is_empty(),
            "validation failures must not emit any input"
        );
        assert!(c.held_snapshot().is_empty());
    }

    // -----------------------------------------------------------------
    // Text: per-scalar dispatch, Return/Tab clicks, CRLF normalization
    // -----------------------------------------------------------------

    #[test]
    fn move_and_text_pass_through_per_scalar() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Move { x: 100, y: -20 })
            .unwrap();
        c.inject_event(&InputEvent::Text {
            text: "hi 世界".into(),
        })
        .unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Move(100, -20),
                Call::Scalar('h'),
                Call::Scalar('i'),
                Call::Scalar(' '),
                Call::Scalar('世'),
                Call::Scalar('界'),
            ]
        );
        assert!(c.held_snapshot().is_empty(), "text/move never tracked");
    }

    #[test]
    fn newline_scalars_become_exactly_one_return_click_through_held_state() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Text { text: "\n".into() })
            .unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Key(Key::Return, Direction::Press),
                Call::Key(Key::Return, Direction::Release),
            ],
            "'\\n' must produce ONE Return click, never a double Return or a \
             Unicode scalar injection"
        );
        assert!(c.held_snapshot().is_empty());
    }

    #[test]
    fn crlf_is_normalized_to_a_single_return_click() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Text {
            text: "a\r\nb".into(),
        })
        .unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Scalar('a'),
                Call::Key(Key::Return, Direction::Press),
                Call::Key(Key::Return, Direction::Release),
                Call::Scalar('b'),
            ],
            "CRLF must be normalized once by the caller: exactly one Return \
             click between the surrounding scalars"
        );
    }

    #[test]
    fn lone_carriage_return_is_one_return_click() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Text { text: "\r".into() })
            .unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Key(Key::Return, Direction::Press),
                Call::Key(Key::Return, Direction::Release),
            ]
        );
    }

    #[test]
    fn tab_scalar_is_one_tab_click() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Text {
            text: "a\tb".into(),
        })
        .unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Scalar('a'),
                Call::Key(Key::Tab, Direction::Press),
                Call::Key(Key::Tab, Direction::Release),
                Call::Scalar('b'),
            ]
        );
    }

    #[test]
    fn failed_return_click_half_keeps_return_tracked() {
        let (mut c, _calls) = core();
        // The release half of the '\n' Return click fails: Return must stay
        // tracked so release_all cleans it up.
        c.input_mut()
            .fail_release
            .insert(HeldItem::Key(parse_key("enter").unwrap()));
        let err = c
            .inject_event(&InputEvent::Text { text: "\n".into() })
            .unwrap_err();
        assert_eq!(err.code, "input_failed");
        assert_eq!(
            c.held_snapshot(),
            vec![HeldItem::Key(parse_key("enter").unwrap())]
        );
        c.input_mut()
            .heal_release(HeldItem::Key(parse_key("enter").unwrap()));
        c.release_all().unwrap();
        assert!(c.held_snapshot().is_empty());
    }

    // -----------------------------------------------------------------
    // ASCII chord keys are real key events, never the text path
    // -----------------------------------------------------------------

    #[test]
    fn ascii_key_uses_real_key_event_not_text() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Key {
            key: "l".into(),
            direction: Direction::Press,
        })
        .unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Key(Key::Character('l'), Direction::Press)],
            "ASCII chord keys must be real key events (the driver maps them \
             to virtual keys via the current layout), never the text path"
        );
    }

    #[test]
    fn drop_attempts_release_of_remaining_held_items() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Key {
            key: "shift".into(),
            direction: Direction::Press,
        })
        .unwrap();
        c.inject_event(&InputEvent::Button {
            button: "right".into(),
            click_count: 1,
            direction: Direction::Press,
        })
        .unwrap();
        calls.borrow_mut().clear();
        drop(c);
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Right, Direction::Release, 1),
                Call::Key(Key::Shift, Direction::Release),
            ],
            "Drop must best-effort release everything still held"
        );
    }

    // Keep the unused-import lint honest about the error type used above.
    #[allow(dead_code)]
    fn _assert_error_shape(_: InputError) {}

    // -----------------------------------------------------------------
    // Error-code passthrough (DriverInput must NOT flatten driver codes
    // like permission_denied/no_display/unsupported_session/invalid_text
    // into input_failed)
    // -----------------------------------------------------------------

    /// Stub platform driver that fails every call with a fixed code.
    struct FailingDriver {
        code: &'static str,
    }

    impl FailingDriver {
        fn err(&self, what: &str) -> InputError {
            InputError::new(self.code, format!("stub failure: {what}"))
        }
    }

    impl Driver for FailingDriver {
        fn move_mouse(&mut self, _x: i32, _y: i32) -> rpa_native_input::Result<()> {
            Err(self.err("move"))
        }
        fn scroll(
            &mut self,
            _length: i32,
            _axis: rpa_native_input::Axis,
        ) -> rpa_native_input::Result<()> {
            Err(self.err("scroll"))
        }
        fn button(
            &mut self,
            _b: rpa_native_input::Button,
            _d: rpa_native_input::Direction,
            _c: u8,
        ) -> rpa_native_input::Result<()> {
            Err(self.err("button"))
        }
        fn key(
            &mut self,
            _k: rpa_native_input::Key,
            _d: rpa_native_input::Direction,
        ) -> rpa_native_input::Result<()> {
            Err(self.err("key"))
        }
        fn text_scalar(&mut self, _ch: char) -> rpa_native_input::Result<()> {
            Err(self.err("text"))
        }
    }

    #[test]
    fn driver_input_preserves_driver_error_codes() {
        for code in [
            "permission_denied",
            "no_display",
            "unsupported_session",
            "invalid_text",
        ] {
            let mut input = DriverInput::new(Box::new(FailingDriver { code }));
            let cases: Vec<(
                &str,
                Box<dyn FnOnce(&mut DriverInput) -> Result<(), BackendError>>,
            )> = vec![
                ("move", Box::new(|i: &mut DriverInput| i.move_mouse(1, 1))),
                (
                    "scroll",
                    Box::new(|i: &mut DriverInput| i.scroll(1, super::Axis::Vertical)),
                ),
                (
                    "button",
                    Box::new(|i: &mut DriverInput| {
                        i.button(rpa_native_input::Button::Left, Direction::Press, 1)
                    }),
                ),
                (
                    "key",
                    Box::new(|i: &mut DriverInput| {
                        i.key(rpa_native_input::Key::Return, Direction::Press)
                    }),
                ),
                ("text", Box::new(|i: &mut DriverInput| i.text_scalar('a'))),
            ];
            for (what, f) in cases {
                let err = f(&mut input).expect_err(what);
                assert_eq!(err.code, code, "{what} must preserve driver code");
                assert!(err.message.contains(what), "{what} keeps its context");
            }
        }
    }
    fn button_event(button: &str, direction: Direction, click_count: u8) -> InputEvent {
        InputEvent::Button {
            button: button.into(),
            direction,
            click_count,
        }
    }

    #[test]
    fn atomic_counts_reach_driver_unchanged_and_invalid_counts_dispatch_nothing() {
        let (mut c, calls) = core();
        for count in 1..=3 {
            c.inject_event(&button_event("right", Direction::Press, count))
                .unwrap();
            c.inject_event(&button_event("right", Direction::Release, count))
                .unwrap();
        }
        let expected: Vec<_> = (1..=3)
            .flat_map(|count| {
                [
                    Call::Button(Button::Right, Direction::Press, count),
                    Call::Button(Button::Right, Direction::Release, count),
                ]
            })
            .collect();
        assert_eq!(*calls.borrow(), expected);
        assert!(c.held_snapshot().is_empty());
        calls.borrow_mut().clear();
        for bad in [0, 4, 255] {
            for direction in [Direction::Press, Direction::Release] {
                assert_eq!(
                    c.inject_event(&button_event("left", direction, bad))
                        .unwrap_err()
                        .code,
                    "invalid_button"
                );
            }
        }
        c.release_all().unwrap();
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn invalid_release_does_not_erase_last_press_metadata() {
        let (mut c, calls) = core();
        c.inject_event(&button_event("left", Direction::Press, 3))
            .unwrap();
        calls.borrow_mut().clear();
        assert!(c
            .inject_event(&button_event("left", Direction::Release, 0))
            .is_err());
        assert!(calls.borrow().is_empty());
        c.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Left, Direction::Release, 3)]
        );
    }

    #[test]
    fn uncertain_button_press_cleanup_retries_same_metadata_and_all_other_items() {
        let (mut c, calls) = core();
        let left = HeldItem::Button(Button::Left);
        c.input_mut().fail_press = Some(left);
        assert!(c
            .inject_event(&button_event("left", Direction::Press, 2))
            .is_err());
        c.inject_event(&button_event("right", Direction::Press, 3))
            .unwrap();
        c.inject_event(&InputEvent::Key {
            key: "shift".into(),
            direction: Direction::Press,
        })
        .unwrap();
        c.input_mut().fail_release.insert(left);
        calls.borrow_mut().clear();
        assert!(c.release_all().is_err());
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Left, Direction::Release, 2),
                Call::Button(Button::Right, Direction::Release, 3),
                Call::Key(Key::Shift, Direction::Release),
            ]
        );
        assert_eq!(c.held_snapshot(), vec![left]);
        calls.borrow_mut().clear();
        // Repeated failed cleanup retains the SAME count, not a default 1.
        assert!(c.release_all().is_err());
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Left, Direction::Release, 2)]
        );
        c.input_mut().heal_release(left);
        calls.borrow_mut().clear();
        c.release_all().unwrap();
        c.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Left, Direction::Release, 2)]
        );
        assert!(c.held_snapshot().is_empty());
    }

    #[test]
    fn repeated_button_press_is_one_held_entry_with_latest_uncertain_metadata() {
        let (mut c, calls) = core();
        let item = HeldItem::Button(Button::Left);
        c.inject_event(&button_event("left", Direction::Press, 1))
            .unwrap();
        c.input_mut().fail_release.insert(item);
        assert!(c
            .inject_event(&button_event("left", Direction::Release, 1))
            .is_err());
        c.inject_event(&button_event("left", Direction::Press, 2))
            .unwrap();
        c.input_mut().fail_press = Some(item);
        assert!(c
            .inject_event(&button_event("left", Direction::Press, 3))
            .is_err());
        assert_eq!(c.held_snapshot(), vec![item]);
        c.input_mut().heal_release(item);
        calls.borrow_mut().clear();
        c.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Left, Direction::Release, 3)]
        );
    }

    #[test]
    fn drop_retries_failed_cleanup_with_latest_button_metadata() {
        let (mut c, calls) = core();
        c.inject_event(&button_event("middle", Direction::Press, 3))
            .unwrap();
        let item = HeldItem::Button(Button::Middle);
        c.input_mut().fail_release.insert(item);
        assert!(c.release_all().is_err());
        c.input_mut().heal_release(item);
        calls.borrow_mut().clear();
        drop(c);
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Middle, Direction::Release, 3)]
        );
    }
    #[test]
    fn host_raw_driver_seam_carries_each_pair_once_without_an_input_wrapper() {
        struct RawRecorder(Rc<RefCell<Vec<(Button, rpa_native_input::Direction, u8)>>>);
        impl Driver for RawRecorder {
            fn move_mouse(&mut self, _x: i32, _y: i32) -> rpa_native_input::Result<()> {
                Ok(())
            }
            fn scroll(
                &mut self,
                _n: i32,
                _axis: rpa_native_input::Axis,
            ) -> rpa_native_input::Result<()> {
                Ok(())
            }
            fn key(
                &mut self,
                _key: Key,
                _direction: rpa_native_input::Direction,
            ) -> rpa_native_input::Result<()> {
                Ok(())
            }
            fn text_scalar(&mut self, _ch: char) -> rpa_native_input::Result<()> {
                Ok(())
            }
            fn button(
                &mut self,
                b: Button,
                d: rpa_native_input::Direction,
                c: u8,
            ) -> rpa_native_input::Result<()> {
                self.0.borrow_mut().push((b, d, c));
                Ok(())
            }
        }
        let calls = Rc::new(RefCell::new(Vec::new()));
        let raw = RawRecorder(calls.clone());
        let mut c = BackendCore::new(DriverInput::new(Box::new(raw)));
        for count in 1..=3 {
            c.inject_event(&button_event("left", Direction::Press, count))
                .unwrap();
            c.inject_event(&button_event("left", Direction::Release, count))
                .unwrap();
        }
        c.release_all().unwrap();
        let expected: Vec<_> = (1..=3)
            .flat_map(|count| {
                [
                    (Button::Left, rpa_native_input::Direction::Press, count),
                    (Button::Left, rpa_native_input::Direction::Release, count),
                ]
            })
            .collect();
        assert_eq!(*calls.borrow(), expected);
        calls.borrow_mut().clear();
        assert!(c
            .inject_event(&button_event("left", Direction::Press, 0))
            .is_err());
        assert!(c
            .inject_event(&button_event("left", Direction::Release, 4))
            .is_err());
        c.release_all().unwrap();
        assert!(calls.borrow().is_empty());
    }
}
