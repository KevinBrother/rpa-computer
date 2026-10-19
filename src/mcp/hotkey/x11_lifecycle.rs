//! A single lock linearizes readiness acceptance against expiry/failure.
use super::HotkeyStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[derive(Default)]
enum Phase {
    #[default]
    Initializing,
    Active,
    Failed,
}

#[derive(Default)]
pub(super) struct Control {
    pub stop: AtomicBool,
    /// Mirror for worker polling/tests only, never used to decide transitions.
    pub active: AtomicBool,
    pub failure: OnceLock<HotkeyStatus>,
    phase: Mutex<Phase>,
}

impl Control {
    pub fn accept_ready(&self, deadline: Instant) -> bool {
        self.accept_ready_at(deadline, Instant::now(), || {})
    }

    pub(super) fn accept_ready_at(
        &self,
        deadline: Instant,
        now: Instant,
        before_activation: impl FnOnce(),
    ) -> bool {
        // Reject an already-expired response before entering the test checkpoint.
        // Recheck under the lock as well: expiry/failure may win while paused.
        if now >= deadline || Instant::now() >= deadline {
            return false;
        }
        before_activation(); // Test checkpoint is BEFORE the atomic transition.
        let mut phase = self.phase.lock().unwrap_or_else(|e| e.into_inner());
        if now >= deadline
            || Instant::now() >= deadline
            || self.stop.load(Ordering::SeqCst)
            || self.failure.get().is_some()
            || !matches!(*phase, Phase::Initializing)
        {
            return false;
        }
        *phase = Phase::Active;
        self.active.store(true, Ordering::SeqCst);
        true
    }

    /// Publish failure before any cleanup. Return whether protection was active.
    pub fn begin_failed_cleanup(&self, error: &str, cancel: &AtomicBool) -> bool {
        let mut phase = self.phase.lock().unwrap_or_else(|e| e.into_inner());
        let active = matches!(*phase, Phase::Active);
        *phase = Phase::Failed;
        let _ = self.failure.set(HotkeyStatus::Failed(error.into()));
        self.active.store(false, Ordering::SeqCst);
        if active && !self.stop.load(Ordering::SeqCst) {
            cancel.store(true, Ordering::SeqCst);
        }
        active
    }
}
