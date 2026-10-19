//! Portable token contract tests. No X11 linkage, environment or native input.
use crate::{X11Lease, X11_LEASE_NONCE_PROPERTY, X11_LEASE_SELECTION};

#[test]
fn x11_lease_revocation_is_shared_by_existing_and_future_clones() {
    let lease = X11Lease::new(41, 99, [7; 16]);
    let clone = lease.clone();
    assert!(!lease.is_revoked());
    assert!(!clone.is_revoked());
    clone.revoke();
    assert!(lease.is_revoked());
    assert!(clone.is_revoked());
    assert!(lease.clone().is_revoked());
}

#[test]
fn x11_lease_repeated_revocation_never_rearms_the_token() {
    let lease = X11Lease::new(41, 99, [7; 16]);
    for _ in 0..3 {
        lease.revoke();
        assert!(lease.is_revoked());
    }
}

#[test]
fn x11_lease_monitor_revocation_is_visible_to_the_native_thread() {
    let lease = X11Lease::new(41, 99, [7; 16]);
    let monitor = lease.clone();
    std::thread::spawn(move || monitor.revoke()).join().unwrap();
    assert!(lease.is_revoked());
}

#[test]
fn x11_lease_independent_sessions_do_not_share_revocation_state() {
    let first = X11Lease::new(41, 99, [7; 16]);
    let next = X11Lease::new(41, 100, [8; 16]);
    first.revoke();
    assert!(first.is_revoked());
    assert!(!next.is_revoked());
}

#[test]
fn x11_lease_property_names_match_the_shared_desktop_authority_contract() {
    assert_eq!(X11_LEASE_SELECTION, "_RPA_COMPUTER_DESKTOP_WRITER_V1");
    assert_eq!(X11_LEASE_NONCE_PROPERTY, "_RPA_COMPUTER_DESKTOP_WRITER_NONCE_V1");
}
