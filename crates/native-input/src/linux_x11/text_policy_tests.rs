//! These tests exercise the production pure policy, not an emulated X server.
//! Server mapping notifications, XKB action/type preservation and application
//! decoding still need separate, real X11 validation by the independent tester.

use super::{
    native_keysym, plan_lease, restoration_allowed, validate_effective_keysym,
    wayland_session, Lease, LeaseChoice, MappingRow, PolicyError,
};
use std::ffi::OsStr;

fn vacant(keycode: u8, width: usize) -> MappingRow {
    MappingRow {
        keycode,
        core: vec![0; width],
        xkb: vec![0; width],
        is_modifier: false,
        is_pressed: false,
        has_actions: false,
    }
}

fn lease(keycode: u8, keysym: u64, width: usize) -> Lease {
    Lease {
        keycode,
        keysym,
        original: vec![0; width],
        installed: vec![keysym; width],
    }
}

fn installed(lease: &Lease) -> MappingRow {
    MappingRow {
        keycode: lease.keycode,
        core: lease.installed.clone(),
        xkb: lease.installed.clone(),
        is_modifier: false,
        is_pressed: false,
        has_actions: false,
    }
}

#[test]
fn native_text_preserves_case_punctuation_and_latin1() {
    for (scalar, expected) in [('a', 0x61), ('A', 0x41), ('!', 0x21), ('~', 0x7e),
                               (' ', 0x20), ('é', 0xe9), ('ÿ', 0xff)] {
        assert_eq!(native_keysym(scalar), Ok(expected), "scalar {scalar:?}");
    }
}

#[test]
fn native_text_represents_chinese_combining_and_supplementary_scalars() {
    for (scalar, expected) in [('中', 0x0100_4e2d), ('文', 0x0100_6587),
                               ('\u{301}', 0x0100_0301), ('😀', 0x0101_f600),
                               ('\u{10ffff}', 0x0110_ffff)] {
        assert_eq!(native_keysym(scalar), Ok(expected), "scalar U+{:04X}", scalar as u32);
    }
}

#[test]
fn native_text_keeps_return_and_tab_semantics() {
    assert_eq!(native_keysym('\n'), Ok(0xff0d));
    assert_eq!(native_keysym('\r'), Ok(0xff0d));
    assert_eq!(native_keysym('\t'), Ok(0xff09));
}

#[test]
fn impossible_control_scalars_are_explicit_errors_not_printable_or_clipboard() {
    for scalar in ['\0', '\u{1}', '\u{8}', '\u{1b}', '\u{7f}', '\u{80}', '\u{9f}'] {
        assert_eq!(native_keysym(scalar), Err(PolicyError::InvalidScalar), "U+{:04X}", scalar as u32);
    }
}

#[test]
fn empty_wayland_display_does_not_reject_x11() {
    assert!(!wayland_session(None, None));
    assert!(!wayland_session(Some(OsStr::new("")), Some("x11")));
    assert!(!wayland_session(Some(OsStr::new("")), None));
}

#[test]
fn nonempty_wayland_display_or_wayland_session_is_rejected() {
    assert!(wayland_session(Some(OsStr::new("wayland-0")), Some("x11")));
    assert!(wayland_session(Some(OsStr::new(" ")), None));
    assert!(wayland_session(None, Some("WaYlAnD")));
    assert!(wayland_session(Some(OsStr::new("")), Some("wayland")));
}

#[cfg(unix)]
#[test]
fn non_utf8_nonempty_wayland_display_is_still_rejected() {
    use std::os::unix::ffi::OsStrExt;
    assert!(wayland_session(Some(OsStr::from_bytes(&[0xff])), None));
}

#[test]
fn unbound_unicode_reserves_a_vacant_slot_without_a_shift_chord() {
    let rows = [vacant(248, 4)];
    let expected = lease(248, 0x0100_4e2d, 4);
    assert_eq!(plan_lease(expected.keysym, &rows, &[]), Ok(LeaseChoice::Install(expected)));
    assert_eq!(rows[0].core, vec![0; 4], "planning must not mutate server snapshots");
}

#[test]
fn uppercase_is_an_exact_symbol_not_a_lowercase_physical_key() {
    assert_eq!(plan_lease(0x41, &[vacant(248, 2)], &[]),
               Ok(LeaseChoice::Install(lease(248, 0x41, 2))));
}

#[test]
fn same_scalar_reuses_its_verified_stable_lease() {
    let owned = lease(248, 0x0100_4e2d, 4);
    let row = installed(&owned);
    assert_eq!(plan_lease(owned.keysym, &[row], &[owned]), Ok(LeaseChoice::Reuse(248)));
}

#[test]
fn different_scalar_never_repurposes_an_existing_lease() {
    let owned = lease(248, 0x0100_4e2d, 4);
    let rows = [installed(&owned), vacant(249, 4)];
    assert_eq!(plan_lease(0x0100_6587, &rows, &[owned]),
               Ok(LeaseChoice::Install(lease(249, 0x0100_6587, 4))));
}

