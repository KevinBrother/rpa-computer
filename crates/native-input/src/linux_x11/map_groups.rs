//! Prevent observed num_groups changes from native Install / RestoreFull.
//! This is a pre-write check, not atomic observation or retirement evidence.
use super::{Definition, MutationKind, NativeMapRequest};
use crate::{InputError, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroupObservation {
    pub device_id: u16,
    pub range: (u8, u8),
    pub num_groups: u8,
    pub groups_by_key: Vec<u8>,
}

/// Captured with the actual journal State. Epoch is lifetime provenance only,
/// not a promise that separate XKB map/controls requests are atomic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroupState {
    pub epoch: u64,
    pub devices: Vec<GroupObservation>,
}

fn unknown(reason: impl std::fmt::Display) -> InputError {
    InputError::new(
        "input_failed",
        format!("native mapping group observation unknown: {reason}"),
    )
}

/// Both full-write phases pass their actual verified request and final payload.
/// T and M are conservative possible recipients, independently of vector order;
/// no active-source assumption or physical-sibling canonicality gate is added.
pub(crate) fn preflight_full(
    request: &NativeMapRequest,
    replacement: &Definition,
    observation: Result<GroupState>,
) -> Result<()> {
    request.check_binding().map_err(unknown)?;
    if !matches!(
        request.kind,
        MutationKind::Install | MutationKind::RestoreFull
    ) {
        return Err(unknown("groups preflight requires a full-map request"));
    }
    let state = observation.map_err(unknown)?;
    let (master, target, epoch) = request.target.identity();
    let (_, range) = request.target.descriptor();
    if state.epoch != epoch || master == target || range.0 < 8 || range.0 > range.1 {
        return Err(unknown("request lifetime or keyboard range does not match"));
    }
    if !(2..=32).contains(&state.devices.len()) {
        return Err(unknown("incomplete or unbounded keyboard observation"));
    }
    let mut identities = std::collections::HashSet::new();
    for device in &state.devices {
        if device.device_id == 0 || !identities.insert(device.device_id) {
            return Err(unknown("invalid or duplicate keyboard identity"));
        }
    }
    let observed = |id| {
        state
            .devices
            .iter()
            .find(|device| device.device_id == id)
            .ok_or_else(|| unknown("target or master observation missing"))
    };
    let recipients = [observed(target)?, observed(master)?];
    let width = usize::from(range.1) - usize::from(range.0) + 1;
    // check_binding validated the request key against this verified range.
    let key_index = usize::from(request.key) - usize::from(range.0);
    let replacement_groups = replacement.group_info & 0x0f;
    if replacement_groups > 4 {
        return Err(unknown("invalid replacement group count"));
    }
    // Validate both complete observations before diagnosing a layout mismatch.
    // Zero groups at a known key is valid; missing data is never filled with 0.
    for device in recipients {
        if device.range != range
            || device.groups_by_key.len() != width
            || device.groups_by_key.iter().any(|groups| *groups > 4)
            || !(1..=4).contains(&device.num_groups)
        {
            return Err(unknown("invalid target/master group geometry or controls"));
        }
    }
    for device in recipients {
        let before = device
            .groups_by_key
            .iter()
            .copied()
            .max()
            .ok_or_else(|| unknown("empty keyboard group range"))?;
        let after = device
            .groups_by_key
            .iter()
            .enumerate()
            .map(|(index, groups)| {
                if index == key_index {
                    replacement_groups
                } else {
                    *groups
                }
            })
            .max()
            .ok_or_else(|| unknown("empty replacement group range"))?;
        let installing = request.kind == MutationKind::Install;
        if device.num_groups != before {
            return Err(InputError::new(
                if installing {
                    "invalid_text"
                } else {
                    "input_failed"
                },
                if installing {
                    "native mapping rejected before write: noncanonical group count"
                } else {
                    "native mapping restoration rejected before write: noncanonical group count"
                },
            ));
        }
        if device.num_groups != after {
            return Err(InputError::new(
                if installing {
                    "invalid_text"
                } else {
                    "input_failed"
                },
                if installing {
                    "native mapping rejected before write: replacement would change group count"
                } else {
                    "native mapping restoration would change group count"
                },
            ));
        }
    }
    Ok(())
}
