//! Parent-only launch/deadline policy, shared by production and trace tests.
use super::tracker::{Identity, Ownership};
use crate::x11_credentials::{
    ipc::AuthorityRead, Acquired, Cleanup, Failure, FailureKind, Request,
};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Errno {
    Enosys,
    Eperm,
    Eacces,
    Eagain,
    Enomem,
    Einval,
    Other(i32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Method {
    Clone3,
    Legacy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Checkpoint {
    BeforeLaunch,
    BeforeFallback,
    BeforeGate,
    AfterReply,
    AfterParse,
}
/// Parent return only. The future raw child leaf must never return here.
pub(super) enum Spawn<H> {
    Unavailable,
    /// No definitive outcome: keep reservation, never fallback/replay.
    Uncertain,
    NoChild(Errno),
    Parent {
        pid: i32,
        pidfd: Option<H>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Observation {
    NotReady,
    Reaped,
    Echild,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SignalResult {
    Requested,
    Failed,
}

/// Parent-side adapter only; none of these Rust methods may run in the child.
/// Signal requests take ONLY the stable handle, never a reusable bare PID.
pub(super) trait Adapter {
    type Handle;
    type Mask;
    // No-op in production; observed at the same actual attach/reply boundaries.
    #[cfg(test)]
    fn lifecycle_attached(&mut self) {}
    #[cfg(test)]
    fn lifecycle_reply_received(&mut self) {}
    /// Release adapter-owned foreground resources before returning its permit.
    /// Resource-free scripted/unsupported adapters need no extra retirement.
    fn retire_foreground(&mut self) {}
    fn launch_handles(&self) -> Option<[Self::Handle; 2]> {
        None
    }
    fn launch_registered(&mut self) {}
    fn owned_attempt(&self) -> Option<usize> {
        None
    }
    fn prepare(&mut self, request: &Request<'_>) -> Result<(), FailureKind>;
    fn checkpoint(&mut self, point: Checkpoint, deadline: Instant) -> Result<(), FailureKind>;
    fn block_signals(&mut self) -> Result<Self::Mask, FailureKind>;
    fn restore_signals(&mut self, mask: Self::Mask) -> Result<(), FailureKind>;
    fn clone3(&mut self) -> Spawn<Self::Handle>;
    fn legacy_clone_pidfd(&mut self) -> Spawn<Self::Handle>;
    fn validate_pidfd(&mut self, handle: &Self::Handle) -> Result<(), FailureKind>;
    fn open_startup_gate(&mut self) -> Result<(), FailureKind>;
    fn revoke_ipc(&mut self);
    fn receive(&mut self, deadline: Instant) -> Result<AuthorityRead, FailureKind>;
    fn kill_pidfd(&mut self, handle: &Self::Handle) -> SignalResult;
    fn observe(&mut self, handle: &Self::Handle) -> Observation;
}

/// Capacity is reserved even during allocation/setup. Release only when the
/// adapter positively says no child was created, never after ambiguous spawn.
struct Reservation<'a, O, H>
where
    O: Ownership<H>,
{
    owner: &'a O,
    ticket: Option<super::tracker::Ticket>,
    marker: std::marker::PhantomData<H>,
}
impl<O: Ownership<H>, H> Drop for Reservation<'_, O, H> {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket {
            let _ = self.owner.unstarted(ticket);
        }
    }
}
struct Active<'a, A: Adapter, O: Ownership<A::Handle>> {
    adapter: &'a mut A,
    owner: &'a O,
    id: Identity,
    armed: bool,
}
impl<A: Adapter, O: Ownership<A::Handle>> Drop for Active<'_, A, O> {
    fn drop(&mut self) {
        if self.armed {
            let _ = cancel(self.adapter, self.owner, self.id);
        }
    }
}
fn not_started(kind: FailureKind) -> Failure {
    Failure {
        kind,
        cleanup: Cleanup::NotStarted,
    }
}

pub(super) fn run<A: Adapter, O: Ownership<A::Handle>>(
    adapter: &mut A,
    owner: &O,
    request: Request<'_>,
    deadline: Instant,
) -> Result<Acquired, Failure> {
    let foreground = super::foreground::Foreground::new(adapter, owner).map_err(not_started)?;
    run_reserved(
        foreground.adapter,
        owner,
        foreground.ticket,
        request,
        deadline,
    )
}

fn run_reserved<A: Adapter, O: Ownership<A::Handle>>(
    adapter: &mut A,
    owner: &O,
    ticket: super::tracker::Ticket,
    request: Request<'_>,
    deadline: Instant,
) -> Result<Acquired, Failure> {
    let mut reservation = Reservation {
        owner,
        ticket: Some(ticket),
        marker: std::marker::PhantomData,
    };
    adapter
        .checkpoint(Checkpoint::BeforeLaunch, deadline)
        .map_err(not_started)?;
    adapter.prepare(&request).map_err(not_started)?;
    if let Some(handles) = adapter.launch_handles() {
        owner
            .register_attempts(ticket, handles)
            .map_err(not_started)?;
        adapter.launch_registered();
    }
    adapter
        .checkpoint(Checkpoint::BeforeLaunch, deadline)
        .map_err(not_started)?;
    let mask = adapter.block_signals().map_err(not_started)?;
    if let Err(kind) = adapter.checkpoint(Checkpoint::BeforeLaunch, deadline) {
        let _ = adapter.restore_signals(mask);
        adapter.revoke_ipc();
        return Err(not_started(kind));
    }
    // From here, unwinding alone cannot establish that no child was created.
    // Leave an unpublished result bounded/owned; do not classify it by raw FD.
    reservation.ticket = None;
    let mut spawn = adapter.clone3();
    let mut before_fallback_error = None;
    if matches!(
        spawn,
        Spawn::NoChild(Errno::Enosys | Errno::Eperm | Errno::Eacces)
    ) {
        match adapter.checkpoint(Checkpoint::BeforeFallback, deadline) {
            Ok(()) => spawn = adapter.legacy_clone_pidfd(),
            Err(kind) => before_fallback_error = Some(kind),
        }
    }
    // Always restore, including denial, timeout-before-fallback and child success.
    let restored = adapter.restore_signals(mask);
    let (pid, pidfd) = match spawn {
        Spawn::Parent { pid, pidfd } => (pid, pidfd),
        Spawn::Uncertain => {
            adapter.revoke_ipc();
            return Err(Failure {
                kind: FailureKind::ChildSetupFailed,
                cleanup: Cleanup::Unknown,
            });
        }
        Spawn::NoChild(_) | Spawn::Unavailable => {
            // The adapter has explicitly confirmed no child. Re-arm the
            // reservation only now, after the last attempted spawn returned.
            reservation.ticket = Some(ticket);
            adapter.revoke_ipc();
            return Err(not_started(
                before_fallback_error
                    .or(restored.err())
                    .unwrap_or(FailureKind::LauncherUnavailable),
            ));
        }
    };
    // A child exists: reservation remains disarmed, including attach failure.
    let attached = if let Some(attempt) = adapter.owned_attempt() {
        owner.attach_attempt(ticket, pid, attempt)
    } else {
        owner.attach(ticket, pid, pidfd)
    };
    let id = attached.map_err(|kind| {
        adapter.revoke_ipc();
        Failure {
            kind,
            cleanup: Cleanup::Unknown,
        }
    })?;
    #[cfg(test)]
    adapter.lifecycle_attached();
    let handle = owner.handle(id).map_err(|kind| Failure {
        kind,
        cleanup: Cleanup::Unknown,
    })?;
    let validation = handle
        .as_ref()
        .ok_or(FailureKind::ChildSetupFailed)
        .and_then(|h| adapter.validate_pidfd(h));
    if let Err(kind) = validation {
        adapter.revoke_ipc();
        // Do not signal a descriptor that has not been established as our pidfd.
        let cleanup = owner
            .quarantine(id, SignalResult::Failed)
            .unwrap_or(Cleanup::Unknown);
        return Err(Failure { kind, cleanup });
    }
    let handle = handle.expect("validated owned handle");
    let mut active = Active {
        adapter,
        owner,
        id,
        armed: true,
    };
    let work = (|| {
        restored?;
        active
            .adapter
            .checkpoint(Checkpoint::BeforeGate, deadline)?;
        active.adapter.open_startup_gate()?;
        let mut reply = active.adapter.receive(deadline)?;
        #[cfg(test)]
        active.adapter.lifecycle_reply_received();
        let live = active.adapter.checkpoint(Checkpoint::AfterReply, deadline);
        let selected = match &mut reply {
            AuthorityRead::Missing => live.map(|_| None),
            AuthorityRead::Bytes(bytes) => {
                let result = live.and_then(|_| {
                    crate::x11_credentials::auth::select_cookie(
                        bytes,
                        request.hostname,
                        request.display_number,
                    )
                });
                bytes.fill(0);
                result
            }
        }?;
        // Completion is independently required; a full frame is not a reap.
        loop {
            match active.adapter.observe(&handle) {
                Observation::Reaped => {
                    // Reaper may already have retired this generation. Our
                    // retained stable handle proves the same child was reaped;
                    // never mutate a replacement slot or signal a reusable PID.
                    match owner.observe(id, Observation::Reaped) {
                        Ok(_) | Err(FailureKind::StaleIdentity) => (),
                        Err(kind) => return Err(kind),
                    }
                    active.armed = false;
                    break;
                }
                Observation::NotReady => {
                    active
                        .adapter
                        .checkpoint(Checkpoint::AfterReply, deadline)?;
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(FailureKind::Timeout);
                    }
                    std::thread::sleep(remaining.min(std::time::Duration::from_millis(1)));
                }
                evidence => {
                    owner.observe(id, evidence)?;
                    return Err(FailureKind::ChildSetupFailed);
                }
            }
        }
        active
            .adapter
            .checkpoint(Checkpoint::AfterParse, deadline)?;
        Ok(selected)
    })();
    match work {
        Ok(cookie) => Ok(Acquired {
            cookie,
            cleanup: Cleanup::Reaped,
        }),
        Err(kind) => {
            let cleanup = if active.armed {
                let result = match cancel(active.adapter, owner, id) {
                    Ok(cleanup) => cleanup,
                    Err(FailureKind::StaleIdentity)
                        if active.adapter.observe(&handle) == Observation::Reaped =>
                    {
                        Cleanup::Reaped
                    }
                    Err(_) => Cleanup::Unknown,
                };
                active.armed = false;
                result
            } else {
                Cleanup::Reaped
            };
            Err(Failure { kind, cleanup })
        }
    }
}

/// Identity resolution precedes every signal. Killing is not completion. Keep
/// unknown/pending children in the tracker until positive pidfd wait evidence.
pub(super) fn cancel<A: Adapter, O: Ownership<A::Handle>>(
    adapter: &mut A,
    owner: &O,
    identity: Identity,
) -> Result<Cleanup, FailureKind> {
    let handle = owner.handle(identity)?;
    adapter.revoke_ipc();
    let signal = handle
        .as_ref()
        .map(|h| adapter.kill_pidfd(h))
        .unwrap_or(SignalResult::Failed);
    let cleanup = owner.quarantine(identity, signal)?;
    if let Some(handle) = handle {
        let evidence = adapter.observe(&handle);
        // NotReady must not downgrade a failed signal's Unknown result.
        if evidence == Observation::NotReady {
            return Ok(cleanup);
        }
        match owner.observe(identity, evidence) {
            Ok(result) => return Ok(result),
            // Reaper may have recorded the same stable handle's cached reap.
            Err(FailureKind::StaleIdentity) if evidence == Observation::Reaped => {
                return Ok(Cleanup::Reaped)
            }
            Err(error) => return Err(error),
        }
    }
    Ok(cleanup)
}
