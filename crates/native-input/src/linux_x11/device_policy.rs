//! Conservative policy for a master map write and every affected keyboard.
//! Definitions include repeat. A divergent slot is skipped, never overwritten.
#[cfg(target_os = "linux")]
use super::{definition::Definition, text_policy};
#[cfg(not(target_os = "linux"))]
use crate::{linux_x11_definition::Definition, linux_x11_text_policy as text_policy};
use text_policy::{Lease, LeaseChoice, MappingRow, PolicyError};

pub(super) struct DeviceMap {
    pub device_id: u16,
    pub rows: Vec<MappingRow>,
    pub definitions: Vec<Definition>,
}

fn row_and_definition(device: &DeviceMap, key: u8) -> Option<(&MappingRow, &Definition)> {
    let index = device.rows.iter().position(|row| row.keycode == key)?;
    Some((&device.rows[index], device.definitions.get(index)?))
}

pub(super) fn plan_affected_lease(
    symbol: u64,
    devices: &[DeviceMap],
    leases: &[Lease],
) -> Result<LeaseChoice, PolicyError> {
    let master = devices.first().ok_or(PolicyError::InvalidMapping)?;
    let mut ids = std::collections::HashSet::new();
    if devices
        .iter()
        .any(|d| !ids.insert(d.device_id) || d.rows.len() != d.definitions.len())
    {
        return Err(PolicyError::InvalidMapping);
    }
    if let Some(lease) = leases.iter().find(|lease| lease.keysym == symbol) {
        let expected_ids = devices.iter().map(|d| d.device_id).collect::<Vec<_>>();
        affected_restoration_allowed(lease, &expected_ids, devices, false)?;
        let (_, expected) =
            row_and_definition(master, lease.keycode).ok_or(PolicyError::OwnershipLost)?;
        if devices.iter().any(|d| {
            row_and_definition(d, lease.keycode)
                .is_none_or(|(_, def)| def.repeat.is_none() || def != expected)
        }) {
            return Err(PolicyError::OwnershipLost);
        }
        return Ok(LeaseChoice::Reuse(lease.keycode));
    }
    let rows = master
        .rows
        .iter()
        .filter(|row| {
            let Some((_, expected)) = row_and_definition(master, row.keycode) else {
                return false;
            };
            expected.repeat.is_some()
                // Borrowing a multi-group key could change global num_groups
                // even if all its symbols are vacant. Do not mutate that state.
                && expected.group_info & 0x0f <= 1
                && !expected.has_effects()
                && !expected.is_modifier()
                && devices.iter().all(|device| {
                    row_and_definition(device, row.keycode).is_some_and(|(other, def)| {
                        def == expected
                            && other.core == row.core
                            && other.xkb == row.xkb
                            && !other.is_pressed
                            && !other.is_modifier
                            && !other.has_actions
                    })
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    text_policy::plan_lease(symbol, &rows, leases)
}

pub(super) fn affected_restoration_allowed(
    lease: &Lease,
    expected_device_ids: &[u16],
    devices: &[DeviceMap],
    pending: bool,
) -> Result<(), PolicyError> {
    if pending {
        return Err(PolicyError::KeyStillDown);
    }
    let expected: std::collections::BTreeSet<_> = expected_device_ids.iter().copied().collect();
    let actual: std::collections::BTreeSet<_> = devices.iter().map(|d| d.device_id).collect();
    if expected.is_empty()
        || expected != actual
        || actual.len() != devices.len()
        || expected.len() != expected_device_ids.len()
    {
        return Err(PolicyError::OwnershipLost);
    }
    for device in devices {
        let (row, definition) =
            row_and_definition(device, lease.keycode).ok_or(PolicyError::OwnershipLost)?;
        if definition.repeat.is_none() {
            return Err(PolicyError::OwnershipLost);
        }
        text_policy::restoration_allowed(lease, row, false)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "device_policy_tests.rs"]
mod device_policy_tests;

// Pure collection seam is also compiled by the existing portable policy exposure.
#[path = "keymap_read.rs"]
pub(super) mod keymap_read;
