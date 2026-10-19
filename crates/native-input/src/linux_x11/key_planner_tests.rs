//! Physical modifier keys and text have intentionally different lookup rules.
use super::*;

#[test]
fn physical_named_modifiers_resolve_modifier_bound_keycodes() {
    for symbol in [XK_CONTROL_L, XK_SHIFT_L, XK_ALT_L, XK_SUPER_L] {
        let modifiers = HashSet::from([37]);
        assert_eq!(find_physical_keycode(&[symbol, 0], 2, 37, &modifiers, symbol), Some(37));
        assert_eq!(find_unshifted_keycode(&[symbol, 0], 2, 37, &modifiers, symbol), None,
                   "text lookup must continue excluding modifiers");
    }
}

#[test]
fn physical_character_never_borrows_a_modifier_bound_key() {
    let modifiers = HashSet::from([37]);
    assert_eq!(find_physical_keycode(&[0x61, 0], 2, 37, &modifiers, 0x61), None);
    assert_eq!(find_physical_keycode(&[XK_RETURN, 0], 2, 37, &modifiers, XK_RETURN), None);
}

#[test]
fn physical_lookup_still_requires_an_exact_base_level_binding() {
    assert_eq!(find_physical_keycode(&[0x61, 0x41], 2, 38, &HashSet::new(), 0x41), None);
    assert_eq!(find_physical_keycode(&[0x61, 0x41], 2, 38, &HashSet::new(), 0x61), Some(38));
}
