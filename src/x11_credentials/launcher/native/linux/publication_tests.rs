//! Supplemental production-record/real-FD tests, NOT kernel clone evidence.
//! Fixture writes kernel-output/publication cells of the SAME production types;
//! all adoption/drop, fallback, cancellation and tracker paths are production.
use super::super::super::tracker::{Ownership, Ticket, Tracker};
use super::*;
use crate::x11_credentials::{Cleanup, Request};
use std::os::fd::{AsRawFd, IntoRawFd};
use std::path::Path;
use std::sync::atomic::AtomicIsize;
use std::time::Duration;

struct Fixture {
    native: Native<'static>,
    owner: Arc<Tracker<Handle>>,
    ticket: Ticket,
}
impl Fixture {
    fn new() -> Self {
        let owner = Arc::new(Tracker::new());
        let ticket = owner.reserve_foreground().unwrap();
        let mut native = Native::new();
        native
            .checkpoint(
                Checkpoint::BeforeLaunch,
                Instant::now() + Duration::from_secs(2),
            )
            .unwrap();
        native
            .prepare(&Request {
                authority_path: Some(Path::new("/fixture-not-opened")),
                hostname: b"host",
                display_number: "1",
            })
            .unwrap();
        owner
            .register_attempts(ticket, native.launch_handles().unwrap())
            .unwrap();
        native.launch_registered();
        Self {
            native,
            owner,
            ticket,
        }
    }
    fn operation(&self) -> &Arc<Operation> {
        self.native.operation.as_ref().unwrap()
    }
}
fn publish(process: &process::Process, result: isize, output: i32) {
    unsafe {
        process.output_pointer().write(output);
        // Same width/location and release contract as the raw parent store.
        (&*process.publication_pointer().cast::<AtomicIsize>()).store(result, Ordering::Release);
    }
}
fn open(fd: i32) -> bool {
    unsafe { libc::fcntl(fd, libc::F_GETFD) >= 0 }
}

#[test]
fn negative_returns_never_adopt_or_close_a_positive_rollback_output() {
    for (method, index) in [(Method::Clone3, 0), (Method::Legacy, 1)] {
        let fixture = Fixture::new();
        let operation = fixture.operation();
        operation.enter(Method::Clone3).unwrap();
        if method == Method::Legacy {
            publish(&operation.attempts[0], -(libc::ENOSYS as isize), -1);
            operation.enter(method).unwrap();
        }
        let (sentinel, _peer) = UnixStream::pair().unwrap();
        let raw = sentinel.as_raw_fd();
        publish(&operation.attempts[index], -(libc::ENOMEM as isize), raw);
        let handles = operation.handles();
        assert!(handles[index].published_pid().is_none());
        assert!(handles[index].fd().is_none());
        assert!(operation.finish_no_child());
        drop(handles);
        drop(fixture);
        assert!(
            open(raw),
            "negative clone output may name a reused unrelated fd; never close it"
        );
    }
}

#[test]
fn unpublished_output_neither_adopts_nor_authorizes_fallback_or_no_child() {
    let (sentinel, _peer) = UnixStream::pair().unwrap();
    for output in [-1, sentinel.as_raw_fd()] {
        let fixture = Fixture::new();
        let operation = fixture.operation();
        operation.enter(Method::Clone3).unwrap();
        unsafe {
            operation.attempts[0].output_pointer().write(output);
        }
        assert_eq!(operation.attempts[0].result(), process::UNPUBLISHED);
        assert!(operation.handles()[0].fd().is_none());
        assert!(operation.enter(Method::Legacy).is_err());
        assert!(!operation.finish_no_child());
        assert_eq!(fixture.owner.occupied(), 1);
        drop(fixture);
        assert!(open(sentinel.as_raw_fd()));
    }
}

#[test]
fn cancellation_before_fallback_prevents_entry_with_the_original_record() {
    let mut fixture = Fixture::new();
    let operation = fixture.operation();
    operation.enter(Method::Clone3).unwrap();
    publish(&operation.attempts[0], -(libc::EPERM as isize), -1);
    operation.cancel();
    assert_eq!(operation.enter(Method::Legacy), Err(FailureKind::Timeout));
    assert_eq!(operation.attempts[1].result(), process::UNPUBLISHED);
    assert_eq!(
        fixture.native.open_startup_gate(),
        Err(FailureKind::Timeout)
    );
}

