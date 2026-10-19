//! G3: exercise the exact owned-map validation used by read(), not a parallel
//! numeric-action translator. No transport/Reply/unsafe decoder or X server.
use super::super::super::x11_candidate::{decide, Candidate, Decision};
use super::super::super::x11_delivery::{invalidates, MapChange};
use super::super::super::x11_plan::plan as make_plan;
use super::super::xkb_wire::{Map, Symbols, TypeEntry};
use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

fn fixture() -> (KeyboardMap, HotkeyPlan, Map) {
    let mut slots: [Vec<u8>; 8] = Default::default();
    slots[0] = vec![11];
    slots[1] = vec![13];
    slots[2] = vec![10];
    slots[3] = vec![12];
    let core = KeyboardMap {
        first_keycode: 9,
        keysyms_per_keycode: 1,
        keysyms: vec![0xff1b, 0xffe3, 0xffe1, 0xffe9, 0xffe5, 0x61],
        modifier_keycodes: slots,
    };
    let plan = make_plan(&core).expect("small physical Escape/Ctrl/Shift/Alt/Caps map");
    let mut map = Map {
        device: 3,
        min_key: 9,
        max_key: 14,
        types: BTreeMap::new(),
        symbols: BTreeMap::new(),
        actions: BTreeMap::new(),
        behaviors: BTreeMap::new(),
        virtuals: [0; 16],
        modifiers: [0; 256],
        explicit: BTreeMap::new(),
        virtual_modifiers: BTreeMap::new(),
    };
    for index in [0, 1] {
        map.types.insert(
            index,
            KeyType {
                mask: 0,
                real: 0,
                virtuals: 0,
                levels: 1,
                entries: vec![],
                preserve: None,
            },
        );
    }
    for (i, symbol) in core.keysyms.iter().enumerate() {
        let code = 9 + i as u8;
        map.symbols.insert(
            code,
            Symbols {
                types: [u8::from(code == 14), 0, 0, 0],
                group_info: 1,
                width: 1,
                symbols: vec![*symbol],
            },
        );
        map.actions.insert(code, vec![WireAction([0; 8])]);
    }
    for (code, tag, mask) in [(10u8, 1, 4), (11, 1, 1), (12, 1, 8), (13, 3, 2)] {
        map.modifiers[usize::from(code)] = mask;
        map.actions
            .insert(code, vec![WireAction([tag, 0, mask, mask, 0, 0, 0, 0])]);
    }
    (core, plan, map)
}

#[test]
fn owned_entry_accepts_numeric_setmods_caps_lockmods_and_escape_noaction() {
    let (core, plan, map) = fixture();
    let result = validate_owned(&core, &plan, 1, map).unwrap();
    assert_eq!(result.modifiers.control, [10]);
    assert_eq!(result.modifiers.shift, [11]);
    assert_eq!(result.modifiers.alt, [12]);
    assert_eq!(result.snapshot.dependency_keys, [9, 10, 11, 12, 13]);
    assert_eq!(result.snapshot.dependency_types, [0]);
    assert_eq!(result.snapshot.cases.len(), plan.modifier_masks.len());
    assert!(result
        .snapshot
        .cases
        .iter()
        .all(|c| matches!(c.action, Action::NoAction)));
}

#[test]
fn use_mod_map_mods_numeric_flag_uses_physical_mask_not_raw_real_field() {
    let (core, plan, mut map) = fixture();
    // XML SA.UseModMapMods=bit2. Deliberately nonphysical raw realMods.
    map.actions.get_mut(&10).unwrap()[0].0 = [1, 4, 4, 0xff, 0, 0, 0, 0];
    validate_owned(&core, &plan, 1, map).expect("flag selects physical Control mask");
    let (core, plan, mut map) = fixture();
    map.actions.get_mut(&10).unwrap()[0].0 = [1, 0, 4, 0xff, 0, 0, 0, 0];
    assert!(validate_owned(&core, &plan, 1, map).is_err());
}

#[test]
fn locking_action_is_rejected_for_each_required_modifier_not_caps() {
    let (core, plan, map) = fixture();
    validate_owned(&core, &plan, 1, map).unwrap();
    for code in [10, 11, 12] {
        let (core, plan, mut map) = fixture();
        map.actions.get_mut(&code).unwrap()[0].0[0] = 3;
        assert!(validate_owned(&core, &plan, 1, map).is_err());
    }
}

#[test]
fn action_virtual_high_low_are_distinct_bytes_not_native_card16() {
    for (high, low) in [(1, 0), (0, 2), (1, 2)] {
        let (core, plan, mut map) = fixture();
        map.virtuals[8] = 4;
        map.virtuals[1] = 4;
        // Swapping (1,2) resolves to Shift|Alt, not Control.
        map.virtuals[9] = 1;
        map.virtuals[0] = 8;
        map.actions.get_mut(&10).unwrap()[0].0 = [1, 0, 4, 0, high, low, 0, 0];
        validate_owned(&core, &plan, 1, map).expect("separate high/low virtual bits");
    }
    let (core, plan, mut map) = fixture();
    map.virtuals[8] = 4;
    map.virtuals[1] = 4;
    map.virtuals[9] = 1;
    map.virtuals[0] = 8;
    map.actions.get_mut(&10).unwrap()[0].0 = [1, 0, 4, 0, 2, 1, 0, 0];
    assert!(validate_owned(&core, &plan, 1, map).is_err());
}

