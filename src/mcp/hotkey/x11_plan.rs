//! Pure X11 keyboard/modifier planning and emergency-event classification.
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct KeyboardMap {
    pub first_keycode: u8,
    pub keysyms_per_keycode: usize,
    pub keysyms: Vec<u32>,
    /// Shift, Lock, Control, Mod1, Mod2, Mod3, Mod4, Mod5; zero is padding.
    pub modifier_keycodes: [Vec<u8>; 8],
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct HotkeyPlan {
    pub keycodes: Vec<u8>,
    pub modifier_masks: Vec<u16>,
    pub requires_identity: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Modifier {
    Control,
    Alt,
    Shift,
    Meta,
    Lock,
    Other,
}

pub(super) fn modifier(symbol: u32) -> Modifier {
    match symbol {
        0xffe3 | 0xffe4 => Modifier::Control,
        0xffe9 | 0xffea => Modifier::Alt,
        0xffe1 | 0xffe2 => Modifier::Shift,
        0xffe7 | 0xffe8 => Modifier::Meta,
        0xffe5 | 0xff7f => Modifier::Lock, // Caps_Lock, Num_Lock only.
        _ => Modifier::Other,              // In particular Super and ISO_Level3_Shift.
    }
}

pub(super) fn plan(map: &KeyboardMap) -> Result<HotkeyPlan, String> {
    let width = map.keysyms_per_keycode;
    if width == 0 || map.keysyms.is_empty() || map.keysyms.len() % width != 0 {
        return Err("invalid X11 keyboard mapping dimensions".into());
    }
    let count = map.keysyms.len() / width;
    if map.first_keycode < 8 || count > 256 - usize::from(map.first_keycode) {
        return Err("X11 keyboard mapping exceeds physical keycode range".into());
    }
    let primary = |key: u8| identity(map, key);
    let keycodes: Vec<_> = map
        .keysyms
        .chunks_exact(width)
        .enumerate()
        .filter(|(_, row)| row[0] == 0xff1b)
        .map(|(row, _)| (usize::from(map.first_keycode) + row) as u8)
        .collect();
    if keycodes.is_empty() {
        return Err("X11 keyboard has no unshifted physical Escape key".into());
    }
    let (mut control, mut alt, mut shift, mut locks) = (0u16, 0u16, 0u16, 0u16);
    let mut requires_identity = false;
    for (slot, codes) in map.modifier_keycodes.iter().enumerate() {
        let kinds: Vec<_> = codes
            .iter()
            .copied()
            .filter(|key| *key != 0)
            .map(|key| primary(key).map(modifier).unwrap_or(Modifier::Other))
            .collect();
        let Some(&kind) = kinds
            .iter()
            .find(|&&kind| kind != Modifier::Other && kind != Modifier::Meta)
        else {
            continue;
        };
        // A modifier bit alone cannot distinguish Alt from a Super/AltGr/Shift
        // key assigned to the same slot. Never broaden the emergency shortcut.
        if kinds
            .iter()
            .any(|&other| other != kind && !(kind == Modifier::Alt && other == Modifier::Meta))
        {
            return Err(format!("ambiguous X11 hotkey modifier slot {slot}"));
        }
        requires_identity |= kinds.contains(&Modifier::Meta);
        match kind {
            Modifier::Control => control |= 1 << slot,
            Modifier::Alt => alt |= 1 << slot,
            Modifier::Shift => shift |= 1 << slot,
            Modifier::Lock => locks |= 1 << slot,
            Modifier::Other | Modifier::Meta => unreachable!(),
        }
    }
    if control == 0 || alt == 0 || shift == 0 {
        return Err(
            "X11 keyboard needs physical Control, Alt and Shift modifier assignments".into(),
        );
    }
    // Enumerating the eight protocol bits also covers left/right modifiers
    // in distinct slots, simultaneous presses, and all lock combinations.
    let allowed = control | alt | shift | locks;
    let modifier_masks = (0..=255u16)
        .filter(|mask| {
            mask & !allowed == 0 && mask & control != 0 && mask & alt != 0 && mask & shift != 0
        })
        .collect();
    Ok(HotkeyPlan {
        keycodes,
        modifier_masks,
        requires_identity,
    })
}

pub(super) fn handle_key_event(
    plan: &HotkeyPlan,
    cancel: &AtomicBool,
    keycode: u8,
    state: u16,
    is_press: bool,
    synthetic: bool,
) -> bool {
    // Button1..5 are not keyboard modifiers. Emergency stop must work during
    // a drag too. SendEvent is not physical input (XTEST is not distinguishable).
    if !plan.requires_identity
        && is_press
        && !synthetic
        && plan.keycodes.contains(&keycode)
        && plan.modifier_masks.contains(&(state & 0xff))
    {
        cancel.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// Explicit inputs keep tests from mutating process-global environment.
pub(super) fn check_session(
    display: Option<&str>,
    session_type: Option<&str>,
    wayland_display: Option<&str>,
) -> Result<(), String> {
    if session_type.is_some_and(|s| s.trim().eq_ignore_ascii_case("wayland"))
        || wayland_display.is_some_and(|s| !s.trim().is_empty())
    {
        return Err(
            "global emergency hotkey requires native X11; Wayland/XWayland is unsupported".into(),
        );
    }
    if display.is_none_or(|s| s.trim().is_empty()) {
        return Err(
            "global emergency hotkey requires a nonempty DISPLAY in the controlled X11 session"
                .into(),
        );
    }
    Ok(())
}

/// The first non-NoSymbol identifies the key. Secondary Meta on physical Alt
/// does not change its identity; primary Meta with secondary Alt stays Meta.
pub(super) fn identity(map: &KeyboardMap, key: u8) -> Option<u32> {
    let row = usize::from(key.checked_sub(map.first_keycode)?);
    let start = row.checked_mul(map.keysyms_per_keycode)?;
    map.keysyms
        .get(start..start.checked_add(map.keysyms_per_keycode)?)?
        .iter()
        .copied()
        .find(|symbol| *symbol != 0)
}
