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

use std::collections::HashSet;

use super::dispatch::{Axis, NativeInput};
use super::keys;
use super::{BackendError, Direction, HeldItem, InputEvent};

pub(crate) struct BackendCore<I: NativeInput> {
    input: I,
    /// Keys/buttons pressed and not yet successfully released.
    held: HashSet<HeldItem>,
}

impl<I: NativeInput> BackendCore<I> {
    /// Create the state machine around a native input dispatcher.
    pub(crate) fn new(input: I) -> Self {
        Self {
            input,
            held: HashSet::new(),
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
        items.sort_by_key(|i| i.sort_key());
        items
    }

    /// Best-effort release of one held item, WITHOUT changing tracking:
    /// the caller decides whether to keep (failure) or drop (success) it.
    fn release_item(&mut self, item: HeldItem) -> Result<(), BackendError> {
        match item {
            HeldItem::Key(k) => self
                .input
                .key(k.enigo_key(), Direction::Release)
                .map_err(|e| {
                    BackendError::new(
                        "input_failed",
                        format!("failed to release {}: {}", k.name(), e.message),
                    )
                }),
            HeldItem::Button(b) => self.input.button(b, Direction::Release).map_err(|e| {
                BackendError::new(
                    "input_failed",
                    format!("failed to release {b:?}: {}", e.message),
                )
            }),
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
            InputEvent::Button { button, direction } => {
                let parsed = keys::parse_button(button)?; // validate before any input
                let item = HeldItem::Button(parsed);
                match direction {
                    Direction::Press => {
                        self.held.insert(item);
                        self.input.button(parsed, Direction::Press)?;
                    }
                    Direction::Release => {
                        // Attempt first; forget only on success.
                        self.input.button(parsed, Direction::Release)?;
                        self.held.remove(&item);
                    }
                }
            }
            InputEvent::Key { key, direction } => {
                let parsed = keys::parse_key(key)?; // validate before any input
                let item = HeldItem::Key(parsed);
                match direction {
                    Direction::Press => {
                        self.held.insert(item);
                        self.input.key(parsed.enigo_key(), Direction::Press)?;
                    }
                    Direction::Release => {
                        // Attempt first; forget only on success.
                        self.input.key(parsed.enigo_key(), Direction::Release)?;
                        self.held.remove(&item);
                    }
                }
            }
            InputEvent::Text { text } => {
                keys::validate_text(text)?;
                self.input.text(text)?;
            }
            InputEvent::Scroll { x, y } => {
                // Contract semantic (positive = right/down) is EXACTLY the
                // enigo 0.3 `Mouse::scroll` API semantic ("positive length
                // will result in scrolling down ... to the right"), so the
                // deltas pass through unchanged. Enigo performs any native
                // wheel-delta sign handling per platform internally; negating
                // here would invert the user's scroll direction.
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
        let mut failures: Vec<String> = Vec::new();
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
        // through the explicit `release_all` path. Enigo additionally
        // releases ITS tracked held keys on drop, but that is not a
        // substitute for these explicit attempts — it does not cover mouse
        // buttons and its outcome is not observable, so explicit cleanup
        // correctness must never be inferred from it.
        let mut items: Vec<HeldItem> = self.held.iter().copied().collect();
        items.sort_by_key(|i| i.sort_key());
        for item in items {
            let _ = self.release_item(item);
        }
        self.held.clear();
    }
}
