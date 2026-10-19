//! Validation of normalized server XKB types/actions and change dependencies.
use super::x11_plan::HotkeyPlan;

#[derive(Clone, Debug)]
pub(super) enum Action {
    NoAction,
    SwitchScreen,
    Other(u8),
}

/// One resolved key/group/modifier case from the server's actual type + action
/// tables, not a guess from the core keysym row. `None` means unresolved type.
#[derive(Clone, Debug)]
pub(super) struct DeliveryCase {
    pub keycode: u8,
    pub group: u8,
    pub modifiers: u16,
    pub type_index: Option<u8>,
    pub symbol: u32,
    pub action: Action,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Snapshot {
    pub subscribed_before_read: bool,
    pub changed_during_read: bool,
    pub groups: u8,
    pub cases: Vec<DeliveryCase>,
    pub dependency_keys: Vec<u8>,
    pub dependency_types: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(super) enum MapChange {
    CoreModifiers,
    Symbols { first: u8, count: u8 },
    Actions { first: u8, count: u8 },
    Types { first: u8, count: u8 },
    ModifierBindings,
    Compatibility,
    Controls,
    NewKeyboard,
}

pub(super) fn validate_delivery(plan: &HotkeyPlan, snapshot: &Snapshot) -> Result<(), String> {
    if !snapshot.subscribed_before_read
        || snapshot.changed_during_read
        || !(1..=4).contains(&snapshot.groups)
        || plan.keycodes.is_empty()
        || plan.modifier_masks.is_empty()
    {
        return Err("XKB delivery snapshot is incomplete or changed during setup".into());
    }
    for &key in &plan.keycodes {
        for group in 0..snapshot.groups {
            for &mask in &plan.modifier_masks {
                let cases: Vec<_> = snapshot
                    .cases
                    .iter()
                    .filter(|case| {
                        case.keycode == key && case.group == group && case.modifiers == mask
                    })
                    .collect();
                if cases.len() != 1 {
                    return Err("XKB delivery case missing or duplicated".into());
                }
                let case = cases[0];
                if let Action::Other(code) = case.action {
                    return Err(format!(
                        "XKB action {code} cannot deliver the Escape emergency chord"
                    ));
                }
                if case.symbol != 0xff1b
                    || !matches!(case.action, Action::NoAction)
                    || !case
                        .type_index
                        .is_some_and(|index| snapshot.dependency_types.contains(&index))
                {
                    return Err("XKB type/action cannot deliver the Escape emergency chord".into());
                }
            }
        }
    }
    Ok(())
}

pub(super) fn invalidates(snapshot: &Snapshot, change: &MapChange) -> bool {
    let overlaps = |keys: &[u8], first: u8, count: u8| {
        count == 0
            || keys.iter().any(|key| {
                u16::from(*key) >= u16::from(first)
                    && u16::from(*key) < u16::from(first) + u16::from(count)
            })
    };
    match *change {
        MapChange::Symbols { first, count } | MapChange::Actions { first, count } => {
            overlaps(&snapshot.dependency_keys, first, count)
        }
        MapChange::Types { first, count } => overlaps(&snapshot.dependency_types, first, count),
        _ => true,
    }
}
