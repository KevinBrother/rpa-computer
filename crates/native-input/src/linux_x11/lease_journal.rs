//! Actual native map lease journal and request/readback ordering, without FFI.
//! Writes only a pinned verified XTEST slave; readback checks each device role.
//! Per-key confirmation is NOT whole-master projection/residue proof (C).
#[cfg(target_os = "linux")]
use super::super::{definition::Definition, device_policy, inventory, text_policy};
use super::{
    dispatch_after_final_fence_on, target_identity, with_snapshot, MutationKind, NativeMapRequest,
};
#[cfg(not(target_os = "linux"))]
use crate::{
    linux_x11_definition::Definition, linux_x11_device_policy as device_policy,
    linux_x11_inventory as inventory, linux_x11_text_policy as text_policy,
};
use crate::{InputError, Result};
use device_policy::DeviceMap;
use inventory::{require_same_lifetime, Identity};
use text_policy::{Lease, MappingRow, PolicyError};

#[path = "map_groups.rs"]
pub(crate) mod map_groups;

/// Confirmation here is per-key server observation only: not full projection
/// safety, not an accepted scalar, and not application receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Prepared,
    InstallMayHaveApplied,
    InstalledConfirmed,
    FullRestoreMayHaveApplied,
    GuardedConfirmed,
    FinalizeMayHaveApplied,
}
#[derive(Clone)]
pub(crate) struct SavedKey {
    device_id: u16,
    original: Definition,
    original_core: Vec<u64>,
}
#[derive(Clone)]
pub(crate) struct NativeLease {
    pub epoch: u64,
    pub policy: Lease,
    pub identities: Vec<Identity>,
    saved: Vec<SavedKey>,
    installed: Definition,
    target: Option<target_identity::VerifiedTarget>,
    phase: Phase,
}
impl NativeLease {
    pub fn input_target(&self) -> Option<target_identity::VerifiedTarget> {
        self.target.clone()
    }
    pub fn is_uncertain(&self) -> bool {
        self.phase != Phase::InstalledConfirmed
    }
    #[cfg(test)]
    pub fn phase(&self) -> &Phase {
        &self.phase
    }
    fn restoration_started(&self) -> bool {
        matches!(
            self.phase,
            Phase::FullRestoreMayHaveApplied
                | Phase::GuardedConfirmed
                | Phase::FinalizeMayHaveApplied
        )
    }
}
#[derive(Clone, Copy)]
pub(crate) struct View<'a> {
    pub epoch: u64,
    pub identities: &'a [Identity],
    pub maps: &'a [DeviceMap],
}
/// The adapter supplies observations/IO, NOT journal decisions or request routing.
/// Production and recording transports share prepare/install/restore below.
pub(crate) trait Transport {
    type State;
    type Descriptor;
    fn leases(&self) -> &[NativeLease];
    fn leases_mut(&mut self) -> &mut Vec<NativeLease>;
    fn observe(&mut self) -> Result<Self::State>;
    fn view(state: &Self::State) -> View<'_>;
    /// Owned full-range observations from this State, never a default success.
    fn group_observations(state: &Self::State) -> Result<map_groups::GroupState>;
    fn descriptors(state: &mut Self::State) -> &mut [Self::Descriptor];
    fn targets(&mut self, state: &Self::State) -> Result<target_identity::Inventory>;
    fn fence(&mut self, epoch: u64, expected: &[Identity], observed: &[Identity]) -> Result<()>;
    fn pending_release(&self, key: u8) -> bool;
    fn require_one_level(&self, state: &Self::State) -> Result<()>;
    /// Raw translation observation only; the shared journal owns safety checks.
    fn effective_symbol(&self, state: &Self::State, device: u16, key: u8) -> Result<u64>;
    fn write_full(
        &mut self,
        descriptor: &mut Self::Descriptor,
        request: NativeMapRequest,
        value: &Definition,
    ) -> Result<()>;
    fn write_explicit(
        &mut self,
        descriptor: &mut Self::Descriptor,
        request: NativeMapRequest,
        flags: u8,
    ) -> Result<()>;
}

