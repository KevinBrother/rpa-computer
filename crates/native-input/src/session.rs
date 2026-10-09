//! [`Input`]: the reusable session wrapper owning local held-key/button state
//! on top of any [`Driver`].
//!
//! Semantics (mirrors the host runtime's state machine, see the module docs
//! in `session.rs` there):
//! - **Press**: the item is recorded as held BEFORE the dispatch, so a
//!   partially delivered (error-returning) press is still cleaned up later.
//! - **Release**: dispatched FIRST; the item is removed from tracking ONLY on
//!   success. A failed release stays held so a later release or
//!   [`Input::release_all`] genuinely retries it.
//! - **release_all**: attempts EVERY held item even after failures; failures
//!   stay tracked and are reported in one aggregated `input_failed` error.
//! - Only items THIS session pressed are ever released — never unrelated
//!   user input.
//!
//! Validation happens before any dispatch: `Key::Character` must be a single
//! ASCII letter/digit (physical chord key, not text injection) and is
//! rejected with `invalid_key` otherwise. `text_scalar` rejects NUL with
//! `invalid_text`, maps `'\n'`/`'\r'` to ONE Return click and `'\t'` to ONE
//! Tab click each (the caller normalizes CRLF sequences once, so `"\r\n"`
//! must not reach `text_scalar` as two scalars).
//!
//! Held state follows the **cooperative desktop assumption**: `Input` tracks
//! only what IT pressed, so `release_all` cleans up session-owned items
//! only. It does NOT (and cannot) protect against a physical user pressing
//! the same key simultaneously — concurrent human and automated input
//! contending for the same key/button state is outside this crate's
//! guarantees. Explicit low-level release calls for items this session never
//! pressed remain the caller's responsibility (documented, not prevented).
//!
//! Note on X11: text injection there is RESTRICTED — only characters mapped
//! in the current keymap without Shift are accepted; unmapped or
//! shifted-only characters are rejected honestly with `invalid_text`. See
//! the crate README.

use std::collections::{HashMap, HashSet};

use crate::types::{Axis, Button, Direction, Driver, InputError, Key, Result};

/// Test diagnostic view of the separate key/button stores.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HeldItem {
    Key(Key),
    Button(Button),
}

#[cfg(test)]
impl HeldItem {
    fn name(&self) -> String {
        match self {
            HeldItem::Key(k) => format!("key({})", k.name()),
            HeldItem::Button(b) => format!("button({b:?})"),
        }
    }

    /// Deterministic ordering for release attempts (buttons first, then keys
    /// by name) so diagnostics are reproducible.
    fn sort_key(&self) -> (u8, String) {
        match self {
            HeldItem::Button(b) => (0, format!("{b:?}")),
            HeldItem::Key(k) => (1, k.name()),
        }
    }
}

/// A native input session over a raw [`Driver`]. Intentionally NOT `Send`:
/// thread-bound devices are created and used on one worker thread.
///
/// Buttons are tracked per-button with the LAST press metadata (the native
/// click-state this session pressed them with) in `held_buttons`; a repeated
/// press of the same button overwrites the metadata instead of accumulating
/// duplicate entries, so `release_all`/`Drop` release each held button
/// exactly once with its matching click-state.
pub struct Input {
    driver: Box<dyn Driver>,
    held_keys: HashSet<Key>,
    held_buttons: HashMap<Button, u8>,
}

impl Input {
    /// Create a session on the real platform device (Windows/macOS/X11
    /// `Platform`). Fails honestly when the platform refuses (missing
    /// display, permission, unsupported session, ...).
    pub fn new() -> Result<Self> {
        Self::from_driver(crate::native_driver()?)
    }

    /// Wrap an existing [`Driver`] (production alternative factory and the
    /// test seam for mock devices). Documented addition to the shared
    /// interface; see the contract report.
    pub fn from_driver(driver: Box<dyn Driver>) -> Result<Self> {
        Ok(Self {
            driver,
            held_keys: HashSet::new(),
            held_buttons: HashMap::new(),
        })
    }

