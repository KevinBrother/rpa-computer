//! Production preparation orchestration against a recording connection. This
//! proves ordering/early exits, NOT real XCB locking or socket cancellation.
use super::policy::{BlockedKeys, GuardSnapshot};
use super::transaction::{prepare, AuthoritySnapshot, PrepareIo};
use crate::backend::BackendError;
use rpa_native_input::X11Lease;
use std::time::{Duration, Instant};

struct Connection {
    calls: Vec<&'static str>,
    grabbed: bool,
    authority: AuthoritySnapshot,
    snapshot: GuardSnapshot,
    cancelled: bool,
    revoke_during_probe: Option<X11Lease>,
    cancel_during_probe: bool,
    cancel_during_guards: bool,
    fail_publish: bool,
    fail_release: bool,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            calls: vec![],
            grabbed: false,
            authority: AuthoritySnapshot {
                canonical_selection: 10,
                selection_owner: 20,
                nonce_is_string: true,
                nonce_format: 8,
                nonce: vec![7; 16],
                nonce_bytes_after: 0,
            },
            snapshot: GuardSnapshot {
                focus: 42,
                clipboard_owner: 90,
                owner_generation: 1,
                blocked: BlockedKeys::default(),
            },
            cancelled: false,
            revoke_during_probe: None,
            cancel_during_probe: false,
            cancel_during_guards: false,
            fail_publish: false,
            fail_release: false,
        }
    }
}
impl PrepareIo for Connection {
    fn cancelled(&self) -> bool {
        self.cancelled
    }
    fn grab(&mut self, deadline: Instant) -> Result<(), BackendError> {
        assert!(deadline > Instant::now());
        assert!(!self.grabbed);
        self.calls.push("grab");
        self.grabbed = true;
        Ok(())
    }
    fn authority(&mut self, _: Instant) -> Result<AuthoritySnapshot, BackendError> {
        assert!(self.grabbed);
        self.calls.push("authority");
        if let Some(token) = &self.revoke_during_probe {
            token.revoke();
        }
        self.cancelled |= self.cancel_during_probe;
        Ok(self.authority.clone())
    }
    fn guards(&mut self, _: Instant) -> Result<GuardSnapshot, BackendError> {
        assert!(self.grabbed);
        self.calls.push("guards");
        self.cancelled |= self.cancel_during_guards;
        Ok(self.snapshot)
    }
    fn publish_clipboard(
        &mut self,
        bytes: &[u8],
        _: Instant,
    ) -> Result<GuardSnapshot, BackendError> {
        assert!(self.grabbed, "same-connection grab must enclose mutation");
        assert_eq!(bytes, b"payload");
        self.calls.push("publish:CLIPBOARD");
        if self.fail_publish {
            return Err(BackendError::new(
                "input_failed",
                "selection write uncertain",
            ));
        }
        self.snapshot.clipboard_owner = 80;
        self.snapshot.owner_generation += 1;
        Ok(self.snapshot)
    }
    fn ungrab(&mut self) -> Result<(), BackendError> {
        assert!(self.grabbed);
        self.calls.push("ungrab");
        self.grabbed = false;
        if self.fail_release {
            Err(BackendError::new("input_failed", "ungrab uncertain"))
        } else {
            Ok(())
        }
    }
}
fn lease() -> X11Lease {
    X11Lease::new(10, 20, [7; 16])
}
fn run(io: &mut Connection, lease: &X11Lease) -> Result<GuardSnapshot, BackendError> {
    prepare(
        io,
        lease,
        b"payload",
        Instant::now() + Duration::from_millis(250),
    )
}

#[test]
fn canonical_authority_and_focus_precede_mutation_in_one_short_connection_grab() {
    let mut io = Connection::default();
    let result = run(&mut io, &lease()).unwrap();
    assert_eq!(result.clipboard_owner, 80);
    assert_eq!(
        io.calls,
        ["grab", "authority", "guards", "publish:CLIPBOARD", "ungrab"]
    );
    assert!(!io.grabbed);
    // No old data read, PRIMARY write, focus/layout operation, key or retry API.
}

