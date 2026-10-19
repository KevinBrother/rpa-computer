//! Regression fixture for cc-hotkey-x11-1's unmodified Xvfb modifier map.
//!
//! These tests exercise the production planner/mask-only event entrypoint.
//! They do not connect to X11, rewrite keymaps, or substitute a mock backend.
//! The observed Alt rows are reproduced below. Dedicated Meta is tested both
//! with a primary symbol and a leading NoSymbol: the log identified key 205 as
//! Meta_L but did not print its complete keysym row.
use super::x11_plan::{handle_key_event, plan, KeyboardMap};
use std::sync::atomic::{AtomicBool, Ordering};

const CONTROL_L: u32 = 0xffe3;
const CONTROL_R: u32 = 0xffe4;
const ALT_L: u32 = 0xffe9;
const ALT_R: u32 = 0xffea;
const META_L: u32 = 0xffe7;
const META_R: u32 = 0xffe8;
const F12: u32 = 0xffc9;
const ESCAPE: u32 = 0xff1b;
const SHIFT: u16 = 1;
const CTRL: u16 = 1 << 2;
const MOD1: u16 = 1 << 3;
const CAPS: u16 = 1 << 1;
const NUM: u16 = 1 << 4;

fn set_row(map: &mut KeyboardMap, keycode: u8, symbols: &[u32]) {
    let start = usize::from(keycode - map.first_keycode) * map.keysyms_per_keycode;
    let row = &mut map.keysyms[start..start + map.keysyms_per_keycode];
    row.fill(0);
    row[..symbols.len()].copy_from_slice(symbols);
}

fn default_xvfb_map() -> KeyboardMap {
    let mut map = KeyboardMap {
        first_keycode: 8,
        keysyms_per_keycode: 8,
        keysyms: vec![0; 248 * 8],
        modifier_keycodes: std::array::from_fn(|_| Vec::new()),
    };
    for (code, symbol) in [
        (37, CONTROL_L),
        (105, CONTROL_R),
        (50, 0xffe1),  // Shift_L
        (62, 0xffe2),  // Shift_R
        (66, 0xffe5),  // Caps_Lock
        (77, 0xff7f),  // Num_Lock
        (203, 0xfe11), // ISO_Level5_Shift
        (133, 0xffeb), // Super_L
        (134, 0xffec), // Super_R
        (206, 0xffeb), // Additional Super_L
        (207, 0xffed), // Hyper_L
        (92, 0xfe03),  // ISO_Level3_Shift
        (95, 0xffc8),  // F11, not the emergency key
    ] {
        set_row(&mut map, code, &[symbol]);
    }
    set_row(&mut map, 64, &[ALT_L, META_L, ALT_L, META_L]);
    set_row(&mut map, 108, &[ALT_R, META_R, ALT_R, META_R]);
    set_row(&mut map, 204, &[0, ALT_L, 0, ALT_L]);
    set_row(&mut map, 205, &[0, META_L, 0, META_L]);
    set_row(&mut map, 9, &[ESCAPE]);
    set_row(&mut map, 96, &[F12, F12, F12, F12, F12, F12, 0x1008fe0c]);
    map.modifier_keycodes[0] = vec![50, 62];
    map.modifier_keycodes[1] = vec![66];
    map.modifier_keycodes[2] = vec![37, 105];
    map.modifier_keycodes[3] = vec![64, 108, 204, 205];
    map.modifier_keycodes[4] = vec![77];
    map.modifier_keycodes[5] = vec![203];
    map.modifier_keycodes[6] = vec![133, 134, 206, 207];
    map.modifier_keycodes[7] = vec![92];
    map
}

#[test]
fn untouched_default_xvfb_map_must_produce_real_registration_candidates() {
    let map = default_xvfb_map();
    let result = plan(&map).expect("default X11 Alt/Meta aliases must be supported, not rejected");
    assert_eq!(result.keycodes, vec![9]);
    assert_eq!(result.modifier_masks, vec![13, 15, 29, 31]);
}

#[test]
fn secondary_meta_symbols_on_physical_alt_keys_do_not_make_them_meta_only() {
    let mut map = default_xvfb_map();
    map.modifier_keycodes[3] = vec![64, 108];
    let result = plan(&map).expect("the actual Alt_L/Alt_R rows are valid Alt identities");
    assert_eq!(result.modifier_masks, vec![13, 15, 29, 31]);
}

