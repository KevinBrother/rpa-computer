//! Scripted parent-policy assertions. No OS/process behavior is simulated as
//! evidence: tests call the SAME policy::run boundary as public acquisition.
use super::policy::*;
use super::tracker::{Identity, Ownership, Ticket};
use crate::x11_credentials::{
    ipc::AuthorityRead, Acquired, Cleanup, Failure, FailureKind, Request,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Handle(u64);
type Trace = Rc<RefCell<Vec<&'static str>>>;
struct Script {
    trace: Trace,
    spawns: VecDeque<Spawn<Handle>>,
    timeout: Option<Checkpoint>,
    invalid_handle: bool,
    signal: SignalResult,
    observation: Observation,
}
impl Adapter for Script {
    type Handle = Handle;
    type Mask = [u8; 8];
    fn prepare(&mut self, _: &Request<'_>) -> Result<(), FailureKind> {
        self.trace.borrow_mut().push("prepare");
        Ok(())
    }
    fn checkpoint(&mut self, point: Checkpoint, _: Instant) -> Result<(), FailureKind> {
        if self.timeout == Some(point) {
            Err(FailureKind::Timeout)
        } else {
            Ok(())
        }
    }
    fn block_signals(&mut self) -> Result<Self::Mask, FailureKind> {
        self.trace.borrow_mut().push("block");
        Ok([0xa5; 8])
    }
    fn restore_signals(&mut self, mask: Self::Mask) -> Result<(), FailureKind> {
        assert_eq!(mask, [0xa5; 8], "restore the exact inherited parent mask");
        self.trace.borrow_mut().push("restore");
        Ok(())
    }
    fn clone3(&mut self) -> Spawn<Handle> {
        self.trace.borrow_mut().push("clone3");
        self.spawns.pop_front().expect("no extra spawn")
    }
    fn legacy_clone_pidfd(&mut self) -> Spawn<Handle> {
        self.trace.borrow_mut().push("legacy");
        self.spawns.pop_front().expect("one fallback only")
    }
    fn validate_pidfd(&mut self, handle: &Handle) -> Result<(), FailureKind> {
        assert_ne!(handle.0, 0);
        self.trace.borrow_mut().push("validate");
        if self.invalid_handle {
            Err(FailureKind::StaleIdentity)
        } else {
            Ok(())
        }
    }
    fn open_startup_gate(&mut self) -> Result<(), FailureKind> {
        self.trace.borrow_mut().push("gate");
        Ok(())
    }
    fn revoke_ipc(&mut self) {
        self.trace.borrow_mut().push("revoke");
    }
    fn receive(&mut self, _: Instant) -> Result<AuthorityRead, FailureKind> {
        self.trace.borrow_mut().push("receive");
        // Binary authority bytes parsed by production parent auth policy.
        let mut body = 256u16.to_be_bytes().to_vec();
        for field in [
            &b"host"[..],
            &b"100001"[..],
            &b"MIT-MAGIC-COOKIE-1"[..],
            &[0x42; 16][..],
        ] {
            body.extend_from_slice(&(field.len() as u16).to_be_bytes());
            body.extend_from_slice(field);
        }
        Ok(AuthorityRead::Bytes(body))
    }
    fn kill_pidfd(&mut self, _: &Handle) -> SignalResult {
        self.trace.borrow_mut().push("kill-pidfd");
        self.signal
    }
    fn observe(&mut self, _: &Handle) -> Observation {
        self.trace.borrow_mut().push("observe");
        self.observation
    }
}
/// Scripted metadata adapter isolates launcher traces from the separately RED
/// real tracker. Production run uses Tracker, never this scripted implementation.
struct Owner {
    trace: Trace,
    capacity_full: bool,
    reservations: Cell<usize>,
    handle: RefCell<Option<Arc<Handle>>>,
    stale: bool,
}
impl Ownership<Handle> for Owner {
    fn reserve(&self) -> Result<Ticket, FailureKind> {
        self.trace.borrow_mut().push("reserve");
        if self.capacity_full {
            return Err(FailureKind::CapacityExhausted);
        }
        self.reservations.set(self.reservations.get() + 1);
        Ok(Ticket {
            slot: 0,
            generation: 1,
        })
    }
    fn attach(
        &self,
        ticket: Ticket,
        pid: i32,
        handle: Option<Handle>,
    ) -> Result<Identity, FailureKind> {
        self.trace.borrow_mut().push("attach");
        *self.handle.borrow_mut() = handle.map(Arc::new);
        Ok(Identity { ticket, pid })
    }
    fn unstarted(&self, _: Ticket) -> Result<(), FailureKind> {
        self.trace.borrow_mut().push("unstarted");
        Ok(())
    }
    fn handle(&self, _: Identity) -> Result<Option<Arc<Handle>>, FailureKind> {
        self.trace.borrow_mut().push("identity");
        if self.stale {
            Err(FailureKind::StaleIdentity)
        } else {
            Ok(self.handle.borrow().clone())
        }
    }
    fn quarantine(&self, _: Identity, signal: SignalResult) -> Result<Cleanup, FailureKind> {
        self.trace.borrow_mut().push("quarantine");
        Ok(if signal == SignalResult::Failed {
            Cleanup::Unknown
        } else {
            Cleanup::Pending
        })
    }
    fn observe(&self, _: Identity, evidence: Observation) -> Result<Cleanup, FailureKind> {
        self.trace.borrow_mut().push("record-observation");
        Ok(match evidence {
            Observation::Reaped => Cleanup::Reaped,
            Observation::NotReady => Cleanup::Pending,
            _ => Cleanup::Unknown,
        })
    }
    fn occupied(&self) -> usize {
        self.reservations.get()
    }
}
fn good() -> Spawn<Handle> {
    Spawn::Parent {
        pid: 1234,
        pidfd: Some(Handle(99)),
    }
}
fn fixture(spawns: Vec<Spawn<Handle>>) -> (Script, Owner) {
    let trace = Rc::new(RefCell::new(Vec::new()));
    (
        Script {
            trace: trace.clone(),
            spawns: spawns.into(),
            timeout: None,
            invalid_handle: false,
            signal: SignalResult::Requested,
            observation: Observation::Reaped,
        },
        Owner {
            trace,
            capacity_full: false,
            reservations: Cell::new(0),
            handle: RefCell::new(None),
            stale: false,
        },
    )
}
fn run(script: &mut Script, owner: &Owner) -> Result<Acquired, Failure> {
    super::policy::run(
        script,
        owner,
        Request {
            authority_path: Some(Path::new("/synthetic-not-opened")),
            hostname: b"host",
            display_number: "100001",
        },
        Instant::now() + Duration::from_secs(1),
    )
}
fn sequence(trace: &Trace, expected: &[&str]) {
    let calls = trace.borrow();
    let mut offset = 0;
    for step in expected {
        offset += calls[offset..]
            .iter()
            .position(|call| call == step)
            .unwrap_or_else(|| panic!("missing ordered step {step}; actual {calls:?}"))
            + 1;
    }
}
fn count(trace: &Trace, call: &str) -> usize {
    trace.borrow().iter().filter(|&&c| c == call).count()
}

#[test]
fn clone3_success_never_attempts_legacy_and_restores_parent_mask() {
    let (mut s, o) = fixture(vec![good()]);
    let outcome = run(&mut s, &o).expect("valid atomic pidfd launch must reach parent acceptance");
    assert_eq!(outcome.cleanup, Cleanup::Reaped);
    assert_eq!(outcome.cookie.unwrap().as_bytes(), &[0x42; 16]);
    sequence(
        &s.trace,
        &[
            "reserve", "prepare", "block", "clone3", "restore", "validate", "gate", "receive",
            "observe",
        ],
    );
    assert_eq!(count(&s.trace, "legacy"), 0);
}
#[test]
fn definitive_denial_tries_legacy_once_with_original_reservation() {
    for errno in [Errno::Enosys, Errno::Eperm, Errno::Eacces] {
        let (mut s, o) = fixture(vec![Spawn::NoChild(errno), good()]);
        assert_eq!(run(&mut s, &o).unwrap().cleanup, Cleanup::Reaped);
        sequence(
            &s.trace,
            &[
                "reserve", "prepare", "block", "clone3", "legacy", "restore", "validate", "gate",
            ],
        );
        assert_eq!(o.reservations.get(), 1);
        assert_eq!(count(&s.trace, "legacy"), 1);
    }
}
#[test]
fn resource_or_argument_failure_does_not_retry() {
    for errno in [Errno::Eagain, Errno::Enomem, Errno::Einval] {
        let (mut s, o) = fixture(vec![Spawn::NoChild(errno)]);
        assert!(run(&mut s, &o).is_err());
        sequence(&s.trace, &["block", "clone3", "restore", "unstarted"]);
        assert_eq!(count(&s.trace, "legacy"), 0);
        assert_eq!(count(&s.trace, "gate"), 0);
    }
}
#[test]
fn expired_before_fallback_prevents_spawn_and_restores_mask() {
    let (mut s, o) = fixture(vec![Spawn::NoChild(Errno::Enosys)]);
    s.timeout = Some(Checkpoint::BeforeFallback);
    assert_eq!(
        run(&mut s, &o).unwrap_err(),
        Failure {
            kind: FailureKind::Timeout,
            cleanup: Cleanup::NotStarted
        }
    );
    sequence(&s.trace, &["clone3", "restore", "unstarted"]);
    assert_eq!(count(&s.trace, "legacy"), 0);
}
#[test]
fn both_denied_are_one_bounded_failure_without_third_spawn() {
    let (mut s, o) = fixture(vec![
        Spawn::NoChild(Errno::Eperm),
        Spawn::NoChild(Errno::Eacces),
    ]);
    assert!(run(&mut s, &o).is_err());
    sequence(&s.trace, &["clone3", "legacy", "restore", "unstarted"]);
    assert_eq!(count(&s.trace, "clone3") + count(&s.trace, "legacy"), 2);
}
#[test]
fn child_pid_without_atomic_handle_never_retries_or_opens_gate() {
    let (mut s, o) = fixture(vec![Spawn::Parent {
        pid: 1234,
        pidfd: None,
    }]);
    let failure = run(&mut s, &o).unwrap_err();
    assert_eq!(failure.cleanup, Cleanup::Unknown);
    sequence(
        &s.trace,
        &["clone3", "restore", "attach", "revoke", "quarantine"],
    );
    for forbidden in ["legacy", "gate", "kill-pidfd", "receive"] {
        assert_eq!(count(&s.trace, forbidden), 0);
    }
}
#[test]
fn exhausted_capacity_precedes_prepare_mask_and_spawn() {
    let (mut s, mut o) = fixture(vec![]);
    o.capacity_full = true;
    assert_eq!(
        run(&mut s, &o).unwrap_err().kind,
        FailureKind::CapacityExhausted
    );
    assert_eq!(&*s.trace.borrow(), &["reserve"]);
}
#[test]
fn invalid_handle_never_releases_startup_gate() {
    let (mut s, o) = fixture(vec![good()]);
    s.invalid_handle = true;
    assert_eq!(run(&mut s, &o).unwrap_err().cleanup, Cleanup::Unknown);
    sequence(&s.trace, &["validate", "revoke", "quarantine"]);
    assert_eq!(count(&s.trace, "gate"), 0);
    assert_eq!(count(&s.trace, "kill-pidfd"), 0);
}
#[test]
fn deadline_after_handle_validation_still_prevents_startup() {
    let (mut s, o) = fixture(vec![good()]);
    s.timeout = Some(Checkpoint::BeforeGate);
    s.observation = Observation::NotReady;
    assert_eq!(run(&mut s, &o).unwrap_err().kind, FailureKind::Timeout);
    sequence(
        &s.trace,
        &["validate", "revoke", "kill-pidfd", "quarantine"],
    );
    assert_eq!(count(&s.trace, "gate"), 0);
}
#[test]
fn timeout_after_full_reply_or_after_parsing_never_returns_cookie() {
    for point in [Checkpoint::AfterReply, Checkpoint::AfterParse] {
        let (mut s, o) = fixture(vec![good()]);
        s.timeout = Some(point);
        s.observation = if point == Checkpoint::AfterParse {
            Observation::Reaped
        } else {
            Observation::NotReady
        };
        assert_eq!(run(&mut s, &o).unwrap_err().kind, FailureKind::Timeout);
        if point == Checkpoint::AfterReply {
            sequence(&s.trace, &["receive", "revoke", "kill-pidfd", "quarantine"]);
        } else {
            sequence(&s.trace, &["receive", "observe"]);
        }
    }
}
#[test]
fn kill_success_is_pending_not_reaped_and_kill_error_is_unknown() {
    for (signal, expected) in [
        (SignalResult::Requested, Cleanup::Pending),
        (SignalResult::Failed, Cleanup::Unknown),
    ] {
        let (mut s, o) = fixture(vec![]);
        s.signal = signal;
        s.observation = Observation::NotReady;
        *o.handle.borrow_mut() = Some(Arc::new(Handle(99)));
        let identity = Identity {
            ticket: Ticket {
                slot: 0,
                generation: 1,
            },
            pid: 1234,
        };
        assert_eq!(
            super::policy::cancel(&mut s, &o, identity).unwrap(),
            expected
        );
        sequence(
            &s.trace,
            &["identity", "revoke", "kill-pidfd", "quarantine"],
        );
    }
}
#[test]
fn stale_identity_cannot_signal_a_reused_pid() {
    let (mut s, mut o) = fixture(vec![]);
    o.stale = true;
    let identity = Identity {
        ticket: Ticket {
            slot: 0,
            generation: 1,
        },
        pid: 1234,
    };
    assert_eq!(
        super::policy::cancel(&mut s, &o, identity),
        Err(FailureKind::StaleIdentity)
    );
    assert_eq!(&*s.trace.borrow(), &["identity"]);
}
