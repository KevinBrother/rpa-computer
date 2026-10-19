//! Portable native-text planning and mapping-lease policy (no X11 linkage).
//!
//! A lease is never reassigned to another scalar during a session. It can be
//! reused only while the observed mapping is still ours. This bounds native
//! text capacity by the server's vacant keycodes; exhaustion is not permission
//! to overwrite a user's key or silently switch to clipboard input.

use std::ffi::OsStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PolicyError {
    InvalidScalar,
    InvalidMapping,
    MappingExhausted,
    OwnershipLost,
    MappingMismatch,
    KeyStillDown,
}

/// Core and XKB observations of one candidate keycode. `has_actions` also
/// covers non-default behavior: an apparently empty symbol row need not be a
/// safe key to borrow. The native layer additionally retains full XKB metadata.
#[derive(Debug, Clone)]
pub(super) struct MappingRow {
    pub keycode: u8,
    pub core: Vec<u64>,
    pub xkb: Vec<u64>,
    pub is_modifier: bool,
    pub is_pressed: bool,
    pub has_actions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Lease {
    pub keycode: u8,
    pub keysym: u64,
    pub original: Vec<u64>,
    pub installed: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LeaseChoice {
    Reuse(u8),
    Install(Lease),
}

pub(super) fn wayland_session(display: Option<&OsStr>, session_type: Option<&str>) -> bool {
    display.is_some_and(|value| !value.is_empty())
        || session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
}

/// X11's Unicode keysym encoding, not a physical-key lookup. Rust `char`
/// already excludes surrogates and values beyond U+10FFFF. Controls without
/// explicit text semantics must never become arbitrary keyboard actions.
pub(super) fn native_keysym(ch: char) -> Result<u64, PolicyError> {
    match ch {
        '\r' | '\n' => Ok(0xff0d),
        '\t' => Ok(0xff09),
        ' '..='\u{7e}' | '\u{a0}'..='\u{ff}' => Ok(ch as u64),
        '\u{100}'..='\u{10ffff}' => Ok(0x0100_0000 | ch as u64),
        _ => Err(PolicyError::InvalidScalar),
    }
}

fn owns_row(lease: &Lease, row: &MappingRow) -> bool {
    row.keycode == lease.keycode
        && row.core == lease.installed
        && !row.xkb.is_empty()
        && row.xkb.iter().all(|symbol| *symbol == lease.keysym)
        && !row.is_modifier
        && !row.has_actions
}

/// Selects a vacant slot without mutating either the snapshots or the lease
/// set. Commit a new lease only in the native layer, with rollback information
/// retained BEFORE sending a potentially partially accepted map change.
pub(super) fn plan_lease(
    keysym: u64,
    rows: &[MappingRow],
    leases: &[Lease],
) -> Result<LeaseChoice, PolicyError> {
    if keysym == 0 {
        return Err(PolicyError::InvalidScalar);
    }
    let mut seen = [false; 256];
    for row in rows {
        if row.keycode < 8 || row.core.is_empty() || seen[row.keycode as usize] {
            return Err(PolicyError::InvalidMapping);
        }
        seen[row.keycode as usize] = true;
    }
    if let Some(lease) = leases.iter().find(|lease| lease.keysym == keysym) {
        let row = rows
            .iter()
            .find(|row| row.keycode == lease.keycode)
            .ok_or(PolicyError::OwnershipLost)?;
        restoration_allowed(lease, row, false)?;
        return Ok(LeaseChoice::Reuse(lease.keycode));
    }
    let row = rows
        .iter()
        .find(|row| {
            !row.is_modifier
                && !row.is_pressed
                && !row.has_actions
                && row.core.iter().all(|symbol| *symbol == 0)
                && row.xkb.iter().all(|symbol| *symbol == 0)
                && !leases.iter().any(|lease| lease.keycode == row.keycode)
        })
        .ok_or(PolicyError::MappingExhausted)?;
    Ok(LeaseChoice::Install(Lease {
        keycode: row.keycode,
        keysym,
        original: row.core.clone(),
        installed: vec![keysym; row.core.len()],
    }))
}

pub(super) fn validate_effective_keysym(
    requested: u64,
    core: u64,
    xkb: u64,
) -> Result<(), PolicyError> {
    if requested != 0 && requested == core && requested == xkb {
        Ok(())
    } else {
        Err(PolicyError::MappingMismatch)
    }
}

/// Only authorizes a server-side restoration attempt, never asserts a target
/// application's receipt. The native layer must make ownership check + update
/// atomic, verify full XKB metadata, and read back restoration before forgetting
/// the lease. Failed or uncertain key releases prohibit map restoration.
pub(super) fn restoration_allowed(
    lease: &Lease,
    row: &MappingRow,
    pending_owned_release: bool,
) -> Result<(), PolicyError> {
    if !owns_row(lease, row) {
        return Err(PolicyError::OwnershipLost);
    }
    if row.is_pressed || pending_owned_release {
        return Err(PolicyError::KeyStillDown);
    }
    Ok(())
}

#[cfg(test)]
#[path = "text_policy_tests.rs"]
mod tests;

// Linux controls must pass the same neutral-state/pending-release guards as
// other scalars. Explicit physical-key APIs retain their separate semantics.
pub(super) fn control_text_uses_scalar() -> bool {
    true
}

#[cfg(test)]
#[path = "control_text_tests.rs"]
mod control_text_tests;