#[test]
fn leading_nosymbol_alt_member_is_not_discarded_as_unusable() {
    let mut map = default_xvfb_map();
    // Do not assume key 204 is unreachable, erase it, or silently ignore it.
    // Its Alt identity and modifier membership must stand on their own.
    map.modifier_keycodes[3] = vec![204];
    let result = plan(&map).expect("NoSymbol/Alt_L still identifies a mapped Alt key");
    assert_eq!(result.modifier_masks, vec![13, 15, 29, 31]);
}

#[test]
fn dedicated_meta_may_share_mod1_with_alt_without_blocking_registration() {
    for meta_row in [&[META_L, 0, META_L][..], &[0, META_L, 0, META_L][..]] {
        let mut map = default_xvfb_map();
        map.modifier_keycodes[3] = vec![64, 108, 205];
        set_row(&mut map, 205, meta_row);
        let result =
            plan(&map).expect("shared Mod1 needs physical-state resolution, not rejection");
        assert_eq!(result.modifier_masks, vec![13, 15, 29, 31]);
    }
}

#[test]
fn meta_only_modifier_assignment_cannot_substitute_for_physical_alt() {
    for meta_row in [&[META_L, 0, META_L][..], &[0, META_L, 0, META_L][..]] {
        let mut map = default_xvfb_map();
        map.modifier_keycodes[3] = vec![205];
        set_row(&mut map, 205, meta_row);
        assert!(
            plan(&map).is_err(),
            "Meta must never be globally reclassified as Alt"
        );
    }
}

#[test]
fn primary_meta_key_with_secondary_alt_is_not_reclassified_as_an_alt_key() {
    let mut map = default_xvfb_map();
    map.modifier_keycodes[3] = vec![205];
    set_row(&mut map, 205, &[META_L, ALT_L, META_L, ALT_L]);
    assert!(
        plan(&map).is_err(),
        "searching for any Alt keysym would accept this Meta key"
    );
}

#[test]
fn default_aliases_follow_server_modifier_slot_permutations() {
    for rotation in 0..8 {
        let mut map = default_xvfb_map();
        map.modifier_keycodes.rotate_right(rotation);
        let result = plan(&map).expect("default aliases must not depend on hardcoded Mod1");
        let bit = |slot: usize| 1u16 << ((slot + rotation) % 8);
        let base = bit(0) | bit(2) | bit(3);
        let mut expected = vec![base, base | bit(1), base | bit(4), base | bit(1) | bit(4)];
        expected.sort_unstable();
        assert_eq!(result.modifier_masks, expected);
    }
}

#[test]
fn default_escape_selection_does_not_grab_f12_vt_switch_or_f11() {
    let map = default_xvfb_map();
    let result = plan(&map).expect("Escape candidates must coexist with the untouched F12/VT row");
    assert_eq!(
        result.keycodes,
        vec![9],
        "do not grab F12, F11 or modifier keys"
    );
}

#[test]
fn shared_mod1_mask_without_key_identity_must_not_authorize_cancellation() {
    let result = plan(&default_xvfb_map()).expect("default map must reach event classification");
    for locks in [0, CAPS, NUM, CAPS | NUM] {
        for pointer_buttons in [0, 1 << 8, 0x1f00] {
            // This mask can mean Ctrl+Alt+Shift, Ctrl+Meta+Shift, or all four.
            // The mask-only API has no physical-state evidence to choose one.
            // The future native path must resolve identity under a synchronous
            // grab, then replay non-matches instead of consuming them.
            let cancel = AtomicBool::new(false);
            assert!(
                !handle_key_event(
                    &result,
                    &cancel,
                    9,
                    CTRL | MOD1 | SHIFT | locks | pointer_buttons,
                    true,
                    false
                ),
                "shared Mod1 alone must not produce Ctrl+Meta false activation"
            );
            assert!(!cancel.load(Ordering::SeqCst));
        }
    }
}

#[test]
fn unambiguous_alt_rows_preserve_existing_exact_combo_behavior() {
    let mut map = default_xvfb_map();
    map.modifier_keycodes[3] = vec![64, 108];
    let result = plan(&map).unwrap();
    for locks in [0, CAPS, NUM, CAPS | NUM] {
        let cancel = AtomicBool::new(false);
        assert!(handle_key_event(
            &result,
            &cancel,
            9,
            CTRL | MOD1 | SHIFT | locks,
            true,
            false
        ));
        assert!(cancel.load(Ordering::SeqCst));
        let cancel = AtomicBool::new(false);
        assert!(
            !handle_key_event(&result, &cancel, 9, CTRL | MOD1 | locks, true, false),
            "missing Shift is not the intended combo"
        );
        assert!(!cancel.load(Ordering::SeqCst));
    }
}
