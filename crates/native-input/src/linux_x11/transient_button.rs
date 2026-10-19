//! Production transient-wheel algorithm with an injectable native-send seam.
//! Record before dispatch: a failed transport call may already have effects.
use crate::{InputError, Result};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct TransientButtons {
    pending: BTreeSet<u32>,
}
impl TransientButtons {
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    #[cfg(test)]
    pub fn pending_ids(&self) -> Vec<u32> {
        self.pending.iter().copied().collect()
    }

    pub fn click(
        &mut self,
        button: u32,
        mut send: impl FnMut(u32, bool) -> Result<()>,
    ) -> Result<()> {
        if self.has_pending() {
            return Err(InputError::new(
                "input_failed",
                "wheel release pending; cleanup required before another tick",
            ));
        }
        self.pending.insert(button);
        let mut failures = Vec::new();
        if let Err(error) = send(button, true) {
            failures.push(error.to_string());
        }
        match send(button, false) {
            Ok(()) => {
                self.pending.remove(&button);
            }
            Err(first) => {
                failures.push(first.to_string());
                match send(button, false) {
                    Ok(()) => {
                        self.pending.remove(&button);
                    }
                    Err(retry) => failures.push(format!("owned release retry: {retry}")),
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(InputError::new(
                "input_failed",
                format!(
                    "wheel tick partially accepted or acceptance unknown; do not blindly retry: {}",
                    failures.join("; ")
                ),
            ))
        }
    }

    pub fn cleanup(&mut self, mut send: impl FnMut(u32, bool) -> Result<()>) -> Result<()> {
        let mut failures = Vec::new();
        for button in self.pending.iter().copied().collect::<Vec<_>>() {
            match send(button, false) {
                Ok(()) => {
                    self.pending.remove(&button);
                }
                Err(error) => failures.push(format!("wheel button {button}: {error}")),
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(InputError::new("input_failed", failures.join("; ")))
        }
    }
}

#[cfg(test)]
#[path = "transient_button_tests.rs"]
mod transient_button_tests;