#[test]
fn selected_escape_switchscreen_and_unknown_actions_are_not_noaction() {
    let (core, plan, map) = fixture();
    validate_owned(&core, &plan, 1, map).unwrap();
    for tag in [13, 21, 0xfe, 0xff] {
        let (core, plan, mut map) = fixture();
        map.actions.get_mut(&9).unwrap()[0].0[0] = tag;
        assert!(
            validate_owned(&core, &plan, 1, map).is_err(),
            "selected tag {tag}"
        );
    }
}

#[test]
fn independent_state_and_unknown_actions_join_real_replay_and_change_dependencies() {
    // Explicit semantic oracle from XML; not calling modifies_state to compute expectations.
    for tag in [1, 2, 3, 4, 5, 6, 11, 14, 15, 17, 21, 0xfe, 0xff] {
        let (core, plan, mut map) = fixture();
        map.actions.get_mut(&14).unwrap()[0].0[0] = tag;
        let v = validate_owned(&core, &plan, 1, map).unwrap();
        assert!(v.modifiers.unwanted.contains(&14), "held action tag {tag}");
        assert!(v.snapshot.dependency_keys.contains(&14));
        assert!(v.snapshot.dependency_types.contains(&1));
        assert!(invalidates(
            &v.snapshot,
            &MapChange::Actions {
                first: 14,
                count: 1
            }
        ));
        assert!(invalidates(
            &v.snapshot,
            &MapChange::Types { first: 1, count: 1 }
        ));
        assert!(!invalidates(
            &v.snapshot,
            &MapChange::Actions {
                first: 15,
                count: 1
            }
        ));
        assert!(!invalidates(
            &v.snapshot,
            &MapChange::Types { first: 2, count: 1 }
        ));
        let cancel = AtomicBool::new(false);
        let candidate = Candidate {
            keycode: 9,
            state: 13,
            is_press: true,
            synthetic: false,
            held_keys: &[9, 10, 11, 12, 14],
            modifiers: &v.modifiers,
        };
        assert_eq!(decide(&plan, &cancel, candidate), Decision::Replay);
        assert!(!cancel.load(Ordering::SeqCst));
    }
}

#[test]
fn independent_noaction_is_not_an_invented_state_dependency() {
    let (core, plan, map) = fixture();
    let v = validate_owned(&core, &plan, 1, map).unwrap();
    assert!(!v.modifiers.unwanted.contains(&14));
    assert!(!invalidates(
        &v.snapshot,
        &MapChange::Actions {
            first: 14,
            count: 1
        }
    ));
    assert!(!invalidates(
        &v.snapshot,
        &MapChange::Types { first: 1, count: 1 }
    ));
}

#[test]
fn modifier_map_mismatch_is_rejected_before_delivery_authorization() {
    let (core, plan, map) = fixture();
    validate_owned(&core, &plan, 1, map).unwrap();
    let (core, plan, mut map) = fixture();
    map.modifiers[10] = 8;
    assert!(validate_owned(&core, &plan, 1, map).is_err());
}

#[test]
fn nondefault_dependent_behavior_is_rejected_for_escape_and_modifiers() {
    for key in 9..=13 {
        let (core, plan, mut map) = fixture();
        map.behaviors.insert(key, [0, 0]);
        validate_owned(&core, &plan, 1, map).unwrap();
        let (core, plan, mut map) = fixture();
        map.behaviors.insert(key, [1, 0]);
        assert!(validate_owned(&core, &plan, 1, map).is_err());
    }
}

#[test]
fn dependent_type_and_entry_virtual_masks_are_resolved_by_production_validation() {
    for broken in [false, true] {
        let (core, plan, mut map) = fixture();
        map.virtuals[8] = 4;
        map.virtuals[1] = if broken { 0 } else { 4 };
        let ty = map.types.get_mut(&0).unwrap();
        ty.mask = 4;
        ty.virtuals = 0x0100;
        ty.entries.push(TypeEntry {
            active: true,
            mask: 4,
            level: 0,
            real: 0,
            virtuals: 0x0002,
        });
        let result = validate_owned(&core, &plan, 1, map);
        if broken {
            assert!(result.is_err(), "unresolved entry virtual bit");
        } else {
            result.expect("resolved type high bit and entry low bit");
        }
    }
    let (core, plan, mut map) = fixture();
    map.types.get_mut(&0).unwrap().virtuals = 0x0100;
    assert!(
        validate_owned(&core, &plan, 1, map).is_err(),
        "unresolved type virtual bit"
    );
}

#[test]
fn unknown_modifier_action_is_rejected_not_treated_as_noaction() {
    let (core, plan, map) = fixture();
    validate_owned(&core, &plan, 1, map).unwrap();
    let (core, plan, mut map) = fixture();
    map.actions.get_mut(&12).unwrap()[0].0[0] = 0xfe;
    assert!(validate_owned(&core, &plan, 1, map).is_err());
}
