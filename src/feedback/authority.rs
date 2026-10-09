//! The single, serialized lifecycle owner. Locks protect only bounded facts;
//! never native calls or pipe I/O. There is no pointer/history event queue.
use super::FeedbackError;
use crate::backend::Geometry;
use rpa_desktop_feedback::{
    channel::{Publisher, Subscriber},
    control::ControlGate,
    protocol::{Cleanup, Phase, Pointer, Session, Snapshot, Surface, V1},
    Diagnostic,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Debug, Clone, Copy)]
pub enum RuntimePhase {
    Idle,
    Paused,
    Faulted,
    Closed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatchStamp {
    session: Session,
    surface_id: String,
    version: String,
}
struct Owner {
    gate: ControlGate,
    snapshot: Snapshot,
    generation: u64,
    dirty: bool,
    // Once the coordinator quarantines a native thread, late cleanup is not
    // permission to declare that whole worker safe again.
    quarantined: bool,
}
struct Shared {
    owner: Mutex<Owner>,
    publisher: Publisher,
    terminated: AtomicBool,
    notified: AtomicBool,
    on_stop: Arc<dyn Fn() + Send + Sync>,
}
#[derive(Clone)]
pub struct FeedbackHandle(Arc<Shared>);
impl FeedbackHandle {
    pub fn new(on_stop: Arc<dyn Fn() + Send + Sync>) -> Result<Self, FeedbackError> {
        Ok(Self(Arc::new(Shared {
            owner: Mutex::new(Owner {
                gate: ControlGate::default(),
                generation: 0,
                dirty: true,
                quarantined: false,
                snapshot: Snapshot {
                    version: V1,
                    sequence: 1,
                    session: None,
                    phase: Phase::Starting,
                    cleanup: Cleanup::NotNeeded,
                    surface: None,
                    pointer: None,
                },
            }),
            publisher: Publisher::new(1).map_err(FeedbackError::Protocol)?,
            terminated: AtomicBool::new(false),
            notified: AtomicBool::new(false),
            on_stop,
        })))
    }
    pub fn is_terminated(&self) -> bool {
        self.0.terminated.load(Ordering::SeqCst)
    }
    fn latch(&self) {
        self.0.terminated.store(true, Ordering::SeqCst);
        self.notify();
    }
    fn notify(&self) {
        if !self.0.notified.swap(true, Ordering::SeqCst) {
            (self.0.on_stop)();
        }
    }
    fn edit<T>(
        &self,
        f: impl FnOnce(&mut Owner) -> Result<T, FeedbackError>,
    ) -> Result<T, FeedbackError> {
        let result = self
            .0
            .owner
            .lock()
            .map_err(|_| FeedbackError::AuthorityUnavailable)
            .and_then(|mut s| f(&mut s));
        if matches!(
            &result,
            Err(FeedbackError::AuthorityUnavailable | FeedbackError::Protocol(_))
        ) {
            self.latch();
        }
        result
    }
    pub fn grant(&self, id: &str, geometry: &Geometry) -> Result<Session, FeedbackError> {
        self.edit(|s| {
            if self.is_terminated() {
                return Err(FeedbackError::AuthorityRevoked);
            }
            let generation = s
                .generation
                .checked_add(1)
                .ok_or(FeedbackError::AuthorityUnavailable)?;
            let session = Session {
                id: id.into(),
                generation,
            };
            let surface = surface(geometry);
            surface.validate().map_err(FeedbackError::Protocol)?;
            s.gate
                .grant(session.clone())
                .map_err(|_| FeedbackError::AuthorityRevoked)?;
            s.generation = generation;
            s.snapshot.session = Some(session.clone());
            s.snapshot.surface = Some(surface);
            s.snapshot.pointer = None;
            s.snapshot.phase = Phase::Idle;
            s.snapshot.cleanup = Cleanup::NotNeeded;
            changed(s)?;
            Ok(session)
        })
    }
    /// Stop identity and permanent child revocation are one critical section.
    /// The cancellation hook bumps worker epoch, latches cancel and sends Quit.
    pub fn stop(&self, request: &Session) -> bool {
        let accepted = self
            .edit(|s| {
                if self.is_terminated() || s.gate.authorize_stop(request).is_err() {
                    return Ok(false);
                }
                self.0.terminated.store(true, Ordering::SeqCst);
                stopping(s);
                changed(s)?;
                Ok(true)
            })
            .unwrap_or(false);
        if accepted {
            self.notify();
        }
        accepted
    }
    /// Sticky renderer failure also revokes a child with no session yet. This
    /// closes the ready→open window; failure cannot be erased by open/resume.
    pub fn terminate(&self) {
        let first = self
            .edit(|s| {
                if self.0.terminated.swap(true, Ordering::SeqCst) {
                    return Ok(false);
                }
                let _ = s.gate.revoke_current();
                stopping(s);
                changed(s)?;
                Ok(true)
            })
            .unwrap_or(false);
        if first {
            self.notify();
        }
    }
    pub fn settle(&self, id: &str, phase: RuntimePhase, geometry: &Geometry) {
        let _ = self.edit(|s| {
            if self.is_terminated() || !s.gate.current().is_some_and(|x| x.id == id) {
                return Ok(());
            }
            if matches!(phase, RuntimePhase::Closed) {
                retire(s, s.snapshot.cleanup)?;
            } else {
                let next = surface(geometry);
                if s.snapshot.surface.as_ref() != Some(&next) {
                    s.snapshot.pointer = None;
                }
                s.snapshot.surface = Some(next);
                s.snapshot.phase = match phase {
                    RuntimePhase::Idle => Phase::Idle,
                    RuntimePhase::Paused => Phase::Paused,
                    _ => Phase::Faulted,
                };
                if !matches!(phase, RuntimePhase::Idle) {
                    s.snapshot.pointer = None;
                }
                changed(s)?;
            }
            Ok(())
        });
    }
    /// Ordinary close can be followed by an explicit new session. Terminal
    /// renderer stop deliberately cannot: that requires a fresh Host child.
    pub fn retire(&self, cleanup: Cleanup) {
        let _ = self.edit(|s| {
            if !self.is_terminated() {
                retire(s, cleanup)?;
            }
            Ok(())
        });
    }
    pub fn complete_terminal(&self, cleanup: Cleanup) {
        let _ = self.edit(|s| {
            if !self.is_terminated() || s.quarantined {
                return Ok(());
            }
            if cleanup == Cleanup::Unknown {
                s.quarantined = true;
            }
            s.snapshot.session = None;
            s.snapshot.pointer = None;
            s.snapshot.cleanup = cleanup;
            s.snapshot.phase = if matches!(cleanup, Cleanup::Released | Cleanup::NotNeeded) {
                Phase::Closed
            } else {
                Phase::Faulted
            };
            if let Some(token) = s.gate.pending_stop().cloned() {
                let _ = s.gate.finish_stop(&token, cleanup);
            }
            changed(s)
        });
    }
    pub(crate) fn begin_dispatch(
        &self,
        phase: Phase,
    ) -> Result<Option<DispatchStamp>, FeedbackError> {
        self.edit(|s| {
            if self.is_terminated() {
                return Err(FeedbackError::AuthorityRevoked);
            }
            let stamp = stamp(s);
            if stamp.is_some() {
                s.snapshot.phase = phase;
                changed(s)?;
            }
            Ok(stamp)
        })
    }
    pub(crate) fn confirmed(&self, tag: &DispatchStamp, pointer: Pointer) {
        let _ = self.edit(|s| {
            if !self.is_terminated() && stamp(s).as_ref() == Some(tag) {
                s.snapshot.pointer = Some(pointer);
                changed(s)?;
            }
            Ok(())
        });
    }
    pub(crate) fn captured(&self, tag: &DispatchStamp, geometry: &Geometry) {
        let _ = self.edit(|s| {
            if !self.is_terminated() && stamp(s).as_ref() == Some(tag) {
                let next = surface(geometry);
                if s.snapshot.surface.as_ref() != Some(&next) {
                    s.snapshot.pointer = None;
                }
                s.snapshot.surface = Some(next);
                changed(s)?;
            }
            Ok(())
        });
    }
    pub(crate) fn released(&self, cleanup: Cleanup) {
        let _ = self.edit(|s| {
            if !self.is_terminated() {
                s.snapshot.cleanup = cleanup;
                changed(s)?;
            }
            Ok(())
        });
    }
    pub(crate) fn subscribe(&self) -> Result<Subscriber, FeedbackError> {
        self.0
            .publisher
            .subscribe()
            .map_err(FeedbackError::Protocol)
    }
    /// Nonblocking publication with a single retained latest retry slot (the
    /// owner snapshot itself). Input never waits for serialization or pipes.
    pub(crate) fn flush(&self) -> Result<(), FeedbackError> {
        let mut s = match self.0.owner.try_lock() {
            Ok(s) => s,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(()),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(FeedbackError::AuthorityUnavailable)
            }
        };
        if !s.dirty {
            return Ok(());
        }
        match self.0.publisher.try_publish(s.snapshot.clone()) {
            Ok(()) => {
                s.dirty = false;
                Ok(())
            }
            Err(e) if e.diagnostic == Diagnostic::Contended => Ok(()),
            Err(e) => Err(FeedbackError::Protocol(e.diagnostic)),
        }
    }
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Snapshot {
        self.0.owner.lock().unwrap().snapshot.clone()
    }
}
fn changed(s: &mut Owner) -> Result<(), FeedbackError> {
    s.snapshot.sequence = s
        .snapshot
        .sequence
        .checked_add(1)
        .ok_or(FeedbackError::Protocol(Diagnostic::SequenceExhausted))?;
    s.dirty = true;
    Ok(())
}
fn stopping(s: &mut Owner) {
    s.snapshot.session = None;
    s.snapshot.pointer = None;
    s.snapshot.phase = Phase::Stopping;
    s.snapshot.cleanup = Cleanup::Pending;
}
fn retire(s: &mut Owner, cleanup: Cleanup) -> Result<(), FeedbackError> {
    if let Ok(Some(token)) = s.gate.revoke_current() {
        let _ = s.gate.finish_stop(&token, cleanup);
    }
    s.snapshot.session = None;
    s.snapshot.pointer = None;
    s.snapshot.cleanup = cleanup;
    s.snapshot.phase = if matches!(cleanup, Cleanup::Released | Cleanup::NotNeeded) {
        Phase::Closed
    } else {
        Phase::Faulted
    };
    changed(s)
}
fn stamp(s: &Owner) -> Option<DispatchStamp> {
    let session = s.gate.current()?.clone();
    let surface = s.snapshot.surface.as_ref()?;
    Some(DispatchStamp {
        session,
        surface_id: surface.id.clone(),
        version: surface.version.clone(),
    })
}
fn surface(g: &Geometry) -> Surface {
    Surface {
        id: g.surface_id.clone(),
        version: g.version.clone(),
        x: g.input_origin.0 as f64,
        y: g.input_origin.1 as f64,
        width: g.input_size.0 as f64,
        height: g.input_size.1 as f64,
    }
}
