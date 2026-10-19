use super::*;

#[test]
fn linux_controls_require_the_native_scalar_safety_path() {
    assert!(
        control_text_uses_scalar(),
        "Return/Tab text must not bypass neutral-state and pending-release checks"
    );
}

#[test]
fn control_scalars_have_explicit_native_keysyms_not_unicode_control_symbols() {
    assert_eq!(native_keysym('\r'), Ok(0xff0d));
    assert_eq!(native_keysym('\n'), Ok(0xff0d));
    assert_eq!(native_keysym('\t'), Ok(0xff09));
}
