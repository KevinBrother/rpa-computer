//! Process/foreground ownership metadata. Opaque native handles retain bounded
//! launch resources; no parsed cookie, DISPLAY, X connection, native-input token
//! or permission cache is stored or interpreted by this tracker.
use super::policy::{Observation, SignalResult};
use crate::x11_credentials::MAX_PENDING_CHILDREN;
use crate::x11_credentials::{Cleanup, FailureKind};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    pub slot: usize,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub ticket: Ticket,
    pub pid: i32,
}

/// Each operation is a short metadata operation. No tracker mutex may span
/// file/socket waits, clone, signal-mask manipulation, parser or caller joins.
pub(super) trait Ownership<H> {
    /// Child-only reservation primitive; the launcher uses reserve_foreground.
    fn reserve(&self) -> Result<Ticket, FailureKind>;
    /// Own both child and foreground lifetime before any preparation/allocation.
    /// Resource-free policy adapters may use the child-only default.
    fn reserve_foreground(&self) -> Result<Ticket, FailureKind> {
        self.reserve()
    }
    fn retire_foreground(&self, _ticket: Ticket) -> Result<(), FailureKind> {
        Ok(())
    }
    fn register_attempts(&self, _ticket: Ticket, _attempts: [H; 2]) -> Result<(), FailureKind> {
        Err(FailureKind::LauncherUnavailable)
    }
    fn attach_attempt(
        &self,
        _ticket: Ticket,
        _pid: i32,
        _attempt: usize,
    ) -> Result<Identity, FailureKind> {
        Err(FailureKind::LauncherUnavailable)
    }
    fn attach(&self, ticket: Ticket, pid: i32, pidfd: Option<H>) -> Result<Identity, FailureKind>;
    fn unstarted(&self, ticket: Ticket) -> Result<(), FailureKind>;
    fn handle(&self, identity: Identity) -> Result<Option<Arc<H>>, FailureKind>;
    fn quarantine(&self, identity: Identity, signal: SignalResult) -> Result<Cleanup, FailureKind>;
    fn observe(&self, identity: Identity, evidence: Observation) -> Result<Cleanup, FailureKind>;
    fn occupied(&self) -> usize;
}