    /// Move the mouse to absolute desktop coordinates (native input units).
    pub fn move_mouse(&mut self, x: i32, y: i32) -> Result<()> {
        self.driver.move_mouse(x, y)
    }

    /// Press or release a key. `Key::Character` must be ASCII alphanumeric.
    pub fn key(&mut self, key: Key, direction: Direction) -> Result<()> {
        if !key.is_valid() {
            return Err(InputError::new(
                "invalid_key",
                format!(
                    "Key::Character({:?}) is not a physical key; only ASCII letters/digits \
                     are supported for chords — use text_scalar for text",
                    match key {
                        Key::Character(c) => c.to_string(),
                        _ => unreachable!("only Character can be invalid"),
                    }
                ),
            ));
        }
        match direction {
            Direction::Press => {
                // Record BEFORE dispatch: a partial press must be released.
                self.held_keys.insert(key);
                self.driver.key(key, direction)
            }
            Direction::Release => {
                // Attempt first; forget only on success.
                self.driver.key(key, direction)?;
                self.held_keys.remove(&key);
                Ok(())
            }
        }
    }

    /// Press or release a mouse button. `click_count` is the native
    /// click-state of THIS press/up pair (valid 1..=3); invalid values are
    /// rejected with `invalid_button` before any dispatch. Releases are
    /// dispatched with the caller-supplied `click_count`; the remembered
    /// press metadata is used by [`Input::release_all`] and `Drop`.
    pub fn button(&mut self, button: Button, direction: Direction, click_count: u8) -> Result<()> {
        if !crate::is_valid_click_count(click_count) {
            return Err(InputError::new(
                "invalid_button",
                format!("click_count {click_count} is out of range 1..=3"),
            ));
        }
        match direction {
            Direction::Press => {
                // Record BEFORE dispatch: a partial press must be released.
                // Re-pressing a held button overwrites its last metadata.
                self.held_buttons.insert(button, click_count);
                self.driver.button(button, direction, click_count)
            }
            Direction::Release => {
                // Attempt first; forget only on success.
                self.driver.button(button, direction, click_count)?;
                self.held_buttons.remove(&button);
                Ok(())
            }
        }
    }

    /// Scroll `length` wheel ticks; positive = right (horizontal) / down
    /// (vertical).
    pub fn scroll(&mut self, length: i32, axis: Axis) -> Result<()> {
        self.driver.scroll(length, axis)
    }

    /// Inject one Unicode scalar as text. `'\n'` and `'\r'` each produce ONE
    /// Return click (CRLF is normalized by the caller before scalars reach
    /// here); `'\t'` produces ONE Tab click. NUL is rejected. Return/Tab
    /// clicks go through the same held-state machine as explicit keys.
    pub fn text_scalar(&mut self, ch: char) -> Result<()> {
        if ch == '\0' {
            return Err(InputError::new(
                "invalid_text",
                "text contains a NUL character",
            ));
        }
        match ch {
            '\n' | '\r' => self.click_key(Key::Return),
            '\t' => self.click_key(Key::Tab),
            _ => self.driver.text_scalar(ch),
        }
    }

    /// One key click (press + release) through the held-state machine: the
    /// press is tracked before dispatch and removed only after BOTH halves
    /// succeed, so a failed press or failed release is still cleaned up.
    fn click_key(&mut self, key: Key) -> Result<()> {
        self.held_keys.insert(key);
        self.driver.key(key, Direction::Press)?;
        self.driver.key(key, Direction::Release)?;
        self.held_keys.remove(&key);
        Ok(())
    }

