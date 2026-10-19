use super::super::policy::{Adapter, Checkpoint, Observation, SignalResult, Spawn};
use crate::x11_credentials::{ipc::AuthorityRead, FailureKind, Request};
use std::time::Instant;

pub(in crate::x11_credentials::launcher) struct Handle;
pub(in crate::x11_credentials::launcher) struct Mask;
pub(in crate::x11_credentials::launcher) struct Native;
impl Adapter for Native {
    type Handle = Handle;
    type Mask = Mask;
    fn prepare(&mut self, _: &Request<'_>) -> Result<(), FailureKind> {
        Err(FailureKind::LauncherUnavailable)
    }
    fn checkpoint(&mut self, _: Checkpoint, deadline: Instant) -> Result<(), FailureKind> {
        if Instant::now() >= deadline {
            Err(FailureKind::Timeout)
        } else {
            Ok(())
        }
    }
    fn block_signals(&mut self) -> Result<Mask, FailureKind> {
        Err(FailureKind::LauncherUnavailable)
    }
    fn restore_signals(&mut self, _: Mask) -> Result<(), FailureKind> {
        Err(FailureKind::LauncherUnavailable)
    }
    fn clone3(&mut self) -> Spawn<Handle> {
        Spawn::Unavailable
    }
    fn legacy_clone_pidfd(&mut self) -> Spawn<Handle> {
        Spawn::Unavailable
    }
    fn validate_pidfd(&mut self, _: &Handle) -> Result<(), FailureKind> {
        Err(FailureKind::LauncherUnavailable)
    }
    fn open_startup_gate(&mut self) -> Result<(), FailureKind> {
        Err(FailureKind::LauncherUnavailable)
    }
    fn revoke_ipc(&mut self) {}
    fn receive(&mut self, _: Instant) -> Result<AuthorityRead, FailureKind> {
        Err(FailureKind::LauncherUnavailable)
    }
    fn kill_pidfd(&mut self, _: &Handle) -> SignalResult {
        SignalResult::Failed
    }
    fn observe(&mut self, _: &Handle) -> Observation {
        Observation::Failed
    }
}

impl Native {
    pub fn new() -> Self {
        Self
    }
}
