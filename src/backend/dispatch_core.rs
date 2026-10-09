//! [`BackendCore`]: the input state machine (held-item tracking, release
//! semantics, event translation) generic over the [`NativeInput`] dispatch
//! seam, so the full input lifecycle is regression-tested with a recording
//! mock — without touching a real desktop.
//!
//! Semantics enforced here (all covered by `dispatch::tests`):
//! - **Press**: the item is recorded as held BEFORE the press call, so a
//!   partially delivered (error-returning) press is still released later.
//! - **Release**: the release call is attempted FIRST; the item is removed
//!   from tracking ONLY on success. A failed release keeps the item tracked
//!   so a later release or `release_all` genuinely retries it instead of
//!   falsely reporting a clean state.
//! - **release_all**: attempts EVERY held item even after failures; only
//!   successfully released items are forgotten; failures stay tracked and
//!   are reported in one aggregated `input_failed` error.
//! - **Scroll**: contract deltas pass through to the dispatcher unchanged.
//! - **Text**: per Unicode scalar. NUL is rejected before ANY dispatch. CRLF
//!   is normalized once (a `"\r\n"` pair produces exactly ONE Return click);
//!   each remaining `'\n'`/`'\r'` produces ONE Return click and `'\t'` ONE
//!   Tab click — through the same held-state machine as explicit keys. Every
//!   other scalar goes to the native text path. Text is never clipboard.

use std::collections::{HashMap, HashSet};

use super::dispatch::{Axis, NativeInput};
use super::keys;
use super::{BackendError, Direction, HeldItem, InputEvent};
use rpa_native_input::Key;

pub(crate) struct BackendCore<I: NativeInput> {
    input: I,
    /// Keys pressed and not yet successfully released.
    held: HashSet<HeldItem>,
    /// Buttons pressed and not yet successfully released, with the LAST
    /// press metadata (native click-state 1..=3). Re-pressing a held button
    /// overwrites the metadata instead of accumulating duplicate entries,
    /// so release_all/Drop release each held button exactly once with its
    /// matching click-state.
    held_buttons: HashMap<rpa_native_input::Button, u8>,
}

impl<I: NativeInput> BackendCore<I> {
    /// Create the state machine around a native input dispatcher.
    pub(crate) fn new(input: I) -> Self {
        Self {
            input,
            held: HashSet::new(),
            held_buttons: HashMap::new(),
        }
    }

    /// Mutable access to the dispatcher (tests only; production never needs
    /// to bypass the state machine).
    #[cfg(test)]
    pub(crate) fn input_mut(&mut self) -> &mut I {
        &mut self.input
    }

    /// Currently tracked held items in deterministic order (tests).
    #[cfg(test)]
    pub(crate) fn held_snapshot(&self) -> Vec<HeldItem> {
        let mut items: Vec<HeldItem> = self.held.iter().copied().collect();
        for (b, _c) in &self.held_buttons {
            items.push(HeldItem::Button(*b));
        }
        items.sort_by_key(|i| i.sort_key());
        items
    }

    /// Best-effort release of one held item, WITHOUT changing tracking:
    /// the caller decides whether to keep (failure) or drop (success) it.
    /// Buttons are released with their remembered last-press click-state.
    fn release_item(&mut self, item: HeldItem) -> Result<(), BackendError> {
        match item {
            HeldItem::Key(k) => {
                let name = k.name();
                self.input.key(k.key(), Direction::Release).map_err(|e| {
                    BackendError::new(
                        "input_failed",
                        format!("failed to release {name}: {}", e.message),
                    )
                })
            }
            HeldItem::Button(b) => {
                let count = self.held_buttons.get(&b).copied().ok_or_else(|| {
                    BackendError::new("input_failed", format!("missing press metadata for {b:?}"))
                })?;
                self.input
                    .button(b, Direction::Release, count)
                    .map_err(|e| {
                        BackendError::new(
                            "input_failed",
                            format!("failed to release {b:?}: {}", e.message),
                        )
                    })
            }
        }
    }

    /// One key click (press + release) through the held-state machine: the
    /// press is tracked before dispatch and removed only after BOTH halves
    /// succeed, so a failed press or failed release is still cleaned up.
    fn click_key(&mut self, key: Key) -> Result<(), BackendError> {
        let item = HeldItem::Key(keys::ParsedKey::Named(key));
        self.held.insert(item);
        self.input.key(key, Direction::Press)?;
        self.input.key(key, Direction::Release)?;
        self.held.remove(&item);
        Ok(())
    }

    /// Inject one text scalar: `'\n'`/`'\r'` → ONE Return click, `'\t'` →
    /// ONE Tab click, everything else → native scalar text path.
    fn inject_text_scalar(&mut self, ch: char) -> Result<(), BackendError> {
        match ch {
            '\n' | '\r' => self.click_key(Key::Return),
            '\t' => self.click_key(Key::Tab),
            _ => self.input.text_scalar(ch),
        }
    }