    /// Best-effort release of every item this session still holds. Every
    /// item is attempted even after failures; successfully released items
    /// are forgotten, failed ones stay tracked (a later `release_all`
    /// genuinely retries them) and are reported in one aggregated error.
    /// Buttons are released with their remembered last-press click-state.
    pub fn release_all(&mut self) -> Result<()> {
        let mut keys: Vec<Key> = self.held_keys.iter().copied().collect();
        keys.sort_by_key(|k| k.name());
        // Buttons are stored only in the metadata map, never in the key set.
        // Release buttons first, then keys, in deterministic order.
        let mut buttons: Vec<(Button, u8)> =
            self.held_buttons.iter().map(|(b, c)| (*b, *c)).collect();
        buttons.sort_by_key(|(b, _)| format!("{b:?}"));
        let mut failures: Vec<String> = Vec::new();
        for (button, count) in &buttons {
            match self.driver.button(*button, Direction::Release, *count) {
                Ok(()) => {
                    self.held_buttons.remove(button);
                }
                Err(e) => {
                    failures.push(format!("button({button:?}): {}", e.message));
                }
            }
        }
        for key in keys {
            match self.driver.key(key, Direction::Release) {
                Ok(()) => {
                    self.held_keys.remove(&key);
                }
                Err(e) => {
                    failures.push(format!("key({}): {}", key.name(), e.message));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(InputError::new(
                "input_failed",
                format!(
                    "release_all: {} item(s) failed to release (still tracked, will be retried): {}",
                    failures.len(),
                    failures.join("; ")
                ),
            ))
        }
    }

    /// Currently tracked held items in deterministic order (tests).
    #[cfg(test)]
    fn held_snapshot(&self) -> Vec<(bool, String)> {
        let mut items: Vec<(bool, String)> = self
            .held_keys
            .iter()
            .map(|k| (false, HeldItem::Key(*k).name()))
            .collect();
        for (b, _c) in &self.held_buttons {
            items.push((true, HeldItem::Button(*b).name()));
        }
        items.sort();
        items
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        // Best-effort, intentionally silent: errors are only reportable
        // through the explicit `release_all` path.
        let mut buttons: Vec<(Button, u8)> =
            self.held_buttons.iter().map(|(b, c)| (*b, *c)).collect();
        buttons.sort_by_key(|(b, _)| format!("{b:?}"));
        for (button, count) in buttons {
            let _ = self.driver.button(button, Direction::Release, count);
        }
        let mut keys: Vec<Key> = self.held_keys.iter().copied().collect();
        keys.sort_by_key(|k| k.name());
        for key in keys {
            let _ = self.driver.key(key, Direction::Release);
        }
        self.held_keys.clear();
        self.held_buttons.clear();
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests driven through a recording mock `Driver`. No real
    //! desktop input is performed on any platform.

    use super::*;
    use crate::types::{Axis, Button, Direction, Driver, InputError, Key, Result};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Move(i32, i32),
        Button(Button, Direction, u8),
        Key(Key, Direction),
        Scroll(i32, Axis),
        Scalar(char),
    }

    /// Recording mock driver with injectable press/release failures.
    struct MockDriver {
        calls: Rc<RefCell<Vec<Call>>>,
        fail_press: Option<(u8, String)>, // (is_button, name) of the item
        fail_release: HashSet<(u8, String)>,
    }

    fn spec_of(item: &HeldItem) -> (u8, String) {
        (item.sort_key().0, item.sort_key().1)
    }

    impl MockDriver {
        fn new() -> (Self, Rc<RefCell<Vec<Call>>>) {
            let calls = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    calls: Rc::clone(&calls),
                    fail_press: None,
                    fail_release: HashSet::new(),
                },
                calls,
            )
        }
    }

