//! Low-level native input dispatch, isolated behind the [`NativeInput`] trait
//! so held-state tracking, scroll translation and release semantics can be
//! regression-tested with a recording mock (no real desktop input).
//!
//! Contract: all coordinates/directions arriving here are already in native
//! backend input units; scroll deltas follow the contract convention
//! **positive = right/down**, which is exactly enigo 0.3's documented
//! `Mouse::scroll` convention (lib.rs: "positive length will result in
//! scrolling down ... to the right"), so deltas pass through unchanged.

use super::{BackendError, Direction};

/// Native input device abstraction. Production uses [`EnigoInput`]; tests
/// substitute a recording implementation. NOT required to be `Send`: it is
/// constructed and used on the backend worker thread only.
pub(crate) trait NativeInput {
    fn move_mouse(&mut self, x: i32, y: i32) -> Result<(), BackendError>;
    /// `length` in wheel ticks, positive = right (Horizontal) / down
    /// (Vertical). Passed through to enigo unchanged.
    fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), BackendError>;
    fn button(&mut self, button: enigo::Button, direction: Direction) -> Result<(), BackendError>;
    fn key(&mut self, key: enigo::Key, direction: Direction) -> Result<(), BackendError>;
    fn text(&mut self, text: &str) -> Result<(), BackendError>;
}

/// Scroll axis (mirrors `enigo::Axis` without leaking it into the seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

/// Production dispatcher: thin pass-through over `enigo::Enigo`.
///
/// No translation happens here on purpose:
/// - Scroll deltas: enigo 0.3 documents positive = down/right; the internal
///   sign flips it applies for native wheel conventions (Windows negates
///   vertical before `MOUSEEVENTF_WHEEL`, macOS negates into CGEvent wheel
///   deltas) are platform details, not an API-level inversion. Negating here
///   would invert the user's scroll direction.
/// - `Key::Unicode(c)`: enigo maps the character to a virtual key via the
///   current keyboard layout (Windows `VkKeyScanExW`, macOS keycodes) and
///   sends a REAL key event; only if no virtual-key mapping exists does
///   enigo fall back to a text-like path (Windows `KEYEVENTF_UNICODE`). We
///   only ever pass single ASCII alphanumerics, which always map on
///   standard layouts.
pub(crate) struct EnigoInput {
    enigo: enigo::Enigo,
}

impl EnigoInput {
    pub(crate) fn new(enigo: enigo::Enigo) -> Self {
        Self { enigo }
    }

    fn input_error(context: &str, e: enigo::InputError) -> BackendError {
        BackendError::new("input_failed", format!("{context}: {e}"))
    }
}

impl NativeInput for EnigoInput {
    fn move_mouse(&mut self, x: i32, y: i32) -> Result<(), BackendError> {
        use enigo::Mouse;
        self.enigo
            .move_mouse(x, y, enigo::Coordinate::Abs)
            .map_err(|e| Self::input_error("mouse move", e))
    }

    fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), BackendError> {
        use enigo::Mouse;
        let axis = match axis {
            Axis::Horizontal => enigo::Axis::Horizontal,
            Axis::Vertical => enigo::Axis::Vertical,
        };
        self.enigo
            .scroll(length, axis)
            .map_err(|e| Self::input_error("scroll", e))
    }

    fn button(&mut self, button: enigo::Button, direction: Direction) -> Result<(), BackendError> {
        use enigo::Mouse;
        self.enigo
            .button(button, super::keys::enigo_direction(direction))
            .map_err(|e| Self::input_error("mouse button", e))
    }

    fn key(&mut self, key: enigo::Key, direction: Direction) -> Result<(), BackendError> {
        use enigo::Keyboard;
        self.enigo
            .key(key, super::keys::enigo_direction(direction))
            .map_err(|e| Self::input_error("key", e))
    }

    fn text(&mut self, text: &str) -> Result<(), BackendError> {
        use enigo::Keyboard;
        self.enigo
            .text(text)
            .map_err(|e| Self::input_error("text input", e))
    }
}

pub(crate) use crate::backend::dispatch_core::BackendCore;

#[cfg(test)]
mod tests {
    //! Regression tests for the input state machine, driven through a
    //! recording mock dispatcher. No real desktop input is performed.

    use super::{Axis, BackendCore, NativeInput};
    use crate::backend::keys::{parse_button, parse_key};
    use crate::backend::{BackendError, Direction, HeldItem, InputEvent};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    /// One recorded low-level dispatch call.
    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Move(i32, i32),
        Scroll(i32, Axis),
        Button(enigo::Button, Direction),
        Key(enigo::Key, Direction),
        Text(String),
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

        fn held_item_for_key(k: enigo::Key) -> HeldItem {
            // Recover the ParsedKey by matching on the enigo key the state
            // machine would have used.
            for name in ["ctrl", "shift", "a", "l", "enter"] {
                if let Ok(pk) = parse_key(name) {
                    if pk.enigo_key() == k {
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
            button: enigo::Button,
            direction: Direction,
        ) -> Result<(), BackendError> {
            self.calls
                .borrow_mut()
                .push(Call::Button(button, direction));
            self.maybe_fail(HeldItem::Button(button), direction)
        }

        fn key(&mut self, key: enigo::Key, direction: Direction) -> Result<(), BackendError> {
            self.calls.borrow_mut().push(Call::Key(key, direction));
            self.maybe_fail(Self::held_item_for_key(key), direction)
        }

        fn text(&mut self, text: &str) -> Result<(), BackendError> {
            self.calls.borrow_mut().push(Call::Text(text.to_string()));
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
            "positive deltas must scroll right/down per enigo 0.3 API docs; \
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
                Call::Key(enigo::Key::Control, Direction::Press),
                Call::Key(enigo::Key::Control, Direction::Release),
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
            direction: Direction::Press,
        })
        .unwrap();

        // First release attempt fails: item must remain tracked.
        c.input_mut().fail_release.insert(item);
        let err = c
            .inject_event(&InputEvent::Button {
                button: "left".into(),
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
            direction: Direction::Release,
        })
        .unwrap();
        assert!(c.held_snapshot().is_empty());
        // The dispatcher saw press + two release attempts (failed + retry).
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(left, Direction::Press),
                Call::Button(left, Direction::Release),
                Call::Button(left, Direction::Release),
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
            err.message.contains("Shift"),
            "error names the failed item: {}",
            err.message
        );
        // All three items were attempted (button first, then keys by name).
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(enigo::Button::Left, Direction::Release),
                Call::Key(enigo::Key::Control, Direction::Release),
                Call::Key(enigo::Key::Shift, Direction::Release),
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
            vec![Call::Key(enigo::Key::Shift, Direction::Release)],
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

    #[test]
    fn move_and_text_pass_through() {
        let (mut c, calls) = core();
        c.inject_event(&InputEvent::Move { x: 100, y: -20 })
            .unwrap();
        c.inject_event(&InputEvent::Text {
            text: "hi 世界".into(),
        })
        .unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Move(100, -20), Call::Text("hi 世界".into())]
        );
        assert!(c.held_snapshot().is_empty(), "text/move never tracked");
    }

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
            vec![Call::Key(enigo::Key::Unicode('l'), Direction::Press)],
            "ASCII chord keys must be real key events (enigo maps them to \
             virtual keys via the current layout), never the text path"
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
            direction: Direction::Press,
        })
        .unwrap();
        calls.borrow_mut().clear();
        drop(c);
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(enigo::Button::Right, Direction::Release),
                Call::Key(enigo::Key::Shift, Direction::Release),
            ],
            "Drop must best-effort release everything still held"
        );
    }
}
