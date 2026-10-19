//! Pure presentation facts. A local Stop request never asserts Host cleanup.
use rpa_desktop_feedback::{
    protocol::{Cleanup, Phase, Pointer, PointerKind, Session, Snapshot},
    Diagnostic,
};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Normal,
    Warning,
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    pub text: &'static str,
    pub severity: Severity,
    pub safe_completion: bool,
}

struct Ring {
    pointer: Pointer,
    started: Duration,
    expires: Duration,
}

#[derive(Default)]
pub struct FeedbackModel {
    snapshot: Option<Snapshot>,
    // Keep the tombstone through session:null; only a different non-null grant rearms Stop.
    stopped: Option<Session>,
    ring: Option<Ring>,
    last_apply: Option<Duration>,
}
impl FeedbackModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stale snapshots cannot change the session, restart a ring, or rearm Stop.
    /// Validate and compute fallible values before committing any new state.
    pub fn apply(&mut self, snapshot: Snapshot, now: Duration) -> Result<bool, Diagnostic> {
        snapshot.validate()?;
        if self
            .sequence()
            .is_some_and(|sequence| snapshot.sequence <= sequence)
        {
            return Ok(false);
        }
        if self.last_apply.is_some_and(|previous| now < previous) {
            return Err(Diagnostic::ClockRegression);
        }
        let ring = if snapshot.session.is_some() && snapshot.phase != Phase::Closed {
            snapshot
                .pointer
                .as_ref()
                .map(|pointer| {
                    let lifetime = Duration::from_millis(match pointer.kind {
                        PointerKind::Click => 650,
                        PointerKind::Move | PointerKind::Drag => 350,
                    });
                    Ok(Ring {
                        pointer: pointer.clone(),
                        started: now,
                        expires: now.checked_add(lifetime).ok_or(Diagnostic::InvalidLimit)?,
                    })
                })
                .transpose()?
        } else {
            None
        };
        if let Some(session) = &snapshot.session {
            if self
                .stopped
                .as_ref()
                .is_some_and(|stopped| stopped != session)
            {
                self.stopped = None;
            }
        }
        self.snapshot = Some(snapshot);
        self.ring = ring;
        self.last_apply = Some(now);
        Ok(true)
    }

    pub fn sequence(&self) -> Option<u64> {
        self.snapshot.as_ref().map(|snapshot| snapshot.sequence)
    }

    pub fn can_stop(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.phase != Phase::Closed
                && snapshot
                    .session
                    .as_ref()
                    .is_some_and(|session| self.stopped.as_ref() != Some(session))
        })
    }

    /// The caller must enqueue this before repainting. Failure must terminate the
    /// renderer connection; it must not rearm this session and silently retry.
    pub fn request_stop(&mut self) -> Option<Session> {
        if !self.can_stop() {
            return None;
        }
        let session = self.snapshot.as_ref()?.session.clone()?;
        self.stopped = Some(session.clone());
        Some(session)
    }

    pub fn presentation(&self) -> Presentation {
        let Some(snapshot) = &self.snapshot else {
            return presentation("Waiting for Host", Severity::Normal, false);
        };
        let warning = match snapshot.cleanup {
            Cleanup::Failed => Some(("Cleanup failed · inputs may remain held", Severity::Error)),
            Cleanup::Unknown => Some(("Cleanup unknown · not confirmed", Severity::Warning)),
            Cleanup::Pending => Some(("Stopping · cleanup pending", Severity::Warning)),
            Cleanup::Released | Cleanup::NotNeeded => None,
        };
        if let Some((text, severity)) = warning {
            return presentation(text, severity, false);
        }
        if snapshot.phase == Phase::Closed {
            let text = if snapshot.cleanup == Cleanup::Released {
                "Session closed · inputs released"
            } else {
                "Session closed · no cleanup required"
            };
            return presentation(text, Severity::Normal, snapshot.safe_completion());
        }
        if snapshot.session.is_some() && snapshot.session == self.stopped {
            return presentation("Stop requested · awaiting Host", Severity::Warning, false);
        }
        if snapshot.phase == Phase::Faulted {
            return presentation(
                if snapshot.session.is_some() {
                    "Host fault · stop available"
                } else {
                    "Host fault · no control session"
                },
                Severity::Error,
                false,
            );
        }
        if snapshot.session.is_none() {
            return presentation("Waiting for control session", Severity::Normal, false);
        }
        let text = match snapshot.phase {
            Phase::Starting => "Connecting to Host",
            Phase::Idle => "Control held · waiting for Agent",
            Phase::Observing => "Observing · no input action",
            Phase::Executing => "Executing confirmed actions",
            Phase::Paused => "Control paused",
            Phase::Stopping => "Stopping · awaiting cleanup",
            // These branches are handled above, not inferred from local UI state.
            Phase::Faulted | Phase::Closed => unreachable!("terminal phase handled above"),
        };
        presentation(text, Severity::Normal, false)
    }

    /// A read does not advance time or refresh animation. Geometry membership is
    /// checked by geometry::resolve before these native coordinates are painted.
    pub fn ring(&self, now: Duration) -> Option<Pointer> {
        self.ring
            .as_ref()
            .filter(|ring| now >= ring.started && now < ring.expires)
            .map(|ring| ring.pointer.clone())
    }

    pub fn invalidate_geometry(&mut self) {
        self.ring = None;
    }
}

fn presentation(text: &'static str, severity: Severity, safe_completion: bool) -> Presentation {
    Presentation {
        text,
        severity,
        safe_completion,
    }
}