    impl Driver for MockDriver {
        fn move_mouse(&mut self, x: i32, y: i32) -> Result<()> {
            self.calls.borrow_mut().push(Call::Move(x, y));
            Ok(())
        }
        fn button(&mut self, button: Button, direction: Direction, click_count: u8) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(Call::Button(button, direction, click_count));
            let item = HeldItem::Button(button);
            self.maybe_fail(&item, direction)
        }
        fn key(&mut self, key: Key, direction: Direction) -> Result<()> {
            self.calls.borrow_mut().push(Call::Key(key, direction));
            let item = HeldItem::Key(key);
            self.maybe_fail(&item, direction)
        }
        fn scroll(&mut self, length: i32, axis: Axis) -> Result<()> {
            self.calls.borrow_mut().push(Call::Scroll(length, axis));
            Ok(())
        }
        fn text_scalar(&mut self, ch: char) -> Result<()> {
            self.calls.borrow_mut().push(Call::Scalar(ch));
            Ok(())
        }
    }

    impl MockDriver {
        fn maybe_fail(&mut self, item: &HeldItem, direction: Direction) -> Result<()> {
            let spec = spec_of(item);
            match direction {
                Direction::Release if self.fail_release.contains(&spec) => Err(InputError::new(
                    "input_failed",
                    format!("mock release failure for {}", item.name()),
                )),
                Direction::Press if self.fail_press.as_ref() == Some(&spec) => {
                    self.fail_press = None;
                    Err(InputError::new(
                        "input_failed",
                        format!("mock press failure for {}", item.name()),
                    ))
                }
                _ => Ok(()),
            }
        }

        fn heal_release(&mut self, item: &HeldItem) {
            self.fail_release.remove(&spec_of(item));
        }
    }

    fn input() -> (Input, Rc<RefCell<Vec<Call>>>, Rc<RefCell<MockDriver>>) {
        let (mock, calls) = MockDriver::new();
        let mock = Rc::new(RefCell::new(mock));
        let input = Input::from_driver(Box::new(MockProxy(Rc::clone(&mock)))).unwrap();
        (input, calls, mock)
    }

    /// `Box<dyn Driver>` needs shared mutable access to the mock's failure
    /// configuration from tests; this proxy forwards to the shared mock.
    struct MockProxy(Rc<RefCell<MockDriver>>);

    impl Driver for MockProxy {
        fn move_mouse(&mut self, x: i32, y: i32) -> Result<()> {
            self.0.borrow_mut().move_mouse(x, y)
        }
        fn button(&mut self, button: Button, direction: Direction, click_count: u8) -> Result<()> {
            self.0.borrow_mut().button(button, direction, click_count)
        }
        fn key(&mut self, key: Key, direction: Direction) -> Result<()> {
            self.0.borrow_mut().key(key, direction)
        }
        fn scroll(&mut self, length: i32, axis: Axis) -> Result<()> {
            self.0.borrow_mut().scroll(length, axis)
        }
        fn text_scalar(&mut self, ch: char) -> Result<()> {
            self.0.borrow_mut().text_scalar(ch)
        }
    }

    // ---------------------------------------------------------------
    // Validation before any dispatch
    // ---------------------------------------------------------------

    #[test]
    fn invalid_character_key_rejected_with_no_effect() {
        let (mut input, calls, _mock) = input();
        let err = input
            .key(Key::Character('é'), Direction::Press)
            .unwrap_err();
        assert_eq!(err.code, "invalid_key");
        let err = input
            .key(Key::Character('\0'), Direction::Press)
            .unwrap_err();
        assert_eq!(err.code, "invalid_key");
        assert!(
            calls.borrow().is_empty(),
            "validation failures must not emit any input"
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn nul_text_scalar_rejected_with_no_effect() {
        let (mut input, calls, _mock) = input();
        let err = input.text_scalar('\0').unwrap_err();
        assert_eq!(err.code, "invalid_text");
        assert!(calls.borrow().is_empty());
    }

    // ---------------------------------------------------------------
    // Pass-through dispatch
    // ---------------------------------------------------------------

    #[test]
    fn move_scroll_and_scalar_pass_through() {
        let (mut input, calls, _mock) = input();
        input.move_mouse(100, -20).unwrap();
        input.scroll(3, Axis::Horizontal).unwrap();
        input.scroll(-2, Axis::Vertical).unwrap();
        input.text_scalar('世').unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Move(100, -20),
                Call::Scroll(3, Axis::Horizontal),
                Call::Scroll(-2, Axis::Vertical),
                Call::Scalar('世'),
            ]
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn newline_scalar_is_exactly_one_return_click() {
        let (mut input, calls, _mock) = input();
        input.text_scalar('\n').unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Key(Key::Return, Direction::Press),
                Call::Key(Key::Return, Direction::Release),
            ],
            "'\\n' must produce ONE Return click (press+release), never a \
             double Return or a Unicode scalar injection"
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn carriage_return_scalar_is_one_return_click_and_tab_is_one_tab_click() {
        let (mut input, calls, _mock) = input();
        input.text_scalar('\r').unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Key(Key::Return, Direction::Press),
                Call::Key(Key::Return, Direction::Release),
            ]
        );
        calls.borrow_mut().clear();
        input.text_scalar('\t').unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Key(Key::Tab, Direction::Press),
                Call::Key(Key::Tab, Direction::Release),
            ]
        );
    }

    // ---------------------------------------------------------------
    // Held-state: press tracking
    // ---------------------------------------------------------------

    #[test]
    fn successful_press_and_release_update_tracking() {
        let (mut input, calls, _mock) = input();
        input.key(Key::Control, Direction::Press).unwrap();
        assert_eq!(
            input.held_snapshot(),
            vec![(false, "key(control)".to_string())]
        );
        input.key(Key::Control, Direction::Release).unwrap();
        assert!(input.held_snapshot().is_empty());
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Key(Key::Control, Direction::Press),
                Call::Key(Key::Control, Direction::Release),
            ]
        );

        input.button(Button::Left, Direction::Press, 1).unwrap();
        assert_eq!(
            input.held_snapshot(),
            vec![(true, "button(Left)".to_string())]
        );
        input.button(Button::Left, Direction::Release, 1).unwrap();
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn failed_press_still_tracks_item_for_later_release() {
        let (mut input, _calls, mock) = input();
        mock.borrow_mut().fail_press = Some(spec_of(&HeldItem::Key(Key::Control)));
        let err = input.key(Key::Control, Direction::Press).unwrap_err();
        assert_eq!(err.code, "input_failed");
        // Partial delivery is possible: the item MUST remain held.
        assert_eq!(
            input.held_snapshot(),
            vec![(false, "key(control)".to_string())]
        );
        // And release_all releases it.
        input.release_all().unwrap();
        assert!(input.held_snapshot().is_empty());
    }

    // ---------------------------------------------------------------
    // Atomic multi-click metadata
    // ---------------------------------------------------------------

    #[test]
    fn click_count_metadata_passes_through_to_driver() {
        let (mut input, calls, _mock) = input();
        for count in 1..=3u8 {
            input.button(Button::Left, Direction::Press, count).unwrap();
            input
                .button(Button::Left, Direction::Release, count)
                .unwrap();
        }
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Left, Direction::Press, 1),
                Call::Button(Button::Left, Direction::Release, 1),
                Call::Button(Button::Left, Direction::Press, 2),
                Call::Button(Button::Left, Direction::Release, 2),
                Call::Button(Button::Left, Direction::Press, 3),
                Call::Button(Button::Left, Direction::Release, 3),
            ],
            "each press/up pair must reach the driver with ITS click-state"
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn invalid_click_count_rejected_before_any_dispatch() {
        let (mut input, calls, _mock) = input();
        for bad in [0u8, 4, 255] {
            let err = input
                .button(Button::Left, Direction::Press, bad)
                .unwrap_err();
            assert_eq!(err.code, "invalid_button");
            let err = input
                .button(Button::Left, Direction::Release, bad)
                .unwrap_err();
            assert_eq!(err.code, "invalid_button");
        }
        assert!(
            calls.borrow().is_empty(),
            "invalid counts must produce ZERO driver events"
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn repeated_press_overwrites_metadata_release_all_releases_once() {
        // A failed release keeps the button tracked; a later press with a
        // different click-state overwrites the remembered metadata instead
        // of accumulating duplicates, so release_all dispatches exactly one
        // release with the LAST metadata.
        let (mut input, calls, mock) = input();
        mock.borrow_mut()
            .fail_release
            .insert(spec_of(&HeldItem::Button(Button::Left)));
        input.button(Button::Left, Direction::Press, 1).unwrap();
        let _ = input.button(Button::Left, Direction::Release, 1); // fails
        input.button(Button::Left, Direction::Press, 2).unwrap();
        mock.borrow_mut()
            .heal_release(&HeldItem::Button(Button::Left));
        calls.borrow_mut().clear();
        input.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Left, Direction::Release, 2)],
            "release_all uses the last press metadata, exactly once"
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn failed_press_metadata_still_tracked_for_release_all() {
        let (mut input, calls, mock) = input();
        mock.borrow_mut().fail_press = Some(spec_of(&HeldItem::Button(Button::Left)));
        let err = input.button(Button::Left, Direction::Press, 2).unwrap_err();
        assert_eq!(err.code, "input_failed");
        assert_eq!(
            input.held_snapshot(),
            vec![(true, "button(Left)".to_string())]
        );
        input.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Left, Direction::Press, 2),
                Call::Button(Button::Left, Direction::Release, 2),
            ],
            "uncertain press is tracked with its metadata and cleaned up"
        );
    }

    #[test]
    fn drop_releases_held_buttons_with_remembered_metadata() {
        let (input, calls, _mock) = {
            let (mock, calls) = MockDriver::new();
            let mock = Rc::new(RefCell::new(mock));
            let input = Input::from_driver(Box::new(MockProxy(Rc::clone(&mock)))).unwrap();
            (input, calls, mock)
        };
        let mut input = input;
        input.button(Button::Right, Direction::Press, 3).unwrap();
        input.key(Key::Shift, Direction::Press).unwrap();
        calls.borrow_mut().clear();
        drop(input);
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Right, Direction::Release, 3),
                Call::Key(Key::Shift, Direction::Release),
            ],
            "Drop must best-effort release buttons with their press metadata"
        );
    }

    // ---------------------------------------------------------------
    // Held-state: release retains on failure
    // ---------------------------------------------------------------

    #[test]
    fn failed_release_keeps_item_tracked_and_is_retried() {
        let (mut input, calls, mock) = input();
        input.button(Button::Left, Direction::Press, 1).unwrap();
        mock.borrow_mut()
            .fail_release
            .insert(spec_of(&HeldItem::Button(Button::Left)));

        let err = input
            .button(Button::Left, Direction::Release, 1)
            .unwrap_err();
        assert_eq!(err.code, "input_failed");
        assert_eq!(
            input.held_snapshot(),
            vec![(true, "button(Left)".to_string())],
            "a failed release must NOT forget the held item"
        );

        // Retry after the device recovers: a real release is dispatched.
        mock.borrow_mut()
            .heal_release(&HeldItem::Button(Button::Left));
        input.button(Button::Left, Direction::Release, 1).unwrap();
        assert!(input.held_snapshot().is_empty());
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Left, Direction::Press, 1),
                Call::Button(Button::Left, Direction::Release, 1),
                Call::Button(Button::Left, Direction::Release, 1),
            ]
        );
    }

    #[test]
    fn release_all_attempts_every_held_item_and_retains_failures() {
        let (mut input, calls, mock) = input();
        input.key(Key::Control, Direction::Press).unwrap();
        input.key(Key::Shift, Direction::Press).unwrap();
        input.button(Button::Left, Direction::Press, 1).unwrap();

        // One release fails; release_all must still try every item.
        let shift = HeldItem::Key(Key::Shift);
        mock.borrow_mut().fail_release.insert(spec_of(&shift));
        calls.borrow_mut().clear();
        let err = input.release_all().unwrap_err();
        assert_eq!(err.code, "input_failed");
        assert!(
            err.message.contains("shift"),
            "error names the failed item: {}",
            err.message
        );
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Left, Direction::Release, 1),
                Call::Key(Key::Control, Direction::Release),
                Call::Key(Key::Shift, Direction::Release),
            ],
            "buttons release first, then keys by name"
        );
        assert_eq!(
            input.held_snapshot(),
            vec![(false, "key(shift)".to_string())]
        );

        // A later retry MUST actually attempt the failed item again.
        calls.borrow_mut().clear();
        mock.borrow_mut().heal_release(&shift);
        input.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Key(Key::Shift, Direction::Release)]
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn release_all_with_empty_held_set_is_noop_success() {
        let (mut input, calls, _mock) = input();
        input.release_all().unwrap();
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn failed_return_click_keeps_return_held_for_cleanup() {
        let (mut input, calls, mock) = input();
        // The release half of the Return click fails: Return must stay held
        // so release_all cleans it up instead of leaving a stuck modifier.
        mock.borrow_mut()
            .fail_release
            .insert(spec_of(&HeldItem::Key(Key::Return)));
        let err = input.text_scalar('\n').unwrap_err();
        assert_eq!(err.code, "input_failed");
        assert_eq!(
            input.held_snapshot(),
            vec![(false, "key(return)".to_string())]
        );
        calls.borrow_mut().clear();
        mock.borrow_mut().heal_release(&HeldItem::Key(Key::Return));
        input.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Key(Key::Return, Direction::Release)]
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn only_owned_items_are_released() {
        // The session releases exactly what IT tracked: a key the driver was
        // told to release externally (never pressed via Input) is not in the
        // held set, so release_all dispatches nothing for it.
        let (mut input, calls, mock) = input();
        mock.borrow_mut()
            .fail_release
            .insert(spec_of(&HeldItem::Key(Key::Escape)));
        input.key(Key::Escape, Direction::Press).unwrap();
        // Simulate an external release the session did not observe as a
        // successful Press/Release pair: a FAILED release keeps it tracked,
        // so instead verify an item that never existed is untouched.
        mock.borrow_mut()
            .fail_release
            .remove(&spec_of(&HeldItem::Key(Key::Escape)));
        input.key(Key::Escape, Direction::Release).unwrap();
        input.key(Key::Tab, Direction::Press).unwrap();
        calls.borrow_mut().clear();
        input.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Key(Key::Tab, Direction::Release)],
            "release_all must dispatch releases ONLY for session-held items"
        );
    }
    #[test]
    fn cleanup_failure_retries_same_metadata_and_attempts_all_other_items() {
        let (mut input, calls, mock) = input();
        let left = HeldItem::Button(Button::Left);
        mock.borrow_mut().fail_press = Some(spec_of(&left));
        assert!(input.button(Button::Left, Direction::Press, 2).is_err());
        input.button(Button::Right, Direction::Press, 3).unwrap();
        input.key(Key::Shift, Direction::Press).unwrap();
        mock.borrow_mut().fail_release.insert(spec_of(&left));
        calls.borrow_mut().clear();
        assert!(input.release_all().is_err());
        assert_eq!(
            *calls.borrow(),
            vec![
                Call::Button(Button::Left, Direction::Release, 2),
                Call::Button(Button::Right, Direction::Release, 3),
                Call::Key(Key::Shift, Direction::Release),
            ]
        );
        calls.borrow_mut().clear();
        assert!(input.release_all().is_err());
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Left, Direction::Release, 2)]
        );
        mock.borrow_mut().heal_release(&left);
        calls.borrow_mut().clear();
        input.release_all().unwrap();
        input.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Left, Direction::Release, 2)]
        );
        assert!(input.held_snapshot().is_empty());
    }

    #[test]
    fn invalid_release_preserves_held_metadata_and_drop_retries_cleanup_failure() {
        let (mut input, calls, mock) = input();
        let button = HeldItem::Button(Button::Middle);
        input.button(Button::Middle, Direction::Press, 3).unwrap();
        calls.borrow_mut().clear();
        assert!(input.button(Button::Middle, Direction::Release, 4).is_err());
        assert!(calls.borrow().is_empty());
        mock.borrow_mut().fail_release.insert(spec_of(&button));
        assert!(input.release_all().is_err());
        mock.borrow_mut().heal_release(&button);
        calls.borrow_mut().clear();
        drop(input);
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Middle, Direction::Release, 3)]
        );
    }

    #[test]
    fn last_uncertain_repeated_press_overwrites_metadata_without_duplicate_held_entries() {
        let (mut input, calls, mock) = input();
        input.button(Button::Left, Direction::Press, 1).unwrap();
        input.button(Button::Left, Direction::Press, 2).unwrap();
        mock.borrow_mut().fail_press = Some(spec_of(&HeldItem::Button(Button::Left)));
        assert!(input.button(Button::Left, Direction::Press, 3).is_err());
        assert_eq!(input.held_snapshot(), vec![(true, "button(Left)".into())]);
        calls.borrow_mut().clear();
        input.release_all().unwrap();
        assert_eq!(
            *calls.borrow(),
            vec![Call::Button(Button::Left, Direction::Release, 3)]
        );
        assert!(input.held_snapshot().is_empty());
    }
}
