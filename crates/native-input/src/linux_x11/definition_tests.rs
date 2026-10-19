use super::*;

fn saved(repeat: Option<bool>, explicit: u8) -> Definition {
    let mut def = Definition::for_symbol(0);
    def.explicit = explicit;
    def.repeat = repeat;
    def
}

#[test]
fn restoration_rejects_disabled_nonexplicit_repeat_becoming_enabled() {
    // Exact F5 preconditions: neither Interpret (0x10) nor AutoRepeat (0x20).
    let original = saved(Some(false), 0);
    let recomputed = saved(Some(true), 0);
    assert!(
        !original.restoration_matches(&recomputed),
        "map/explicit equality cannot conceal changed repeat controls"
    );
}

#[test]
fn lease_ownership_includes_external_repeat_changes() {
    let installed = saved(Some(false), 0x1f);
    let changed = saved(Some(true), 0x1f);
    assert_ne!(
        installed, changed,
        "external repeat change must prevent overwrite"
    );
}

#[test]
fn restoration_cannot_be_confirmed_without_repeat_observations() {
    let unknown = saved(None, 0);
    assert!(!unknown.restoration_matches(&unknown));
    assert!(!saved(Some(false), 0).restoration_matches(&unknown));
}

#[test]
fn exact_repeat_and_original_explicit_flags_can_confirm_restoration() {
    for repeat in [false, true] {
        let original = saved(Some(repeat), 0);
        assert!(original.restoration_matches(&original));
        let mut changed_flags = original.clone();
        changed_flags.explicit |= 0x20;
        assert!(
            !original.restoration_matches(&changed_flags),
            "do not permanently retain explicit-repeat as a workaround"
        );
    }
}

#[test]
fn guarded_restore_preserves_disabled_repeat_and_does_not_mutate_saved_flags() {
    let original = saved(Some(false), 0);
    let staged = original.guarded_restoration();
    assert_eq!(staged.repeat, Some(false));
    assert_eq!(staged.explicit, 0x30);
    assert_eq!(original.explicit, 0);
    assert!(!original.restoration_matches(&staged));
    let mut finalized = staged;
    finalized.explicit = original.explicit;
    assert!(original.restoration_matches(&finalized));
}