#[test]
fn occupied_modifier_held_and_xkb_action_slots_are_not_borrowed() {
    let mut occupied = vacant(240, 4);
    occupied.core[3] = 0x41; // Checking only column zero is insufficient.
    let mut xkb_occupied = vacant(241, 4);
    xkb_occupied.xkb[1] = 0x42;
    let mut modifier = vacant(242, 4);
    modifier.is_modifier = true;
    let mut held = vacant(243, 4);
    held.is_pressed = true;
    let mut action = vacant(244, 4);
    action.has_actions = true;
    let rows = [occupied, xkb_occupied, modifier, held, action, vacant(245, 4)];
    assert_eq!(plan_lease(0x0100_4e2d, &rows, &[]),
               Ok(LeaseChoice::Install(lease(245, 0x0100_4e2d, 4))));
}

#[test]
fn exhausted_mapping_capacity_is_explicit_not_eviction_or_clipboard() {
    let owned = lease(248, 0x0100_4e2d, 4);
    assert_eq!(plan_lease(0x0100_6587, &[installed(&owned)], &[owned]),
               Err(PolicyError::MappingExhausted));
    assert_eq!(plan_lease(0x0100_6587, &[], &[]), Err(PolicyError::MappingExhausted));
}

#[test]
fn zero_width_and_out_of_protocol_keycodes_are_rejected() {
    assert_eq!(plan_lease(0x41, &[vacant(248, 0)], &[]), Err(PolicyError::InvalidMapping));
    assert_eq!(plan_lease(0x41, &[vacant(7, 4)], &[]), Err(PolicyError::InvalidMapping));
}

#[test]
fn lost_lease_is_not_reused_even_if_another_vacant_slot_exists() {
    let owned = lease(248, 0x0100_4e2d, 4);
    let mut changed = installed(&owned);
    changed.core[0] = 0x42;
    assert_eq!(plan_lease(owned.keysym, &[changed, vacant(249, 4)], &[owned]),
               Err(PolicyError::OwnershipLost));
}

#[test]
fn missing_owned_keycode_is_ownership_loss() {
    let owned = lease(248, 0x41, 4);
    assert_eq!(plan_lease(0x41, &[vacant(249, 4)], &[owned]), Err(PolicyError::OwnershipLost));
}

#[test]
fn core_and_xkb_effective_lookup_must_agree_with_the_requested_scalar() {
    assert_eq!(validate_effective_keysym(0x41, 0x41, 0x41), Ok(()));
    assert_eq!(validate_effective_keysym(0x41, 0x41, 0x61), Err(PolicyError::MappingMismatch));
    assert_eq!(validate_effective_keysym(0x0100_4e2d, 0x61, 0x0100_4e2d),
               Err(PolicyError::MappingMismatch));
    assert_eq!(validate_effective_keysym(0x41, 0, 0), Err(PolicyError::MappingMismatch));
}

#[test]
fn restoration_requires_still_owned_mapping_and_released_key() {
    let owned = lease(248, 0x0100_4e2d, 4);
    assert_eq!(restoration_allowed(&owned, &installed(&owned), false), Ok(()));
    assert_eq!(owned.original, vec![0; 4], "complete original row retained for restoration");
}

#[test]
fn restoration_does_not_overwrite_another_clients_mapping() {
    let owned = lease(248, 0x0100_4e2d, 4);
    let mut changed = installed(&owned);
    changed.core[3] = 0x42;
    assert_eq!(restoration_allowed(&owned, &changed, false), Err(PolicyError::OwnershipLost));
}

#[test]
fn restoration_detects_xkb_change_even_when_core_row_is_unchanged() {
    let owned = lease(248, 0x41, 4);
    let mut changed = installed(&owned);
    changed.xkb[1] = 0x61;
    assert_eq!(restoration_allowed(&owned, &changed, false), Err(PolicyError::OwnershipLost));
    changed = installed(&owned);
    changed.has_actions = true;
    assert_eq!(restoration_allowed(&owned, &changed, false), Err(PolicyError::OwnershipLost));
}

#[test]
fn restoration_rejects_a_new_modifier_assignment_or_wrong_slot() {
    let owned = lease(248, 0x41, 4);
    let mut changed = installed(&owned);
    changed.is_modifier = true;
    assert_eq!(restoration_allowed(&owned, &changed, false), Err(PolicyError::OwnershipLost));
    changed = installed(&owned);
    changed.keycode = 249;
    assert_eq!(restoration_allowed(&owned, &changed, false), Err(PolicyError::OwnershipLost));
}

#[test]
fn restoration_and_reuse_reject_a_physically_held_slot() {
    let owned = lease(248, 0x41, 4);
    let mut held = installed(&owned);
    held.is_pressed = true;
    assert_eq!(restoration_allowed(&owned, &held, false), Err(PolicyError::KeyStillDown));
    assert_eq!(plan_lease(0x41, &[held], &[owned]), Err(PolicyError::KeyStillDown));
}

#[test]
fn failed_owned_release_prevents_restoration_even_if_snapshot_says_up() {
    let owned = lease(248, 0x41, 4);
    assert_eq!(restoration_allowed(&owned, &installed(&owned), true),
               Err(PolicyError::KeyStillDown));
}
