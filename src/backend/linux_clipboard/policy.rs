//! Payload and single-use authorization policy. No OS handles or clipboard reads.
use crate::backend::{keys, BackendError};
use std::time::{Duration, Instant};

pub(super) const MAX_BYTES: usize = 16_384;
#[cfg(target_os = "linux")]
pub(super) const RECEIPT_TTL: Duration = Duration::from_millis(250);

pub(super) fn error(message: impl Into<String>) -> BackendError {
    BackendError::new("input_failed", message)
}
pub(super) fn prepare_payload(text: &str, inline_budget: usize) -> Result<Vec<u8>, BackendError> {
    keys::validate_paste_text(text)?;
    if text.len() > MAX_BYTES || text.len() > inline_budget {
        return Err(error(
            "paste exceeds bounded inline X11 property capacity; clipboard unchanged",
        ));
    }
    Ok(text.as_bytes().to_vec())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct BlockedKeys {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    pub v: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GuardSnapshot {
    pub focus: u32,
    pub clipboard_owner: u32,
    pub owner_generation: u64,
    pub blocked: BlockedKeys,
}
impl GuardSnapshot {
    pub fn before_publish(&self) -> Result<(), BackendError> {
        if self.focus <= 1 {
            return Err(error(
                "paste requires a concrete focused X11 window; clipboard unchanged",
            ));
        }
        if self.blocked != BlockedKeys::default() {
            return Err(error(
                "paste requires released physical Ctrl/Shift/Alt/Meta/V; clipboard unchanged",
            ));
        }
        Ok(())
    }
    fn receipt_ready(&self) -> Result<(), BackendError> {
        self.before_publish()?;
        if self.clipboard_owner == 0 || self.owner_generation == 0 {
            return Err(error("paste owner identity/generation unavailable"));
        }
        Ok(())
    }
}
struct Receipt {
    snapshot: GuardSnapshot,
    deadline: Instant,
}
#[derive(Default)]
pub(super) struct ReceiptSlot {
    epoch: u64,
    exhausted: bool,
    receipt: Option<Receipt>,
}
impl ReceiptSlot {
    pub fn begin_prepare(&mut self) -> u64 {
        self.invalidate();
        self.epoch
    }
    pub fn invalidate(&mut self) {
        self.receipt = None;
        match self.epoch.checked_add(1) {
            Some(next) => self.epoch = next,
            None => self.exhausted = true,
        }
    }
    pub fn arm(
        &mut self,
        epoch: u64,
        snapshot: GuardSnapshot,
        now: Instant,
        ttl: Duration,
    ) -> Result<(), BackendError> {
        if self.exhausted || epoch == 0 || epoch != self.epoch {
            return Err(error("stale or cancelled paste preparation"));
        }
        self.receipt = None;
        snapshot.receipt_ready()?;
        if ttl.is_zero() {
            return Err(error("paste receipt expired"));
        }
        let deadline = now
            .checked_add(ttl)
            .ok_or_else(|| error("invalid paste receipt deadline"))?;
        self.receipt = Some(Receipt { snapshot, deadline });
        Ok(())
    }
    pub fn consume(&mut self, actual: GuardSnapshot, now: Instant) -> Result<(), BackendError> {
        let receipt = self
            .receipt
            .take()
            .ok_or_else(|| error("no prepared clipboard paste receipt"))?;
        if now >= receipt.deadline {
            return Err(error("paste receipt expired; clipboard already modified"));
        }
        actual
            .receipt_ready()
            .map_err(|_| error("paste focus/input/owner changed; clipboard already modified"))?;
        if actual != receipt.snapshot {
            return Err(error("paste focus, selection owner or content generation changed; clipboard already modified"));
        }
        Ok(())
    }
}
