//! Pure protocol supervision driven by a monotonic Host clock. No timers,
//! threads, streams, UI work or input injection are started by this module.
use crate::{
    protocol::{Ready, RendererMessage, Session},
    Diagnostic,
};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub ready: Duration,
    pub heartbeat: Duration,
}
impl Default for Timeouts {
    fn default() -> Self {
        Self {
            ready: Duration::from_secs(5),
            heartbeat: Duration::from_secs(5),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub diagnostic: Diagnostic,
    /// Trusted Host notification: revoke current control, cancel and clean up;
    /// also deny new control until an explicitly reconstructed healthy renderer.
    pub stop_required: bool,
    /// Explicit enabled startup must not succeed before actual Ready.
    pub startup_rejected: bool,
}
impl Failure {
    pub(crate) fn startup(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostic,
            stop_required: true,
            startup_rejected: true,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiveOutcome {
    Disabled,
    Ready,
    Heartbeat,
    StopBeforeReady,
    StopCandidate(Session),
    Failed(Failure),
    Shutdown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolPhase {
    Disabled,
    AwaitingReady,
    Ready,
    Failed,
    Shutdown,
}

pub struct ProtocolMachine {
    phase: ProtocolPhase,
    timeouts: Timeouts,
    deadline: Duration,
    last_now: Duration,
    capabilities: Option<Ready>,
    failure: Option<Failure>,
}
impl ProtocolMachine {
    pub fn disabled() -> Self {
        Self {
            phase: ProtocolPhase::Disabled,
            timeouts: Timeouts::default(),
            deadline: Duration::ZERO,
            last_now: Duration::ZERO,
            capabilities: None,
            failure: None,
        }
    }
    pub fn enabled(now: Duration, timeouts: Timeouts) -> Result<Self, Diagnostic> {
        if timeouts.ready.is_zero() || timeouts.heartbeat.is_zero() {
            return Err(Diagnostic::InvalidLimit);
        }
        let deadline = now
            .checked_add(timeouts.ready)
            .ok_or(Diagnostic::InvalidLimit)?;
        now.checked_add(timeouts.heartbeat)
            .ok_or(Diagnostic::InvalidLimit)?;
        Ok(Self {
            phase: ProtocolPhase::AwaitingReady,
            timeouts,
            deadline,
            last_now: now,
            capabilities: None,
            failure: None,
        })
    }
    pub fn phase(&self) -> ProtocolPhase {
        self.phase
    }
    pub fn can_accept_control(&self) -> bool {
        matches!(self.phase, ProtocolPhase::Disabled | ProtocolPhase::Ready)
    }
    /// `Requested` is only API-request acknowledgement, never proven exclusion.
    pub fn capabilities(&self) -> Option<&Ready> {
        if self.phase == ProtocolPhase::Ready {
            self.capabilities.as_ref()
        } else {
            None
        }
    }
    /// Sticky, O(1) notification: it cannot overflow or disappear behind frames.
    pub fn failure(&self) -> Option<Failure> {
        self.failure
    }
    pub fn tick(&mut self, now: Duration) -> Option<Failure> {
        if let Some(failure) = self.failure {
            return Some(failure);
        }
        if matches!(
            self.phase,
            ProtocolPhase::Disabled | ProtocolPhase::Shutdown
        ) {
            return None;
        }
        if now < self.last_now {
            return Some(self.fail(Diagnostic::ClockRegression));
        }
        self.last_now = now;
        if now >= self.deadline {
            let diagnostic = if self.phase == ProtocolPhase::AwaitingReady {
                Diagnostic::ReadyTimeout
            } else {
                Diagnostic::HeartbeatTimeout
            };
            return Some(self.fail(diagnostic));
        }
        None
    }
    pub fn receive(&mut self, now: Duration, message: RendererMessage) -> ReceiveOutcome {
        if self.phase == ProtocolPhase::Disabled {
            return ReceiveOutcome::Disabled;
        }
        if let Some(failure) = self.tick(now) {
            return ReceiveOutcome::Failed(failure);
        }
        if self.phase == ProtocolPhase::Shutdown {
            return ReceiveOutcome::Shutdown;
        }
        if let Err(error) = message.validate() {
            return ReceiveOutcome::Failed(self.fail(error));
        }
        match message {
            RendererMessage::Ready(ready) => {
                if self.phase == ProtocolPhase::Ready {
                    return ReceiveOutcome::Failed(self.fail(Diagnostic::DuplicateReady));
                }
                if !self.refresh_deadline(now) {
                    return ReceiveOutcome::Failed(self.failure.expect("overflow failure"));
                }
                self.capabilities = Some(ready);
                self.phase = ProtocolPhase::Ready;
                ReceiveOutcome::Ready
            }
            RendererMessage::Heartbeat(_) => {
                if self.phase != ProtocolPhase::Ready {
                    return ReceiveOutcome::Failed(self.fail(Diagnostic::UnexpectedMessage));
                }
                if !self.refresh_deadline(now) {
                    return ReceiveOutcome::Failed(self.failure.expect("overflow failure"));
                }
                ReceiveOutcome::Heartbeat
            }
            RendererMessage::Stop(stop) => {
                if self.phase != ProtocolPhase::Ready {
                    ReceiveOutcome::StopBeforeReady
                } else {
                    ReceiveOutcome::StopCandidate(stop.session)
                }
            }
            RendererMessage::Error(_) => {
                ReceiveOutcome::Failed(self.fail(Diagnostic::RendererError))
            }
        }
    }
    /// Protocol, write/read failure, process loss and control overflow converge on
    /// one sticky failure. No transport acknowledgement can clear it.
    pub fn fail(&mut self, diagnostic: Diagnostic) -> Failure {
        if let Some(failure) = self.failure {
            return failure;
        }
        let failure = Failure {
            diagnostic,
            stop_required: self.phase != ProtocolPhase::Disabled,
            startup_rejected: self.phase == ProtocolPhase::AwaitingReady,
        };
        // Disabled callers should not call fail; do not enable or break off mode.
        if self.phase != ProtocolPhase::Disabled {
            self.failure = Some(failure);
            self.phase = ProtocolPhase::Failed;
        }
        failure
    }
    pub fn shutdown(&mut self) {
        if !matches!(self.phase, ProtocolPhase::Disabled | ProtocolPhase::Failed) {
            self.phase = ProtocolPhase::Shutdown;
        }
    }
    fn refresh_deadline(&mut self, now: Duration) -> bool {
        match now.checked_add(self.timeouts.heartbeat) {
            Some(deadline) => {
                self.deadline = deadline;
                true
            }
            None => {
                self.fail(Diagnostic::InvalidLimit);
                false
            }
        }
    }
}
