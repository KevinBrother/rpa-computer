//! Control requests have their own bounded mailbox, separate from display data.
//! The Host's serialized lifecycle owner validates session+generation at dequeue
//! and immediately revokes control before cancellation/cleanup or further grants.
use crate::{
    channel::lock_error,
    protocol::{Cleanup, Session},
    Diagnostic,
};
use std::{collections::VecDeque, sync::Mutex};

pub struct StopInbox {
    queue: Mutex<VecDeque<Session>>,
    capacity: usize,
}
impl StopInbox {
    pub fn new(capacity: usize) -> Result<Self, Diagnostic> {
        if capacity == 0 || capacity > 64 {
            return Err(Diagnostic::InvalidLimit);
        }
        Ok(Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        })
    }
    /// Duplicate pending requests coalesce. Overflow/poisoning is a connection
    /// failure, not a silently dropped valid stop: notify the supervisor.
    pub fn try_push(&self, session: Session) -> Result<(), Diagnostic> {
        session.validate()?;
        let mut queue = self.queue.try_lock().map_err(lock_error)?;
        if queue.contains(&session) {
            return Ok(());
        }
        if queue.len() == self.capacity {
            return Err(Diagnostic::ControlOverflow);
        }
        queue.push_back(session);
        Ok(())
    }
    pub fn try_pop(&self) -> Result<Option<Session>, Diagnostic> {
        Ok(self.queue.try_lock().map_err(lock_error)?.pop_front())
    }
    pub fn len(&self) -> Result<usize, Diagnostic> {
        Ok(self.queue.try_lock().map_err(lock_error)?.len())
    }
    pub fn is_empty(&self) -> Result<bool, Diagnostic> {
        Ok(self.len()? == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRejection {
    InvalidSession,
    ControlAlreadyGranted,
    WrongSession,
    NoControl,
    CleanupPending,
    CleanupUnconfirmed,
    WrongStopToken,
    GenerationReused,
    EpochExhausted,
}
/// An authorized cancellation target. Fields cannot be forged via public API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopToken {
    session: Session,
    epoch: u64,
}
impl StopToken {
    pub fn session(&self) -> &Session {
        &self.session
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}
#[derive(Debug, Default)]
pub struct ControlGate {
    current: Option<Session>,
    last_grant: Option<Session>,
    pending: Option<StopToken>,
    epoch: u64,
}
impl ControlGate {
    pub fn current(&self) -> Option<&Session> {
        self.current.as_ref()
    }
    pub fn pending_stop(&self) -> Option<&StopToken> {
        self.pending.as_ref()
    }
    /// Call only after enabled feedback is Ready (or feedback is Disabled).
    /// Session IDs must be Host-owned and unique across control lifetimes; for
    /// the same ID generations must increase. This is not an authentication API.
    pub fn grant(&mut self, session: Session) -> Result<(), StopRejection> {
        session
            .validate()
            .map_err(|_| StopRejection::InvalidSession)?;
        if self.pending.is_some() {
            return Err(StopRejection::CleanupPending);
        }
        if self.current.is_some() {
            return Err(StopRejection::ControlAlreadyGranted);
        }
        if self
            .last_grant
            .as_ref()
            .is_some_and(|last| last.id == session.id && session.generation <= last.generation)
        {
            return Err(StopRejection::GenerationReused);
        }
        self.advance_epoch()?;
        self.last_grant = Some(session.clone());
        self.current = Some(session);
        Ok(())
    }
    pub fn authorize_stop(&mut self, request: &Session) -> Result<StopToken, StopRejection> {
        request
            .validate()
            .map_err(|_| StopRejection::InvalidSession)?;
        let current = self.current.as_ref().ok_or(StopRejection::NoControl)?;
        if current != request {
            return Err(StopRejection::WrongSession);
        }
        // Authorization and invalidation are one operation under the Host owner.
        let token = StopToken {
            session: current.clone(),
            epoch: self.epoch,
        };
        self.current = None;
        self.pending = Some(token.clone());
        Ok(token)
    }
    /// Trusted Host path for enabled renderer loss: idempotently revoke whatever
    /// current control exists, without relying on a rendered/session snapshot.
    pub fn revoke_current(&mut self) -> Result<Option<StopToken>, StopRejection> {
        if let Some(token) = &self.pending {
            return Ok(Some(token.clone()));
        }
        match self.current.clone() {
            Some(session) => self.authorize_stop(&session).map(Some),
            None => Ok(None),
        }
    }
    /// Only actual input cleanup acknowledgement can unlock a new grant.
    /// Failed/unknown/pending retains the pending token, allowing a later retry.
    pub fn finish_stop(
        &mut self,
        token: &StopToken,
        cleanup: Cleanup,
    ) -> Result<(), StopRejection> {
        if self.pending.as_ref() != Some(token) {
            return Err(StopRejection::WrongStopToken);
        }
        if !matches!(cleanup, Cleanup::Released | Cleanup::NotNeeded) {
            return Err(StopRejection::CleanupUnconfirmed);
        }
        self.pending = None;
        Ok(())
    }
    fn advance_epoch(&mut self) -> Result<(), StopRejection> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or(StopRejection::EpochExhausted)?;
        Ok(())
    }
}
