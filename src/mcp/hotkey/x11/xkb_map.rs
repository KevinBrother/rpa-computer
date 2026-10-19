//! XKB map validation over owned byte-decoded values. No generated map() accessor.
use super::super::x11_candidate::{modifier_keys, ModifierKeys};
use super::super::x11_delivery::{validate_delivery, Action, DeliveryCase, Snapshot};
use super::super::x11_plan::{HotkeyPlan, KeyboardMap};
use super::xkb_wire::{self, Action as WireAction, KeyType};
use xcb::xkb;

pub(super) const DEVICE: u16 = xkb::Id::UseCoreKbd as u16;
pub(super) struct Validated {
    pub snapshot: Snapshot,
    pub modifiers: ModifierKeys,
}

pub(super) fn read(
    connection: &xcb::Connection,
    core: &KeyboardMap,
    plan: &HotkeyPlan,
) -> Result<Validated, String> {
    let controls = connection
        .wait_for_reply(connection.send_request(&xkb::GetControls {
            device_spec: DEVICE,
        }))
        .map_err(|e| format!("cannot read XKB controls: {e}"))?;
    validate_controls(
        controls.enabled_controls().bits(),
        controls.internal_mods_mask().bits() as u8,
        controls.ignore_lock_mods_mask().bits() as u8,
    )?;
    validate_owned(core, plan, controls.num_groups(), read_owned(connection)?)
}

pub(super) fn read_owned(connection: &xcb::Connection) -> Result<xkb_wire::Map, String> {
    let reply = connection
        .wait_for_reply(connection.send_request(&xkb::GetMap {
            device_spec: DEVICE,
            full: xkb::MapPart::all(),
            partial: xkb::MapPart::empty(),
            first_type: 0,
            n_types: 0,
            first_key_sym: 0,
            n_key_syms: 0,
            first_key_action: 0,
            n_key_actions: 0,
            first_key_behavior: 0,
            n_key_behaviors: 0,
            virtual_mods: xkb::VMod::all(),
            first_key_explicit: 0,
            n_key_explicit: 0,
            first_mod_map_key: 0,
            n_mod_map_keys: 0,
            first_v_mod_map_key: 0,
            n_v_mod_map_keys: 0,
        }))
        .map_err(|e| format!("cannot read XKB types/actions: {e}"))?;
    xkb_wire::from_reply(&reply)
}

/// Shared controls rejection policy; no second test-only safety algorithm.
pub(super) fn validate_controls(enabled: u32, internal: u8, ignore_lock: u8) -> Result<(), String> {
    // Do not silently change user accessibility/group controls. Filters that can
    // suppress/delay the chord require explicit failure rather than false Armed.
    let unsupported = xkb::BoolCtrl::SLOW_KEYS
        | xkb::BoolCtrl::BOUNCE_KEYS
        | xkb::BoolCtrl::STICKY_KEYS
        | xkb::BoolCtrl::OVERLAY1_MASK
        | xkb::BoolCtrl::OVERLAY2_MASK;
    if enabled & unsupported.bits() != 0 || internal != 0 || ignore_lock != 0 {
        return Err("XKB controls alter emergency key delivery/modifier semantics".into());
    }
    Ok(())
}

