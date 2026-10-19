//! Portable contracts for the production X11 planner. No connection or input.
use super::x11_plan::{check_session, handle_key_event, plan, HotkeyPlan, KeyboardMap};
use std::sync::atomic::{AtomicBool, Ordering};

const ESCAPE: u32 = 0xff1b;
const SHIFT: u16 = 1;
const CONTROL_L: u32 = 0xffe3;
const CONTROL_R: u32 = 0xffe4;
const ALT_L: u32 = 0xffe9;
const ALT_R: u32 = 0xffea;
const CAPS_LOCK: u32 = 0xffe5;
const NUM_LOCK: u32 = 0xff7f;
const CTRL: u16 = 1 << 2;
const ALT: u16 = 1 << 3;
const CAPS: u16 = 1 << 1;
const NUM: u16 = 1 << 4;

fn set_symbol(map: &mut KeyboardMap, keycode: u8, symbol: u32) {
    let offset = usize::from(keycode - map.first_keycode) * map.keysyms_per_keycode;
    map.keysyms[offset] = symbol;
}

fn keyboard() -> KeyboardMap {
    let mut map = KeyboardMap {
        first_keycode: 8,
        keysyms_per_keycode: 4,
        keysyms: vec![0; 128 * 4],
        modifier_keycodes: std::array::from_fn(|_| Vec::new()),
    };
    for (keycode, symbol) in [
        (95, ESCAPE),
        (37, CONTROL_L),
        (105, CONTROL_R),
        (64, ALT_L),
        (108, ALT_R),
        (66, CAPS_LOCK),
        (77, NUM_LOCK),
        (50, 0xffe1),  // Shift_L
        (133, 0xffeb), // Super_L
        (92, 0xfe03),  // ISO_Level3_Shift (AltGr, NOT Alt)
    ] {
        set_symbol(&mut map, keycode, symbol);
    }
    map.modifier_keycodes[0] = vec![50];
    map.modifier_keycodes[1] = vec![66];
    map.modifier_keycodes[2] = vec![37, 105];
    map.modifier_keycodes[3] = vec![64, 108];
    map.modifier_keycodes[4] = vec![77];
    map.modifier_keycodes[6] = vec![133];
    map.modifier_keycodes[7] = vec![92];
    map
}

#[test]
fn server_mapping_produces_exact_ctrl_alt_shift_and_four_lock_variants() {
    let actual = plan(&keyboard()).expect("X11 planner must support the standard map");
    assert_eq!(actual.keycodes, vec![95]);
    assert_eq!(actual.modifier_masks, vec![13, 15, 29, 31]);
}

#[test]
fn modifier_slot_permutations_do_not_assume_mod1_alt_or_mod2_numlock() {
    for rotation in 0..8 {
        let mut map = keyboard();
        map.modifier_keycodes.rotate_right(rotation);
        let actual = plan(&map).expect("physical modifier mapping must survive slot changes");
        let bit = |slot: usize| 1u16 << ((slot + rotation) % 8);
        let base = bit(0) | bit(2) | bit(3);
        let mut expected = vec![base, base | bit(1), base | bit(4), base | bit(1) | bit(4)];
        expected.sort_unstable();
        assert_eq!(actual.modifier_masks, expected, "slot rotation {rotation}");
    }
}

#[test]
fn left_and_right_control_in_different_slots_allow_either_or_both() {
    let mut map = keyboard();
    map.modifier_keycodes[2] = vec![37];
    map.modifier_keycodes[5] = vec![105];
    let actual = plan(&map).unwrap();
    let mut expected = Vec::new();
    for control in [CTRL, 1 << 5, CTRL | (1 << 5)] {
        for locks in [0, CAPS, NUM, CAPS | NUM] {
            expected.push(control | ALT | SHIFT | locks);
        }
    }
    expected.sort_unstable();
    assert_eq!(actual.modifier_masks, expected);
}

#[test]
fn right_alt_alone_is_valid_but_altgr_is_not_alt() {
    let mut map = keyboard();
    set_symbol(&mut map, 64, 0);
    map.modifier_keycodes[3] = vec![108];
    assert_eq!(plan(&map).unwrap().modifier_masks, vec![13, 15, 29, 31]);
    set_symbol(&mut map, 108, 0xfe03);
    assert!(
        plan(&map).is_err(),
        "AltGr-only layouts must not claim Ctrl+Alt"
    );
}

#[test]
fn multiple_escape_keycodes_are_sorted_and_keysym_columns_do_not_duplicate_grabs() {
    let mut map = keyboard();
    set_symbol(&mut map, 96, ESCAPE);
    let offset = usize::from(95 - map.first_keycode) * map.keysyms_per_keycode;
    map.keysyms[offset + 1] = ESCAPE;
    // Candidate enumeration alone is not XKB delivery validation.
    map.keysyms[offset + 2] = ESCAPE;
    assert_eq!(plan(&map).unwrap().keycodes, vec![95, 96]);
}

#[test]
fn shifted_only_escape_is_not_grabbed_as_an_unshifted_emergency_key() {
    let mut map = keyboard();
    set_symbol(&mut map, 96, 0xffc8); // F11
    let offset = usize::from(96 - map.first_keycode) * map.keysyms_per_keycode;
    map.keysyms[offset + 1] = ESCAPE;
    assert_eq!(plan(&map).unwrap().keycodes, vec![95]);
}

#[test]
fn absent_lock_keys_do_not_authorize_unrelated_modifier_bits() {
    let mut map = keyboard();
    map.modifier_keycodes[1].clear();
    map.modifier_keycodes[4].clear();
    assert_eq!(plan(&map).unwrap().modifier_masks, vec![CTRL | ALT | SHIFT]);
}