/// Only metadata is locked; OS operations use cloned stable handles outside it.
struct Slot<H> {
    ticket: Ticket,
    child: Option<(i32, Option<Arc<H>>)>,
    cleanup: Cleanup,
    foreground: bool,
    attempts: Option<[Arc<H>; 2]>,
}
struct State<H> {
    generation: u64,
    slots: [Option<Slot<H>>; MAX_PENDING_CHILDREN],
}
pub(super) struct Tracker<H> {
    state: Mutex<State<H>>,
}
impl<H> Tracker<H> {
    fn reserve_with_foreground(&self, foreground: bool) -> Result<Ticket, FailureKind> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let index = state
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(FailureKind::CapacityExhausted)?;
        // Never wrap generations and accidentally authorize a stale identity.
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or(FailureKind::CapacityExhausted)?;
        let ticket = Ticket {
            slot: index,
            generation: state.generation,
        };
        state.slots[index] = Some(Slot {
            ticket,
            child: None,
            cleanup: Cleanup::Pending,
            foreground,
            attempts: None,
        });
        Ok(ticket)
    }

    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                generation: 0,
                slots: std::array::from_fn(|_| None),
            }),
        }
    }
    /// Fixed two-attempt records remain process-owned before/after raw return.
    /// No pid/output guess: the native reaper requires acquire-positive result.
    pub fn pending_attempts(&self) -> [Option<(Ticket, usize, Arc<H>)>; MAX_PENDING_CHILDREN * 2] {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        std::array::from_fn(|index| {
            let slot = state.slots[index / 2].as_ref()?;
            if slot.child.is_some() {
                return None;
            }
            Some((
                slot.ticket,
                index % 2,
                slot.attempts.as_ref()?[index % 2].clone(),
            ))
        })
    }
    /// Stable handles only. Reaping may race a foreground observer; Handle caches
    /// positive wait evidence, and generations prevent releasing a reused slot.
    pub fn children(&self) -> [Option<(Identity, Arc<H>)>; MAX_PENDING_CHILDREN] {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        std::array::from_fn(|index| {
            let slot = state.slots[index].as_ref()?;
            if slot.cleanup == Cleanup::Reaped {
                return None;
            }
            let (pid, handle) = slot.child.as_ref()?;
            Some((
                Identity {
                    ticket: slot.ticket,
                    pid: *pid,
                },
                handle.clone()?,
            ))
        })
    }
}
fn slot<H>(state: &mut State<H>, ticket: Ticket) -> Result<&mut Slot<H>, FailureKind> {
    state
        .slots
        .get_mut(ticket.slot)
        .and_then(Option::as_mut)
        .filter(|s| s.ticket == ticket)
        .ok_or(FailureKind::StaleIdentity)
}
fn child<H>(state: &mut State<H>, id: Identity) -> Result<&mut Slot<H>, FailureKind> {
    let slot = slot(state, id.ticket)?;
    if slot.child.as_ref().map(|c| c.0) != Some(id.pid) {
        return Err(FailureKind::StaleIdentity);
    }
    Ok(slot)
}
impl<H> Ownership<H> for Tracker<H> {
    fn reserve(&self) -> Result<Ticket, FailureKind> {
        self.reserve_with_foreground(false)
    }
    fn reserve_foreground(&self) -> Result<Ticket, FailureKind> {
        self.reserve_with_foreground(true)
    }
    fn retire_foreground(&self, ticket: Ticket) -> Result<(), FailureKind> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = slot(&mut state, ticket)?;
        if !slot.foreground {
            return Err(FailureKind::StaleIdentity);
        }
        slot.foreground = false;
        // An unpublished launch is NOT NotStarted. Pending/Unknown remain owned.
        if matches!(slot.cleanup, Cleanup::NotStarted | Cleanup::Reaped) {
            let retired = state.slots[ticket.slot].take();
            drop(state);
            drop(retired);
        }
        Ok(())
    }
    fn register_attempts(&self, ticket: Ticket, attempts: [H; 2]) -> Result<(), FailureKind> {
        let handles = attempts.map(Arc::new);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = slot(&mut state, ticket)?;
        if slot.child.is_some() || slot.attempts.is_some() || slot.cleanup != Cleanup::Pending {
            return Err(FailureKind::StaleIdentity);
        }
        slot.attempts = Some(handles);
        Ok(())
    }
    fn attach_attempt(
        &self,
        ticket: Ticket,
        pid: i32,
        attempt: usize,
    ) -> Result<Identity, FailureKind> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = slot(&mut state, ticket)?;
        let handle = slot
            .attempts
            .as_ref()
            .and_then(|a| a.get(attempt))
            .ok_or(FailureKind::StaleIdentity)?;
        if pid <= 0 || slot.cleanup == Cleanup::NotStarted {
            return Err(FailureKind::StaleIdentity);
        }
        if let Some((existing_pid, existing)) = slot.child.as_ref() {
            if *existing_pid != pid || !existing.as_ref().is_some_and(|h| Arc::ptr_eq(h, handle)) {
                return Err(FailureKind::StaleIdentity);
            }
            // Creator/reaper resolve the SAME pre-owned handle; no new adoption.
            return Ok(Identity { ticket, pid });
        }
        slot.child = Some((pid, Some(handle.clone())));
        Ok(Identity { ticket, pid })
    }
    fn attach(&self, ticket: Ticket, pid: i32, pidfd: Option<H>) -> Result<Identity, FailureKind> {
        // Allocate before taking the short metadata lock.
        let handle = pidfd.map(Arc::new);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = slot(&mut state, ticket)?;
        if slot.child.is_some()
            || slot.attempts.is_some()
            || slot.cleanup == Cleanup::NotStarted
            || pid <= 0
        {
            return Err(FailureKind::StaleIdentity);
        }
        slot.cleanup = if handle.is_some() {
            Cleanup::Pending
        } else {
            Cleanup::Unknown
        };
        slot.child = Some((pid, handle));
        Ok(Identity { ticket, pid })
    }
    fn unstarted(&self, ticket: Ticket) -> Result<(), FailureKind> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = slot(&mut state, ticket)?;
        if slot.child.is_some() || slot.cleanup == Cleanup::NotStarted {
            return Err(FailureKind::StaleIdentity);
        }
        if slot.foreground {
            // Foreground allocations/IPC may still exist until its outer guard.
            slot.cleanup = Cleanup::NotStarted;
        } else {
            let retired = state.slots[ticket.slot].take();
            drop(state);
            drop(retired);
        }
        Ok(())
    }
    fn handle(&self, id: Identity) -> Result<Option<Arc<H>>, FailureKind> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Ok(child(&mut state, id)?
            .child
            .as_ref()
            .and_then(|c| c.1.clone()))
    }
    fn quarantine(&self, id: Identity, signal: SignalResult) -> Result<Cleanup, FailureKind> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = child(&mut state, id)?;
        if signal == SignalResult::Failed && slot.cleanup != Cleanup::Reaped {
            slot.cleanup = Cleanup::Unknown;
        }
        Ok(slot.cleanup)
    }
    fn observe(&self, id: Identity, evidence: Observation) -> Result<Cleanup, FailureKind> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let slot = child(&mut state, id)?;
        if slot.cleanup == Cleanup::Reaped {
            return Ok(Cleanup::Reaped); // Positive completion evidence is sticky.
        }
        match evidence {
            Observation::Reaped => {
                if slot.foreground {
                    slot.cleanup = Cleanup::Reaped;
                    return Ok(Cleanup::Reaped);
                }
                let retired = state.slots[id.ticket.slot].take();
                drop(state);
                drop(retired); // Never run a handle destructor under the mutex.
                Ok(Cleanup::Reaped)
            }
            Observation::Echild | Observation::Failed => {
                slot.cleanup = Cleanup::Unknown;
                Ok(Cleanup::Unknown)
            }
            Observation::NotReady => Ok(slot.cleanup),
        }
    }
    fn occupied(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .slots
            .iter()
            .filter(|s| s.is_some())
            .count()
    }
}