/// The actual production semantic validation entry, shared by pure owned-map
/// tests. Callers supply a full decoded snapshot; this does not replace the
/// read path's controls/subscription/consistency checks or authorize a session.
pub(super) fn validate_owned(
    core: &KeyboardMap,
    plan: &HotkeyPlan,
    groups: u8,
    map: xkb_wire::Map,
) -> Result<Validated, String> {
    let xkb_wire::Map {
        types,
        symbols,
        actions,
        behaviors,
        virtuals,
        modifiers: xkb_mods,
        ..
    } = map;
    let mut masks = [0u8; 256];
    for (slot, keys) in core.modifier_keycodes.iter().enumerate() {
        for &key in keys.iter().filter(|key| **key != 0) {
            masks[usize::from(key)] |= 1 << slot;
        }
    }
    if masks != xkb_mods {
        return Err("core/XKB modifier maps disagree or changed during setup".into());
    }
    let mut modifiers = modifier_keys(core);
    let mut snapshot = Snapshot {
        subscribed_before_read: true,
        groups,
        dependency_keys: plan
            .keycodes
            .iter()
            .copied()
            .chain(
                core.modifier_keycodes
                    .iter()
                    .flatten()
                    .copied()
                    .filter(|key| *key != 0),
            )
            .collect(),
        ..Snapshot::default()
    };
    // Independently operated action keys can alter modifiers without appearing
    // in the core modifier map. Require them not to be held at activation.
    for (&code, values) in &actions {
        if masks[usize::from(code)] == 0 && values.iter().any(modifies_state) {
            modifiers.unwanted.push(code);
            snapshot.dependency_keys.push(code);
        }
    }
    snapshot.dependency_keys.sort_unstable();
    snapshot.dependency_keys.dedup();
    for (code, behavior) in behaviors {
        if snapshot.dependency_keys.contains(&code) && behavior[0] != 0 {
            return Err("XKB key behavior redirects or locks an emergency modifier/key".into());
        }
    }
    for &code in &snapshot.dependency_keys {
        let key = symbols
            .get(&code)
            .ok_or("missing dependent XKB key symbols")?;
        let n_groups = key.group_info & 0x0f;
        if n_groups == 0 || n_groups > 4 || key.width == 0 {
            return Err("invalid dependent XKB key groups".into());
        }
        for &index in key.types.iter().take(usize::from(n_groups)) {
            let ty = types.get(&index).ok_or("missing dependent XKB type")?;
            validate_type(ty, &virtuals)?;
            snapshot.dependency_types.push(index);
        }
        // Validate every modifier level, including Alt's shifted Meta aliases.
        if masks[usize::from(code)] != 0 {
            for action in actions.get(&code).into_iter().flatten() {
                match action.0[0] {
                    0 => {} // NoAction
                    1 | 3 => {
                        // SetMods / LockMods: byte fields match XKB wire layout.
                        let [_, flags, mask, real_mods, vmods_high, vmods_low, _, _] = action.0;
                        let real = if flags & xkb::Sa::USE_MOD_MAP_MODS.bits() as u8 != 0 {
                            masks[usize::from(code)]
                        } else {
                            real_mods
                        };
                        let resolved = real
                            | resolve_virtual(
                                u16::from_be_bytes([vmods_high, vmods_low]),
                                &virtuals,
                            )?;
                        if mask != resolved || mask != masks[usize::from(code)] {
                            return Err(
                                "XKB modifier action disagrees with physical modifier map".into()
                            );
                        }
                        if action.0[0] == 3
                            && (modifiers.control.contains(&code)
                                || modifiers.alt.contains(&code)
                                || modifiers.shift.contains(&code))
                        {
                            return Err(
                                "required physical modifier has a locking XKB action".into()
                            );
                        }
                    }
                    _ => return Err("XKB modifier has unsupported action semantics".into()),
                }
            }
        }
    }
    for &code in &plan.keycodes {
        let key = symbols.get(&code).ok_or("missing XKB Escape symbols")?;
        for group in 0..snapshot.groups {
            let key_group = effective_group(key.group_info, group)?;
            let type_index = key.types[usize::from(key_group)];
            let ty = types.get(&type_index).ok_or("missing Escape XKB type")?;
            for &mask in &plan.modifier_masks {
                let effective = (mask as u8) & ty.mask;
                let level = ty
                    .entries
                    .iter()
                    .find(|entry| entry.active && entry.mask == effective)
                    .map_or(0, |entry| entry.level);
                if level >= ty.levels || level >= key.width {
                    return Err("XKB type level outside key width".into());
                }
                let index = usize::from(key_group) * usize::from(key.width) + usize::from(level);
                let symbol = *key
                    .symbols
                    .get(index)
                    .ok_or("missing XKB symbol for selected level")?;
                let action = match actions.get(&code).filter(|values| !values.is_empty()) {
                    None => Action::NoAction,
                    Some(values) => {
                        match values.get(index).ok_or("missing XKB selected action")?.0[0] {
                            0 => Action::NoAction,
                            13 => Action::SwitchScreen,
                            kind => Action::Other(kind),
                        }
                    }
                };
                snapshot.cases.push(DeliveryCase {
                    keycode: code,
                    group,
                    modifiers: mask,
                    type_index: Some(type_index),
                    symbol,
                    action,
                });
            }
        }
    }
    snapshot.dependency_types.sort_unstable();
    snapshot.dependency_types.dedup();
    validate_delivery(plan, &snapshot)?;
    Ok(Validated {
        snapshot,
        modifiers,
    })
}

fn resolve_virtual(bits: u16, virtuals: &[u8; 16]) -> Result<u8, String> {
    let mut mask = 0;
    for (bit, &value) in virtuals.iter().enumerate() {
        if bits & (1 << bit) != 0 {
            if value == 0 {
                return Err("unresolved dependent XKB virtual modifier".into());
            }
            mask |= value;
        }
    }
    Ok(mask)
}
fn validate_type(ty: &KeyType, virtuals: &[u8; 16]) -> Result<(), String> {
    let resolved = ty.real | resolve_virtual(ty.virtuals, virtuals)?;
    if resolved != ty.mask || ty.levels == 0 {
        return Err("inconsistent XKB type modifier resolution".into());
    }
    for entry in ty.entries.iter().filter(|entry| entry.active) {
        let mask = entry.real | resolve_virtual(entry.virtuals, virtuals)?;
        if mask != entry.mask || mask & !resolved != 0 || entry.level >= ty.levels {
            return Err("inconsistent XKB type entry".into());
        }
    }
    Ok(())
}
fn effective_group(info: u8, group: u8) -> Result<u8, String> {
    let count = info & 15;
    if count == 0 || count > 4 {
        return Err("invalid XKB group count".into());
    }
    if group < count {
        return Ok(group);
    }
    match info & 0xc0 {
        0 => Ok(group % count),
        0x40 => Ok(count - 1),
        0x80 if (info >> 4) & 3 < count => Ok((info >> 4) & 3),
        _ => Err("invalid XKB group redirection".into()),
    }
}
fn modifies_state(action: &WireAction) -> bool {
    // Existing known state-changing actions, plus opaque/private types whose
    // effects cannot safely be assumed inert. Those keys join held-key rejection
    // and mapping-invalidation dependencies; no unknown action becomes NoAction.
    matches!(action.0[0], 1..=6 | 11 | 14 | 15 | 17 | 21..=255)
}

#[cfg(test)]
#[path = "xkb_map_tests.rs"]
mod tests;