pub(crate) fn policy_error(error: PolicyError) -> InputError {
    let (code, message) = match error {
        PolicyError::InvalidScalar => ("invalid_text", "scalar has no native text keysym"),
        PolicyError::InvalidMapping => ("input_failed", "invalid keyboard map geometry"),
        PolicyError::MappingExhausted => ("invalid_text", "native text mapping capacity exhausted: no vacant compatible keycode across affected keyboards (explicit clipboard paste is separate)"),
        PolicyError::OwnershipLost => ("input_failed", "native text mapping/device/repeat ownership lost; foreign state was not overwritten"),
        PolicyError::MappingMismatch => ("invalid_text", "native text core/XKB mapping does not resolve to the requested scalar"),
        PolicyError::KeyStillDown => ("input_failed", "native text key release unconfirmed; mapping must not be restored"),
    };
    InputError::new(code, message)
}
pub(crate) fn observed(
    view: View<'_>,
    index: usize,
    key: u8,
) -> Result<(&MappingRow, &Definition)> {
    let device = view.maps.get(index).ok_or_else(ownership_lost)?;
    let slot = device
        .rows
        .iter()
        .position(|row| row.keycode == key)
        .ok_or_else(|| policy_error(PolicyError::OwnershipLost))?;
    Ok((
        &device.rows[slot],
        device.definitions.get(slot).ok_or_else(ownership_lost)?,
    ))
}
pub(crate) fn prepare(view: View<'_>, policy: Lease) -> Result<NativeLease> {
    let saved = (0..view.maps.len())
        .map(|index| {
            let (row, definition) = observed(view, index, policy.keycode)?;
            Ok(SavedKey {
                device_id: view.maps[index].device_id,
                original: definition.clone(),
                original_core: row.core.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    validate_devices(view)?;
    let installed = Definition::for_symbol(policy.keysym as _);
    Ok(NativeLease {
        epoch: view.epoch,
        policy,
        identities: view.identities.to_vec(),
        saved,
        installed,
        target: None,
        phase: Phase::Prepared,
    })
}
fn ownership_lost() -> InputError {
    policy_error(PolicyError::OwnershipLost)
}
fn validate_devices(view: View<'_>) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if view.maps.is_empty() || view.maps.len() != view.identities.len() {
        return Err(ownership_lost());
    }
    for map in view.maps {
        if !ids.insert(map.device_id)
            || !view
                .identities
                .iter()
                .any(|identity| identity.id == map.device_id)
            || map.rows.len() != map.definitions.len()
        {
            return Err(ownership_lost());
        }
    }
    Ok(())
}
fn observed_device(view: View<'_>, device: u16, key: u8) -> Result<(&MappingRow, &Definition)> {
    let index = view
        .maps
        .iter()
        .position(|map| map.device_id == device)
        .ok_or_else(ownership_lost)?;
    observed(view, index, key)
}
fn bound(owned: &NativeLease) -> Result<&target_identity::VerifiedTarget> {
    owned.target.as_ref().ok_or_else(ownership_lost)
}
fn saved_target(owned: &NativeLease) -> Result<&SavedKey> {
    let (_, device, _) = bound(owned)?.identity();
    owned
        .saved
        .iter()
        .find(|saved| saved.device_id == device)
        .ok_or_else(ownership_lost)
}
fn check_target(owned: &NativeLease, request: &NativeMapRequest) -> Result<()> {
    let previous = bound(owned)?;
    // Snapshot index can move in a fresh observation; identity/range cannot.
    if previous.identity() != request.target.identity()
        || previous.descriptor().1 != request.target.descriptor().1
    {
        return Err(ownership_lost());
    }
    Ok(())
}
fn original_matches(saved: &SavedKey, row: &MappingRow, definition: &Definition) -> bool {
    saved.original.restoration_matches(definition) && row.core == saved.original_core
}
fn guarded_matches(saved: &SavedKey, row: &MappingRow, definition: &Definition) -> bool {
    saved
        .original
        .guarded_restoration()
        .restoration_matches(definition)
        && row.core == saved.original_core
}
fn installed_matches(owned: &NativeLease, row: &MappingRow, definition: &Definition) -> bool {
    definition == &owned.installed
        && !row.is_modifier
        && !row.has_actions
        && row.core.first() == Some(&owned.policy.keysym)
        && row
            .core
            .iter()
            .all(|symbol| *symbol == 0 || *symbol == owned.policy.keysym)
        && (owned.is_uncertain() || row.core == owned.policy.installed)
}
fn check_view(owned: &NativeLease, view: View<'_>) -> Result<()> {
    require_same_lifetime(owned.epoch, view.epoch, &owned.identities, view.identities)?;
    validate_devices(view)?;
    if owned.saved.len() != view.maps.len() {
        return Err(ownership_lost());
    }
    Ok(())
}
pub(crate) fn check_installed(owned: &NativeLease, view: View<'_>) -> Result<()> {
    check_view(owned, view)?;
    let (master, target, _) = bound(owned)?.identity();
    for saved in &owned.saved {
        let (row, definition) = observed_device(view, saved.device_id, owned.policy.keycode)?;
        if row.is_pressed {
            return Err(policy_error(PolicyError::KeyStillDown));
        }
        let installed = installed_matches(owned, row, definition);
        let original = original_matches(saved, row, definition);
        let allowed = if saved.device_id == target {
            installed
        } else if saved.device_id == master {
            original || installed
        } else {
            original
        };
        if !allowed {
            return Err(ownership_lost());
        }
    }
    Ok(())
}

pub(crate) fn install<T: Transport>(
    transport: &mut T,
    mut set: T::State,
    mut prepared: NativeLease,
) -> Result<T::State> {
    let key = prepared.policy.keycode;
    let symbol = prepared.policy.keysym;
    let targets = transport.targets(&set)?;
    let request = target_identity::request(MutationKind::Install, &targets, key)?;
    if request.target.identity().2 != prepared.epoch {
        return Err(ownership_lost());
    }
    prepared.target = Some(request.target.clone());
    let saved = saved_target(&prepared)?.clone();
    prepared.policy.original = saved.original_core.clone();
    prepared.installed.repeat = saved.original.repeat;
    let epoch = prepared.epoch;
    let identities = prepared.identities.clone();
    let installed = prepared.installed.clone();
    map_groups::preflight_full(&request, &installed, T::group_observations(&set))?;
    dispatch_after_final_fence_on(
        transport,
        request,
        |transport| transport.fence(epoch, &identities, &identities),
        |transport, request| {
            // Durable uncertainty BEFORE entering the possibly partial writer.
            prepared.phase = Phase::InstallMayHaveApplied;
            transport.leases_mut().push(prepared);
            with_snapshot(T::descriptors(&mut set), request, |descriptor, request| {
                transport.write_full(descriptor, request, &installed)
            })
        },
    )?;
    let readback = transport.observe()?;
    let view = T::view(&readback);
    let owned = transport.leases().last().unwrap();
    check_installed(owned, view)?;
    let (_, target, _) = bound(owned)?.identity();
    let target_core = observed_device(view, target, key)?.0.core.clone();
    text_policy::validate_effective_keysym(
        symbol,
        target_core[0],
        transport.effective_symbol(&readback, target, key)?,
    )
    .map_err(policy_error)?;
    let owned = transport.leases_mut().last_mut().unwrap();
    owned.policy.installed = target_core;
    owned.phase = Phase::InstalledConfirmed;
    Ok(readback)
}

pub(crate) fn restore<T: Transport>(transport: &mut T, index: usize) -> Result<()> {
    let mut set = transport.observe()?;
    let view = T::view(&set);
    let owned = &transport.leases()[index];
    check_view(owned, view)?;
    let key = owned.policy.keycode;
    if transport.pending_release(key) {
        return Err(policy_error(PolicyError::KeyStillDown));
    }
    let (master, target, _) = bound(owned)?.identity();
    let mut all_original = true;
    for saved in &owned.saved {
        let (row, definition) = observed_device(view, saved.device_id, key)?;
        if row.is_pressed {
            return Err(policy_error(PolicyError::KeyStillDown));
        }
        let original = original_matches(saved, row, definition);
        let staged = owned.restoration_started() && guarded_matches(saved, row, definition);
        let installed = installed_matches(owned, row, definition);
        let allowed = if saved.device_id == target {
            installed || (owned.is_uncertain() && (original || staged))
        } else if saved.device_id == master {
            // Inactive XTEST leaves master unchanged. Partial writes can leave
            // master at any known phase; physical siblings never get this rule.
            original || installed || staged
        } else {
            original
        };
        if !allowed {
            return Err(ownership_lost());
        }
        all_original &= original;
    }
    if all_original && owned.is_uncertain() {
        transport.leases_mut().remove(index);
        return Ok(());
    }
    if !owned.is_uncertain() {
        check_installed(owned, view)?;
        transport.require_one_level(&set)?;
    }
    let saved = saved_target(owned)?;
    let staged = saved.original.guarded_restoration();
    let original_explicit = saved.original.explicit;
    let epoch = owned.epoch;
    let identities = owned.identities.clone();
    let observed_ids = view.identities.to_vec();
    let request = transport
        .targets(&set)
        .and_then(|targets| target_identity::request(MutationKind::RestoreFull, &targets, key))
        .and_then(|request| {
            check_target(&transport.leases()[index], &request)?;
            Ok(request)
        });
    if let Ok(request) = &request {
        // A new groups refusal is known-zero-write: retain the preceding phase
        // and real lease, for both confirmed and uncertain restoration attempts.
        map_groups::preflight_full(request, &staged, T::group_observations(&set))?;
    }
    // Normal submission advances only after groups preflight. Existing target
    // and fence error paths keep their previous uncertainty contract; no error
    // here can undo an earlier possibly-applied write or discard its lease.
    transport.leases_mut()[index].phase = Phase::FullRestoreMayHaveApplied;
    let request = request?;
    dispatch_after_final_fence_on(
        transport,
        request,
        |transport| transport.fence(epoch, &identities, &observed_ids),
        |transport, request| {
            with_snapshot(T::descriptors(&mut set), request, |descriptor, request| {
                transport.write_full(descriptor, request, &staged)
            })
        },
    )?;
    let mut readback = transport.observe()?;
    verify_restored(&transport.leases()[index], T::view(&readback), true)?;
    transport.leases_mut()[index].phase = Phase::GuardedConfirmed;
    let targets = transport.targets(&readback)?;
    let request = target_identity::request(MutationKind::RestoreExplicit, &targets, key)?;
    check_target(&transport.leases()[index], &request)?;
    let observed_ids = T::view(&readback).identities.to_vec();
    dispatch_after_final_fence_on(
        transport,
        request,
        |transport| transport.fence(epoch, &identities, &observed_ids),
        |transport, request| {
            transport.leases_mut()[index].phase = Phase::FinalizeMayHaveApplied;
            with_snapshot(
                T::descriptors(&mut readback),
                request,
                |descriptor, request| {
                    transport.write_explicit(descriptor, request, original_explicit)
                },
            )
        },
    )?;
    let final_readback = transport.observe()?;
    verify_restored(&transport.leases()[index], T::view(&final_readback), false)?;
    transport.leases_mut().remove(index);
    Ok(())
}
fn verify_restored(owned: &NativeLease, view: View<'_>, guarded: bool) -> Result<()> {
    check_view(owned, view)?;
    let (master, target, _) = bound(owned)?.identity();
    for saved in &owned.saved {
        let (row, definition) = observed_device(view, saved.device_id, owned.policy.keycode)?;
        if row.is_pressed {
            return Err(policy_error(PolicyError::KeyStillDown));
        }
        let original = original_matches(saved, row, definition);
        let staged = guarded_matches(saved, row, definition);
        let allowed = if !guarded {
            original
        } else if saved.device_id == target {
            staged
        } else if saved.device_id == master {
            original || staged
        } else {
            original
        };
        if !allowed {
            return Err(InputError::new(
                "input_failed",
                "per-device map/explicit/repeat restoration mismatch; lease retained",
            ));
        }
    }
    // Only this leased key has been checked. Full attributable master residue
    // and source-switch conflicts still require the separate C journal.
    Ok(())
}

#[cfg(test)]
#[path = "lease_journal_tests.rs"]
mod lease_journal_tests;
