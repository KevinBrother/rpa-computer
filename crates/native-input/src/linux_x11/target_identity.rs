//! Observed identity input to the ACTUAL native mutation request planner.
//!
//! Verifies an explicit XTEST slave and binds its actual descriptor/range.
//! The cooperative marker is not authority, a lifetime fence or projection proof.
use super::{MutationKind, NativeMapRequest};
use crate::{InputError, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Marker {
    NotQueried,
    Missing,
    Unreadable,
    Reply {
        actual_type: u32,
        format: u8,
        items: usize,
        bytes_after: u32,
        first_byte: Option<u8>,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct Device {
    pub id: i32,
    pub kind: i32,
    pub attachment: i32,
    pub enabled: bool,
    #[allow(dead_code)] // Observed for diagnostics/tests, NEVER an identity predicate.
    pub name: Vec<u8>,
    pub key_sources: Vec<i32>,
    pub marker: Marker,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Descriptor {
    pub device_id: u16,
    pub range: (u8, u8),
}
pub(crate) struct Inventory {
    pub master: u16,
    /// Observation epoch, not a new authority token. The existing final fence
    /// still checks the live epoch and cannot make master SetMap hotplug-safe.
    pub epoch: u64,
    pub devices: Vec<Device>,
    /// In actual Snapshot order, independently of XIQueryDevice enumeration.
    pub descriptors: Vec<Descriptor>,
}

/// Constructible only by verification below. This is an observation-bound
/// target, not a retained lease identity: the journal must still pin its
/// lifetime/saved state and perform the live fence before every mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedTarget {
    master: u16,
    device: u16,
    epoch: u64,
    snapshot_index: usize,
    range: (u8, u8),
}
impl VerifiedTarget {
    pub(super) fn identity(&self) -> (u16, u16, u64) {
        (self.master, self.device, self.epoch)
    }
    pub(super) fn descriptor(&self) -> (usize, (u8, u8)) {
        (self.snapshot_index, self.range)
    }
}
fn invalid(message: &str) -> InputError {
    InputError::new("unsupported_session", message)
}
fn exact_marker(marker: &Marker) -> bool {
    // XA_INTEGER is predefined atom19. The observer requests the canonical
    // existing "XTEST Device" atom, never an atom supplied by a device name.
    matches!(
        marker,
        Marker::Reply {
            actual_type: 19,
            format: 8,
            items: 1,
            bytes_after: 0,
            first_byte: Some(1),
        }
    )
}
/// Unique enabled marked keyboard slave, real matching descriptor, and the
/// narrower device-specific XTEST wire limit. No bare-ID/master fallback.
pub(crate) fn verify(inventory: &Inventory) -> Result<VerifiedTarget> {
    if inventory.devices.is_empty()
        || inventory.devices.len() > 256
        || inventory.descriptors.is_empty()
        || inventory.descriptors.len() > 32
    {
        return Err(invalid("unbounded or empty native target inventory"));
    }
    let mut ids = std::collections::HashSet::new();
    for device in &inventory.devices {
        if !(1..=u16::MAX as i32).contains(&device.id) || !ids.insert(device.id) {
            return Err(invalid("invalid or duplicate native device identity"));
        }
    }
    let master = inventory
        .devices
        .iter()
        .find(|device| device.id == i32::from(inventory.master))
        .ok_or_else(|| invalid("native target master absent from observed inventory"))?;
    if master.kind != 2 || !master.enabled || master.key_sources.len() != 1 {
        return Err(invalid(
            "native target requires an observed enabled master keyboard",
        ));
    }
    let mut descriptor_ids = std::collections::HashSet::new();
    for descriptor in &inventory.descriptors {
        if !descriptor_ids.insert(descriptor.device_id)
            || !ids.contains(&i32::from(descriptor.device_id))
            || descriptor.range.0 < 8
            || descriptor.range.0 > descriptor.range.1
        {
            return Err(invalid("invalid or stale native descriptor inventory"));
        }
    }
    if !descriptor_ids.contains(&inventory.master) {
        return Err(invalid("native master snapshot is missing"));
    }
    let mut target = None;
    for device in &inventory.devices {
        if !device.enabled || device.kind != 4 || device.attachment != i32::from(inventory.master) {
            continue;
        }
        // A missing key class is not a candidate; a duplicate is unobservable,
        // not permission to pick one class or ignore another marked target.
        if device.key_sources.is_empty() {
            continue;
        }
        if device.key_sources.len() != 1 {
            return Err(invalid("ambiguous native keyboard key classes"));
        }
        match &device.marker {
            Marker::Missing => continue,
            Marker::NotQueried | Marker::Unreadable => {
                return Err(invalid("native XTEST marker could not be verified"));
            }
            marker if !exact_marker(marker) => {
                return Err(invalid("native XTEST marker has invalid type or value"));
            }
            _ => {}
        }
        if target.replace(device).is_some() {
            return Err(invalid("ambiguous native XTEST keyboard identity"));
        }
    }
    let device = target.ok_or_else(|| invalid("no verified XTEST keyboard for native mapping"))?;
    if !(1..=127).contains(&device.id) {
        return Err(invalid(
            "native XTEST target ID exceeds its seven-bit input transport",
        ));
    }
    let device_id = u16::try_from(device.id).map_err(|_| invalid("invalid native XTEST ID"))?;
    let (snapshot_index, descriptor) = inventory
        .descriptors
        .iter()
        .enumerate()
        .find(|(_, descriptor)| descriptor.device_id == device_id)
        .ok_or_else(|| invalid("verified XTEST target has no matching snapshot"))?;
    Ok(VerifiedTarget {
        master: inventory.master,
        device: device_id,
        epoch: inventory.epoch,
        snapshot_index,
        range: descriptor.range,
    })
}

/// Production convenience entry: every request goes through the typed verifier.
/// It cannot be used as a bare-ID legacy selector or to bypass descriptor binding.
pub(crate) fn request(
    kind: MutationKind,
    inventory: &Inventory,
    key: u8,
) -> Result<NativeMapRequest> {
    NativeMapRequest::for_verified(kind, verify(inventory)?, key)
}

#[cfg(test)]
#[path = "target_identity_tests.rs"]
mod target_identity_tests;
