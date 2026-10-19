//! Shared credential acquisition with bounded private Linux helper processes.
//! No existing consumer is migrated; raw containment awaits independent review.
#![allow(dead_code)] // Not wired to consumers pending independent native gates.

mod auth;
mod display;
mod ipc;
mod launcher;

#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod display_tests;
#[cfg(test)]
mod ipc_tests;

use std::fmt;
use std::path::Path;
use std::time::Instant;

pub(crate) const MAX_AUTH_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_PATH_BYTES: usize = 4095;
pub(crate) const MAX_HOSTNAME_BYTES: usize = 255;
pub(crate) const MAX_PENDING_CHILDREN: usize = 4;

/// Explicit caller-selected inputs. No environment lookup, fallback pathname,
/// X connection, desktop identity, cancellation flag or authorization token.
/// The caller resolves inherited XAUTHORITY / HOME precedence, without I/O.
pub(crate) struct Request<'a> {
    pub authority_path: Option<&'a Path>,
    pub display_number: &'a str,
    pub hostname: &'a [u8],
}

/// Deliberately has no derived Debug/Display/Clone and never formats its bytes.
pub(crate) struct Cookie([u8; 16]);

impl Cookie {
    pub(crate) fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl Drop for Cookie {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

impl fmt::Debug for Cookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Cookie([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cleanup {
    NotStarted,
    Reaped,
    /// Owned by the shared tracker; termination/reaping is not yet observed.
    Pending,
    /// Signaling/waiting failed or ownership observation is inconclusive.
    /// This still occupies a tracker slot; it is not abandonment or success.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailureKind {
    Unimplemented,
    InvalidRequest,
    Timeout,
    CapacityExhausted,
    LauncherUnavailable,
    ChildSetupFailed,
    ReadFailed,
    NotRegular,
    Oversized,
    Protocol,
    IncompleteReply,
    MalformedAuthority,
    StaleIdentity,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub kind: FailureKind,
    pub cleanup: Cleanup,
}

/// Success requires a complete bounded reply, selected credential policy, and
/// observed/reaped child completion before the original deadline. No child is
/// launched for an absent pathname. Timeout never returns usable bytes.
#[derive(Debug)]
pub(crate) struct Acquired {
    pub cookie: Option<Cookie>,
    pub cleanup: Cleanup,
}

/// Absolute deadline includes all file acquisition and parsing. Does not reset
/// a consumer's budget or claim to contain a subsequent native Xlib constructor.
pub(crate) fn acquire(request: Request<'_>, deadline: Instant) -> Result<Acquired, Failure> {
    let fail = |kind| Failure {
        kind,
        cleanup: Cleanup::NotStarted,
    };
    if Instant::now() >= deadline {
        return Err(fail(FailureKind::Timeout));
    }
    let _number = display::parse_display_number(request.display_number).map_err(fail)?;
    if request.hostname.len() > MAX_HOSTNAME_BYTES {
        return Err(fail(FailureKind::InvalidRequest));
    }
    if let Some(path) = request.authority_path {
        let bytes = path.as_os_str().as_encoded_bytes();
        if bytes.is_empty() || bytes.len() > MAX_PATH_BYTES || bytes.contains(&0) {
            return Err(fail(FailureKind::InvalidRequest));
        }
    }
    if Instant::now() >= deadline {
        return Err(fail(FailureKind::Timeout));
    }
    if request.authority_path.is_none() {
        return Ok(Acquired {
            cookie: None,
            cleanup: Cleanup::NotStarted,
        });
    }
    // Do NOT satisfy this boundary with synchronous reads or an abandoned
    // reader thread. Atomic-pidfd launcher/containment is the next RED gate.
    launcher::acquire(request, deadline)
}
