//! Exact physical-modifier decision for a frozen synchronous passive grab.
use super::x11_plan::{identity, modifier, HotkeyPlan, KeyboardMap, Modifier};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct ModifierKeys {
    pub control: Vec<u8>,
    pub alt: Vec<u8>,
    pub shift: Vec<u8>,
    pub unwanted: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Cancel,
    Replay,
}

pub(super) struct Candidate<'a> {
    pub keycode: u8,
    pub state: u16,
    pub is_press: bool,
    pub synthetic: bool,
    pub held_keys: &'a [u8],
    pub modifiers: &'a ModifierKeys,
}

pub(super) fn decide(plan: &HotkeyPlan, cancel: &AtomicBool, candidate: Candidate<'_>) -> Decision {
    let held = |codes: &[u8]| codes.iter().any(|key| candidate.held_keys.contains(key));
    if candidate.is_press
        && !candidate.synthetic
        && plan.keycodes.contains(&candidate.keycode)
        && plan.modifier_masks.contains(&(candidate.state & 0xff))
        && held(&candidate.modifiers.control)
        && held(&candidate.modifiers.alt)
        && held(&candidate.modifiers.shift)
        && !held(&candidate.modifiers.unwanted)
    {
        cancel.store(true, Ordering::SeqCst);
        Decision::Cancel
    } else {
        Decision::Replay
    }
}

pub(super) fn modifier_keys(map: &KeyboardMap) -> ModifierKeys {
    let mut keys = ModifierKeys::default();
    for key in map
        .modifier_keycodes
        .iter()
        .flatten()
        .copied()
        .filter(|key| *key != 0)
    {
        let bucket = match identity(map, key).map(modifier) {
            Some(Modifier::Control) => &mut keys.control,
            Some(Modifier::Alt) => &mut keys.alt,
            Some(Modifier::Shift) => &mut keys.shift,
            Some(Modifier::Lock) => continue,
            _ => &mut keys.unwanted,
        };
        if !bucket.contains(&key) {
            bucket.push(key);
        }
    }
    keys
}