#[test]
fn every_authority_mismatch_revokes_shared_token_without_clipboard_mutation() {
    for case in 0..8 {
        let mut io = Connection::default();
        let token = lease();
        let clone = token.clone();
        match case {
            0 => io.authority.canonical_selection = 99,
            1 => io.authority.selection_owner = 21,
            2 => io.authority.selection_owner = 0,
            3 => io.authority.nonce[0] ^= 1,
            4 => io.authority.nonce.pop().map(|_| ()).unwrap(),
            5 => io.authority.nonce_format = 32,
            6 => io.authority.nonce_is_string = false,
            7 => io.authority.nonce_bytes_after = 1,
            _ => unreachable!(),
        }
        assert!(run(&mut io, &token).is_err(), "case {case}");
        assert!(clone.is_revoked(), "case {case}");
        assert_eq!(io.calls, ["grab", "authority", "ungrab"]);
        assert!(!io.grabbed);
        io.calls.clear();
        assert!(run(&mut io, &clone).is_err());
        assert!(
            io.calls.is_empty(),
            "irreversible revocation must fail before I/O"
        );
    }
}

#[test]
fn revoked_cancelled_and_expired_operations_never_grab_or_publish() {
    let mut io = Connection::default();
    let token = lease();
    token.revoke();
    assert!(run(&mut io, &token).is_err());
    assert!(io.calls.is_empty());
    io.cancelled = true;
    assert!(run(&mut io, &lease()).is_err());
    assert!(io.calls.is_empty());
    io.cancelled = false;
    assert!(prepare(&mut io, &lease(), b"payload", Instant::now()).is_err());
    assert!(io.calls.is_empty());
}

#[test]
fn revocation_during_authority_probe_is_rechecked_before_shared_mutation() {
    let token = lease();
    let mut io = Connection {
        revoke_during_probe: Some(token.clone()),
        ..Connection::default()
    };
    assert!(run(&mut io, &token).is_err());
    assert!(!io.calls.contains(&"publish:CLIPBOARD"));
    assert_eq!(io.calls.last(), Some(&"ungrab"));
    assert!(!io.grabbed);
}

#[test]
fn preheld_key_and_missing_focus_fail_before_publication_and_release_grab() {
    for focus_missing in [false, true] {
        let mut io = Connection::default();
        if focus_missing {
            io.snapshot.focus = 0;
        } else {
            io.snapshot.blocked.v = true;
        }
        let token = lease();
        assert!(run(&mut io, &token).is_err());
        assert_eq!(io.calls, ["grab", "authority", "guards", "ungrab"]);
        assert!(
            !token.is_revoked(),
            "focus/input contention is not authority loss"
        );
    }
}

#[test]
fn publish_failure_is_not_retried_or_restored_and_ungrab_failure_is_reported() {
    let mut io = Connection {
        fail_publish: true,
        fail_release: true,
        ..Connection::default()
    };
    let error = run(&mut io, &lease()).unwrap_err();
    assert!(error.message.contains("selection write uncertain"));
    assert!(error.message.contains("ungrab uncertain"));
    assert!(error.message.contains("clipboard"));
    assert_eq!(
        io.calls,
        ["grab", "authority", "guards", "publish:CLIPBOARD", "ungrab"]
    );
    assert!(!io.grabbed);
}

#[test]
fn cancellation_during_a_roundtrip_cannot_late_publish_after_reply_arrives() {
    for during_probe in [false, true] {
        let mut io = Connection {
            cancel_during_probe: during_probe,
            cancel_during_guards: !during_probe,
            ..Connection::default()
        };
        assert!(run(&mut io, &lease()).is_err());
        assert!(!io.calls.contains(&"publish:CLIPBOARD"));
        assert_eq!(io.calls.last(), Some(&"ungrab"));
        assert!(!io.grabbed);
    }
}
