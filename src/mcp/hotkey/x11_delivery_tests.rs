//! Portable XKB policy assertions; not evidence of protocol parsing/delivery.
use super::x11_delivery::{
    invalidates, validate_delivery, Action, DeliveryCase, MapChange, Snapshot,
};
use super::x11_plan::HotkeyPlan;

fn plan() -> HotkeyPlan {
    HotkeyPlan {
        keycodes: vec![9],
        modifier_masks: vec![13, 15, 29, 31],
        requires_identity: false,
    }
}

fn safe_snapshot() -> Snapshot {
    Snapshot {
        subscribed_before_read: true,
        changed_during_read: false,
        groups: 2,
        cases: (0..2)
            .flat_map(|group| {
                plan()
                    .modifier_masks
                    .into_iter()
                    .map(move |modifiers| DeliveryCase {
                        keycode: 9,
                        group,
                        modifiers,
                        type_index: Some(3),
                        symbol: 0xff1b,
                        action: Action::NoAction,
                    })
            })
            .collect(),
        dependency_keys: vec![9, 37, 105, 50, 62, 64, 108, 204, 205, 66, 77],
        dependency_types: vec![3],
    }
}

#[test]
fn checked_escape_delivery_across_groups_and_lock_variants_is_accepted() {
    assert!(validate_delivery(&plan(), &safe_snapshot()).is_ok());
}

#[test]
fn core_only_snapshot_must_not_authorize_armed() {
    assert!(validate_delivery(&plan(), &Snapshot::default()).is_err());
}

#[test]
fn switches_or_unknown_actions_on_any_accepted_case_are_rejected() {
    for index in 0..8 {
        for action in [Action::SwitchScreen, Action::Other(0xff)] {
            let mut snapshot = safe_snapshot();
            snapshot.cases[index].action = action;
            assert!(
                validate_delivery(&plan(), &snapshot).is_err(),
                "unsafe case {index}"
            );
        }
    }
}

#[test]
fn type_selecting_non_escape_or_unresolved_virtual_modifiers_is_rejected() {
    let mut snapshot = safe_snapshot();
    snapshot.cases[0].symbol = 0x1008fe0c;
    assert!(validate_delivery(&plan(), &snapshot).is_err());
    let mut snapshot = safe_snapshot();
    snapshot.cases[0].type_index = None;
    assert!(validate_delivery(&plan(), &snapshot).is_err());
}

#[test]
fn incomplete_group_lock_or_key_coverage_cannot_authorize_delivery() {
    for index in 0..8 {
        let mut snapshot = safe_snapshot();
        snapshot.cases.remove(index);
        assert!(
            validate_delivery(&plan(), &snapshot).is_err(),
            "missing case {index}"
        );
    }
    let mut extra_key = plan();
    extra_key.keycodes.push(10);
    assert!(validate_delivery(&extra_key, &safe_snapshot()).is_err());
}

#[test]
fn missing_subscription_or_map_changed_during_setup_requires_retry_or_failure() {
    let mut snapshot = safe_snapshot();
    snapshot.subscribed_before_read = false;
    assert!(validate_delivery(&plan(), &snapshot).is_err());
    let mut snapshot = safe_snapshot();
    snapshot.changed_during_read = true;
    assert!(validate_delivery(&plan(), &snapshot).is_err());
}

#[test]
fn action_only_changes_on_emergency_or_modifier_keys_invalidate() {
    let snapshot = safe_snapshot();
    for key in [9, 37, 50, 64, 204, 205, 66, 77] {
        assert!(
            invalidates(
                &snapshot,
                &MapChange::Actions {
                    first: key,
                    count: 1
                }
            ),
            "key {key}"
        );
    }
}

#[test]
fn type_only_change_of_a_referenced_type_invalidates() {
    let snapshot = safe_snapshot();
    assert!(invalidates(
        &snapshot,
        &MapChange::Types { first: 3, count: 1 }
    ));
    assert!(!invalidates(
        &snapshot,
        &MapChange::Types { first: 4, count: 1 }
    ));
}

#[test]
fn modifier_compatibility_control_and_keyboard_replacement_changes_invalidate() {
    for change in [
        MapChange::CoreModifiers,
        MapChange::ModifierBindings,
        MapChange::Compatibility,
        MapChange::Controls,
        MapChange::NewKeyboard,
    ] {
        assert!(invalidates(&safe_snapshot(), &change), "{change:?}");
    }
}

#[test]
fn unrelated_scratch_key_changes_do_not_revoke_emergency_protection() {
    let snapshot = safe_snapshot();
    for change in [
        MapChange::Symbols {
            first: 240,
            count: 1,
        },
        MapChange::Actions {
            first: 240,
            count: 1,
        },
    ] {
        assert!(!invalidates(&snapshot, &change));
    }
    assert!(invalidates(
        &snapshot,
        &MapChange::Symbols { first: 9, count: 1 }
    ));
}