#[test]
fn fallback_entry_before_cancel_stays_owned_and_cannot_admit_or_replay() {
    let mut fixture = Fixture::new();
    let operation = fixture.operation();
    operation.enter(Method::Clone3).unwrap();
    publish(&operation.attempts[0], -(libc::EACCES as isize), -1);
    assert_eq!(operation.enter(Method::Legacy), Ok(1));
    operation.cancel();
    assert!(!operation.finish_no_child());
    assert!(operation.enter(Method::Legacy).is_err());
    assert_eq!(
        fixture.native.open_startup_gate(),
        Err(FailureKind::Timeout)
    );
    fixture.native.retire_foreground();
    fixture.owner.retire_foreground(fixture.ticket).unwrap();
    assert_eq!(
        fixture.owner.occupied(),
        1,
        "unpublished legacy result is not no-child"
    );
}

#[test]
fn terminal_no_fallback_is_explicit_and_cannot_be_reopened() {
    let fixture = Fixture::new();
    let operation = fixture.operation();
    operation.enter(Method::Clone3).unwrap();
    publish(&operation.attempts[0], -(libc::ENOSYS as isize), -1);
    assert!(operation.finish_no_child());
    assert!(operation.enter(Method::Legacy).is_err());
}

#[test]
fn creator_and_reaper_share_one_canonical_owner_before_any_rust_attach() {
    let fixture = Fixture::new();
    let operation = fixture.operation();
    operation.enter(Method::Clone3).unwrap();
    let (fd, _peer) = UnixStream::pair().unwrap();
    let raw = fd.into_raw_fd(); // Only the positively published Process now owns it.
    publish(&operation.attempts[0], 12345, raw);
    let owner = &fixture.owner;
    let ticket = fixture.ticket;
    let (first, second) = std::thread::scope(|scope| {
        let a = scope.spawn(|| owner.attach_attempt(ticket, 12345, 0).unwrap());
        let b = scope.spawn(|| owner.attach_attempt(ticket, 12345, 0).unwrap());
        (a.join().unwrap(), b.join().unwrap())
    });
    let first = fixture.owner.handle(first).unwrap().unwrap();
    let second = fixture.owner.handle(second).unwrap().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(first.fd().unwrap().as_raw_fd(), raw);
    drop(fixture);
    assert!(open(raw), "live canonical borrowers must keep the fd valid");
    drop(first);
    assert!(open(raw));
    drop(second);
    assert!(!open(raw), "final canonical owner closes once");
}

#[test]
fn positive_result_without_output_remains_unknown_and_never_falls_back() {
    let mut fixture = Fixture::new();
    fixture.operation().enter(Method::Clone3).unwrap();
    publish(&fixture.operation().attempts[0], 12345, -1);
    assert!(fixture.operation().enter(Method::Legacy).is_err());
    let id = fixture
        .owner
        .attach_attempt(fixture.ticket, 12345, 0)
        .unwrap();
    let handle = fixture.owner.handle(id).unwrap().unwrap();
    assert!(!handle.valid());
    assert_eq!(handle.kill(), SignalResult::Failed);
    assert_eq!(
        fixture.owner.observe(id, Observation::Failed),
        Ok(Cleanup::Unknown)
    );
    fixture.native.retire_foreground();
    fixture.owner.retire_foreground(fixture.ticket).unwrap();
    assert_eq!(fixture.owner.occupied(), 1);
}

#[test]
fn fallback_and_cancel_compete_on_the_same_production_entry_word() {
    let mut fixture = Fixture::new();
    let operation = fixture.operation();
    operation.enter(Method::Clone3).unwrap();
    publish(&operation.attempts[0], -(libc::ENOSYS as isize), -1);
    let barrier = std::sync::Barrier::new(3);
    let entry = std::thread::scope(|scope| {
        let cancel = scope.spawn(|| {
            barrier.wait();
            operation.cancel();
        });
        let enter = scope.spawn(|| {
            barrier.wait();
            operation.enter(Method::Legacy)
        });
        barrier.wait();
        cancel.join().unwrap();
        enter.join().unwrap()
    });
    match entry {
        Ok(1) => assert!(
            !operation.finish_no_child(),
            "entered, unpublished legacy remains owned"
        ),
        Err(FailureKind::Timeout) => assert!(
            operation.finish_no_child(),
            "cancel won before any legacy syscall"
        ),
        other => panic!("unexpected linearized entry outcome: {other:?}"),
    }
    assert_eq!(operation.attempts[1].result(), process::UNPUBLISHED);
    assert_eq!(
        fixture.native.open_startup_gate(),
        Err(FailureKind::Timeout)
    );
}
