//! Decision tests do not claim the server actually performs ReplayKeyboard.
use super::x11_candidate::{decide, Candidate, Decision, ModifierKeys};
use super::x11_plan::HotkeyPlan;
use super::LINUX_SHORTCUT_DESCRIPTION;
use std::sync::atomic::{AtomicBool, Ordering};

fn keys() -> ModifierKeys {
    ModifierKeys {
        control: vec![37, 105],
        alt: vec![64, 108, 204],
        shift: vec![50, 62],
        unwanted: vec![205, 133, 134, 206, 207, 92, 203],
    }
}

fn classify(
    held: &[u8],
    state: u16,
    keycode: u8,
    is_press: bool,
    synthetic: bool,
) -> (Decision, bool) {
    let cancel = AtomicBool::new(false);
    let plan = HotkeyPlan {
        keycodes: vec![9],
        modifier_masks: vec![13, 15, 29, 31],
        requires_identity: false,
    };
    let result = decide(
        &plan,
        &cancel,
        Candidate {
            keycode,
            state,
            is_press,
            synthetic,
            held_keys: held,
            modifiers: &keys(),
        },
    );
    (result, cancel.load(Ordering::SeqCst))
}

#[test]
fn linux_description_names_approved_escape_combo_not_f12() {
    assert_eq!(
        LINUX_SHORTCUT_DESCRIPTION,
        "Ctrl+Alt+Shift+Escape (Linux/X11 session)"
    );
}

#[test]
fn either_control_alt_and_shift_with_all_locks_and_pointer_buttons_cancel() {
    for control in [37, 105] {
        for alt in [64, 108, 204] {
            for shift in [50, 62] {
                for locks in [0, 2, 16, 18] {
                    for buttons in [0, 0x100, 0x1f00] {
                        assert_eq!(
                            classify(&[control, alt, shift], 13 | locks | buttons, 9, true, false),
                            (Decision::Cancel, true)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn dedicated_meta_in_shared_mod1_replays_without_setting_cancel() {
    for held in [&[37, 205, 50][..], &[37, 64, 205, 50][..]] {
        for locks in [0, 2, 16, 18] {
            assert_eq!(
                classify(held, 13 | locks, 9, true, false),
                (Decision::Replay, false),
                "Meta is not Alt and adding real Meta is not the intended chord"
            );
        }
    }
}

#[test]
fn effective_mask_without_each_required_physical_modifier_is_not_enough() {
    // Covers latched/locked masks and absent QueryKeymap evidence as well.
    for held in [&[][..], &[37, 64][..], &[37, 50][..], &[64, 50][..]] {
        assert_eq!(
            classify(held, 13, 9, true, false),
            (Decision::Replay, false)
        );
    }
}

#[test]
fn unwanted_held_modifier_cannot_hide_behind_an_accepted_mask() {
    for unwanted in keys().unwanted {
        assert_eq!(
            classify(&[37, 64, 50, unwanted], 13, 9, true, false),
            (Decision::Replay, false)
        );
    }
}

#[test]
fn every_wrong_core_modifier_mask_replays_and_never_cancels() {
    for mask in 0..=255 {
        if [13, 15, 29, 31].contains(&mask) {
            continue;
        }
        assert_eq!(
            classify(&[37, 64, 50], mask, 9, true, false),
            (Decision::Replay, false),
            "{mask:#x}"
        );
    }
}

#[test]
fn f12_plain_escape_releases_and_send_event_do_not_cancel() {
    for (key, mask, press, synthetic) in [
        (96, 13, true, false),
        (9, 0, true, false),
        (9, 12, true, false),
        (9, 13, false, false),
        (9, 13, true, true),
    ] {
        assert_eq!(
            classify(&[37, 64, 50], mask, key, press, synthetic),
            (Decision::Replay, false)
        );
    }
}
