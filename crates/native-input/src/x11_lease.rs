//! Portable desktop-authority token. It carries identity, not Xlib state and
//! not a process-global authorization flag. Only clones of one session share
//! revocation; once revoked, that token can never be rearmed.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const X11_LEASE_SELECTION: &str = "_RPA_COMPUTER_DESKTOP_WRITER_V1";
pub const X11_LEASE_NONCE_PROPERTY: &str = "_RPA_COMPUTER_DESKTOP_WRITER_NONCE_V1";

#[derive(Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct X11Lease {
    selection_atom: u32,
    owner_window: u32,
    nonce: [u8; 16],
    revoked: Arc<AtomicBool>,
}
impl X11Lease {
    /// Construct an identity token for an already established desktop lease.
    /// This does not establish authority: the native Xlib connection verifies
    /// canonical selection identity, owner and nonce before every operation.
    pub fn new(selection_atom: u32, owner_window: u32, nonce: [u8; 16]) -> Self {
        Self {
            selection_atom,
            owner_window,
            nonce,
            revoked: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }
    pub fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }

    /// Read-only server identity for verification on another X11 connection.
    /// Identity alone is not authorization: consumers must check revocation
    /// and atom/owner/nonce atomically on their own connection before writing.
    #[cfg(target_os = "linux")]
    pub fn identity(&self) -> (u32, u32, &[u8; 16]) {
        (self.selection_atom, self.owner_window, &self.nonce)
    }
}
