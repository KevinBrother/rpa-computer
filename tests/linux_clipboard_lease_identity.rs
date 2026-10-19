//! External-crate RED gate: the host clipboard connection needs read-only
//! identity access; no X connection, environment override or input is used.
#![cfg(target_os = "linux")]

use rpa_native_input::X11Lease;

#[test]
fn linux_clipboard_can_read_exact_native_lease_identity_without_rearming_it() {
    let nonce = [0xa5; 16];
    let lease = X11Lease::new(17, 29, nonce);
    let clone = lease.clone();
    let identity: (u32, u32, &[u8; 16]) = clone.identity();
    assert_eq!(identity, (17, 29, &nonce));
    lease.revoke();
    assert!(clone.is_revoked());
    assert_eq!(clone.identity(), (17, 29, &nonce));
    assert!(
        clone.clone().is_revoked(),
        "identity access cannot rearm authority"
    );
}