    /// Execute one atomic input event, validating names BEFORE any dispatch
    /// and maintaining held-state per the module-level semantics.
    pub(crate) fn inject_event(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        match event {
            InputEvent::Move { x, y } => {
                // Coordinates are already in native backend input units
                // (mapped by the Runtime from capture dimensions); never
                // rescale here.
                self.input.move_mouse(*x, *y)?;
            }
            InputEvent::Button {
                button,
                direction,
                click_count,
            } => {
                // Validate name AND count before any native dispatch.
                let parsed = keys::parse_button(button)?;
                if !rpa_native_input::is_valid_click_count(*click_count) {
                    return Err(BackendError::new(
                        "invalid_button",
                        format!("click_count {click_count} is out of range 1..=3"),
                    ));
                }
                match direction {
                    Direction::Press => {
                        // Record BEFORE dispatch: a partial press must be
                        // released. Re-pressing overwrites the metadata.
                        self.held_buttons.insert(parsed, *click_count);
                        self.input.button(parsed, Direction::Press, *click_count)?;
                    }
                    Direction::Release => {
                        // Attempt first; forget only on success.
                        self.input
                            .button(parsed, Direction::Release, *click_count)?;
                        self.held_buttons.remove(&parsed);
                    }
                }
            }
            InputEvent::Key { key, direction } => {
                let parsed = keys::parse_key(key)?; // validate before any input
                let item = HeldItem::Key(parsed);
                match direction {
                    Direction::Press => {
                        self.held.insert(item);
                        self.input.key(parsed.key(), Direction::Press)?;
                    }
                    Direction::Release => {
                        // Attempt first; forget only on success.
                        self.input.key(parsed.key(), Direction::Release)?;
                        self.held.remove(&item);
                    }
                }
            }
            InputEvent::Text { text } => {
                // Validate the WHOLE payload before any dispatch, then inject
                // scalar by scalar. CRLF is normalized once here so a
                // `"\r\n"` pair never produces two Return clicks.
                keys::validate_text(text)?;
                let mut chars = text.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '\r' && chars.peek() == Some(&'\n') {
                        chars.next(); // consume the pair; single Return follows
                    }
                    self.inject_text_scalar(ch)?;
                }
            }
            InputEvent::Scroll { x, y } => {
                // Contract semantic (positive = right/down) is EXACTLY the
                // driver's documented semantic, so the deltas pass through
                // unchanged. Native wheel-delta sign handling is a platform
                // detail inside the driver; negating here would invert the
                // user's scroll direction.
                if *x != 0 {
                    self.input.scroll(*x, Axis::Horizontal)?;
                }
                if *y != 0 {
                    self.input.scroll(*y, Axis::Vertical)?;
                }
            }
        }
        Ok(())
    }

    /// Best-effort release of every held item. Every item is attempted even
    /// after failures. Items whose release SUCCEEDS are forgotten; items
    /// whose release fails stay tracked (a later `release_all` genuinely
    /// retries them) and are reported in the aggregated error.
    pub(crate) fn release_all(&mut self) -> Result<(), BackendError> {
        // Deterministic attempt order for reproducible diagnostics.
        let mut items: Vec<HeldItem> = self.held.iter().copied().collect();
        items.sort_by_key(|i| i.sort_key());
        let mut buttons: Vec<rpa_native_input::Button> =
            self.held_buttons.keys().copied().collect();
        buttons.sort_by_key(|b| format!("{b:?}"));
        let mut failures: Vec<String> = Vec::new();
        for button in &buttons {
            match self.release_item(HeldItem::Button(*button)) {
                Ok(()) => {
                    self.held_buttons.remove(button);
                }
                Err(e) => {
                    failures.push(format!("button({button:?}): {}", e.message));
                }
            }
        }
        for item in items {
            match self.release_item(item) {
                Ok(()) => {
                    self.held.remove(&item);
                }
                Err(e) => {
                    // Keep the item tracked: it may still be physically held.
                    failures.push(format!("{item}: {}", e.message));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(BackendError::new(
                "input_failed",
                format!(
                    "release_all: {} item(s) failed to release (still tracked, will be retried): {}",
                    failures.len(),
                    failures.join("; ")
                ),
            ))
        }
    }
}

impl<I: NativeInput> Drop for BackendCore<I> {
    fn drop(&mut self) {
        // Best-effort, intentionally silent: errors are only reportable
        // through the explicit `release_all` path. Explicit cleanup
        // correctness must never be inferred from any driver-side drop
        // behavior — it does not cover mouse buttons and its outcome is not
        // observable.
        let mut buttons: Vec<rpa_native_input::Button> =
            self.held_buttons.keys().copied().collect();
        buttons.sort_by_key(|b| format!("{b:?}"));
        for button in buttons {
            let _ = self.release_item(HeldItem::Button(button));
        }
        let mut items: Vec<HeldItem> = self.held.iter().copied().collect();
        items.sort_by_key(|i| i.sort_key());
        for item in items {
            let _ = self.release_item(item);
        }
        self.held.clear();
        self.held_buttons.clear();
    }
}
