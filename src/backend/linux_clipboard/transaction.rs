//! Short same-connection authority/mutation transaction. No event loop work here.
use super::policy::{error, GuardSnapshot};
use crate::backend::BackendError;
use rpa_native_input::X11Lease;
use std::time::Instant;

#[derive(Clone, Debug)]
pub(super) struct AuthoritySnapshot {
    pub canonical_selection: u32,
    pub selection_owner: u32,
    pub nonce_is_string: bool,
    pub nonce_format: u8,
    pub nonce: Vec<u8>,
    pub nonce_bytes_after: u32,
}
pub(super) fn denied() -> BackendError {
    BackendError::new(
        "permission_denied",
        "explicit Linux paste requires a live verified X11 lease",
    )
}
pub(super) fn validate(lease: &X11Lease, actual: &AuthoritySnapshot) -> Result<(), BackendError> {
    let (selection, owner, nonce) = lease.identity();
    if lease.is_revoked()
        || selection == 0
        || owner == 0
        || actual.canonical_selection != selection
        || actual.selection_owner != owner
        || !actual.nonce_is_string
        || actual.nonce_format != 8
        || actual.nonce_bytes_after != 0
        || actual.nonce.as_slice() != nonce
    {
        lease.revoke();
        return Err(denied());
    }
    Ok(())
}
pub(super) trait PrepareIo {
    fn cancelled(&self) -> bool;
    fn grab(&mut self, deadline: Instant) -> Result<(), BackendError>;
    fn authority(&mut self, deadline: Instant) -> Result<AuthoritySnapshot, BackendError>;
    fn guards(&mut self, deadline: Instant) -> Result<GuardSnapshot, BackendError>;
    fn publish_clipboard(
        &mut self,
        bytes: &[u8],
        deadline: Instant,
    ) -> Result<GuardSnapshot, BackendError>;
    fn ungrab(&mut self) -> Result<(), BackendError>;
}
fn live(io: &impl PrepareIo, lease: &X11Lease, deadline: Instant) -> Result<(), BackendError> {
    if lease.is_revoked() {
        return Err(denied());
    }
    if io.cancelled() || Instant::now() >= deadline {
        return Err(error("paste preparation cancelled or expired"));
    }
    Ok(())
}
pub(super) fn combine<T>(
    result: Result<T, BackendError>,
    cleanup: Result<(), BackendError>,
) -> Result<T, BackendError> {
    match (result, cleanup) {
        (result, Ok(())) => result,
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => {
            Err(error(format!("{}; {}", primary.message, cleanup.message)))
        }
    }
}
pub(super) fn prepare(
    io: &mut impl PrepareIo,
    lease: &X11Lease,
    bytes: &[u8],
    deadline: Instant,
) -> Result<GuardSnapshot, BackendError> {
    live(io, lease, deadline)?;
    io.grab(deadline)?;
    let mut mutation_attempted = false;
    let result = (|| {
        let authority = io.authority(deadline).inspect_err(|_| lease.revoke())?;
        validate(lease, &authority)?;
        live(io, lease, deadline)?;
        let before = io.guards(deadline)?;
        before.before_publish()?;
        live(io, lease, deadline)?;
        mutation_attempted = true;
        let after = io.publish_clipboard(bytes, deadline)?;
        live(io, lease, deadline)?;
        if after.focus != before.focus || after.blocked != before.blocked {
            return Err(error("paste target/input changed during publication"));
        }
        Ok(after)
    })();
    let cleanup = io.ungrab();
    combine(result, cleanup).map_err(|e| {
        if mutation_attempted {
            error(format!(
                "{}; clipboard may already be modified; no automatic retry",
                e.message
            ))
        } else {
            e
        }
    })
}