#[test]
fn duplicate_and_zero_modifier_entries_do_not_duplicate_grabs() {
    let mut map = keyboard();
    map.modifier_keycodes[2] = vec![0, 37, 105, 37, 0];
    map.modifier_keycodes[3] = vec![108, 64, 108, 0];
    assert_eq!(plan(&map).unwrap().modifier_masks, vec![13, 15, 29, 31]);
}

#[test]
fn required_keys_and_real_modifier_membership_are_required() {
    for codes in [&[95][..], &[37, 105][..], &[64, 108][..], &[50][..]] {
        let mut map = keyboard();
        for &code in codes {
            set_symbol(&mut map, code, 0);
        }
        assert!(plan(&map).is_err(), "missing physical keys: {codes:?}");
    }
    let mut map = keyboard();
    map.modifier_keycodes[3].clear();
    assert!(
        plan(&map).is_err(),
        "Alt keysym alone is not a modifier assignment"
    );
}

#[test]
fn mixed_modifier_slots_fail_closed_instead_of_accepting_super_or_altgr() {
    for alias in [133, 92, 50, 37, 66, 77] {
        let mut map = keyboard();
        map.modifier_keycodes[3].push(alias);
        assert!(
            plan(&map).is_err(),
            "ambiguous Alt slot with keycode {alias}"
        );
    }
}

#[test]
fn unrelated_key_in_an_ignored_lock_slot_must_not_be_silently_allowed() {
    let mut map = keyboard();
    map.modifier_keycodes[4].push(133);
    assert!(
        plan(&map).is_err(),
        "NumLock/Super alias cannot be safely ignored"
    );
}

#[test]
fn unknown_modifier_keycode_fails_closed() {
    let mut map = keyboard();
    map.modifier_keycodes[3].push(250);
    assert!(plan(&map).is_err());
    map.modifier_keycodes[3] = vec![64, 108, 100]; // In range but NoSymbol.
    assert!(plan(&map).is_err());
}

#[test]
fn malformed_keyboard_replies_are_rejected_without_panicking() {
    let mut map = keyboard();
    map.keysyms_per_keycode = 0;
    assert!(plan(&map).is_err());
    let mut map = keyboard();
    map.keysyms.pop();
    assert!(plan(&map).is_err());
    let mut map = keyboard();
    map.keysyms.clear();
    assert!(plan(&map).is_err());
    let mut map = keyboard();
    map.first_keycode = 250; // Rows run past protocol keycode 255.
    assert!(plan(&map).is_err());
}

// Event contracts intentionally bypass plan(): planner RED must not hide an
// independent event-matching defect, and this fixture is not a native backend.
fn standard_plan() -> HotkeyPlan {
    HotkeyPlan {
        keycodes: vec![95],
        modifier_masks: vec![13, 15, 29, 31],
        requires_identity: false,
    }
}

#[test]
fn only_exact_combo_with_optional_discovered_locks_sets_cancel() {
    let actual = standard_plan();
    for state in 0u16..=255 {
        let cancel = AtomicBool::new(false);
        let hit = handle_key_event(&actual, &cancel, 95, state, true, false);
        let expected = [13, 15, 29, 31].contains(&state);
        assert_eq!(hit, expected, "modifier state {state:#x}");
        assert_eq!(cancel.load(Ordering::SeqCst), expected);
    }
}

#[test]
fn pointer_button_state_does_not_disable_the_emergency_key() {
    let actual = standard_plan();
    for buttons in [1 << 8, 1 << 9, 0x1f00] {
        let cancel = AtomicBool::new(false);
        assert!(handle_key_event(
            &actual,
            &cancel,
            95,
            CTRL | ALT | SHIFT | buttons,
            true,
            false
        ));
        assert!(cancel.load(Ordering::SeqCst));
    }
}

#[test]
fn wrong_key_releases_and_send_event_synthetic_presses_do_not_cancel() {
    let actual = standard_plan();
    for (keycode, press, synthetic) in [(94, true, false), (95, false, false), (95, true, true)] {
        let cancel = AtomicBool::new(false);
        assert!(!handle_key_event(
            &actual,
            &cancel,
            keycode,
            CTRL | ALT | SHIFT,
            press,
            synthetic
        ));
        assert!(!cancel.load(Ordering::SeqCst));
    }
}

#[test]
fn repeated_presses_are_harmless_and_unrelated_events_never_clear_cancel() {
    let actual = standard_plan();
    let cancel = AtomicBool::new(false);
    for _ in 0..3 {
        assert!(handle_key_event(
            &actual,
            &cancel,
            95,
            CTRL | ALT | SHIFT,
            true,
            false
        ));
    }
    assert!(!handle_key_event(&actual, &cancel, 94, 0, true, false));
    assert!(cancel.load(Ordering::SeqCst));
}

#[test]
fn x11_session_accepts_empty_wayland_markers_and_inherited_display() {
    for session in [None, Some(""), Some("x11"), Some("X11")] {
        for wayland in [None, Some(""), Some("   ")] {
            assert!(check_session(Some(":77"), session, wayland).is_ok());
        }
    }
}

#[test]
fn wayland_and_xwayland_are_rejected_even_with_display_present() {
    for (session, wayland) in [
        (Some("wayland"), None),
        (Some("WAYLAND"), Some("")),
        (Some(" wayland "), Some("")),
        (Some("x11"), Some("wayland-0")),
        (None, Some("/run/user/1000/wayland-0")),
    ] {
        assert!(check_session(Some(":77"), session, wayland).is_err());
    }
}

#[test]
fn missing_or_blank_display_is_explicitly_rejected() {
    for display in [None, Some(""), Some("  ")] {
        assert!(check_session(display, Some("x11"), None).is_err());
    }
}
