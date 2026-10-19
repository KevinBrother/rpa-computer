//! Supplemental assertion RED for behavior not covered by the original C14.
//! No X server, alternate serializer, or model-only ownership registry.
use super::*;
use crate::InputError;

fn native_definition() -> Definition {
    let mut value = Definition::for_symbol(0x01004e2d);
    value.repeat = Some(false);
    value
}
fn assert_rejected(value: &Definition) {
    let result = validate_definition(value);
    assert!(
        result.is_err(),
        "unsupported payload must be rejected before construction/submission"
    );
}
fn transport_error() -> InputError {
    InputError::new(
        "input_failed",
        "XCB connection lost after map submission; effects unknown",
    )
}
fn protocol_error() -> InputError {
    InputError::new("input_failed", "SetMap protocol BadValue")
}

#[test]
fn nonempty_actions_including_noaction_must_not_be_silently_cleared() {
    let mut value = native_definition();
    value.actions = vec![[0; 8]];
    assert_rejected(&value);
}
#[test]
fn nonzero_behavior_data_must_not_be_silently_cleared() {
    let mut value = native_definition();
    value.behavior = [0, 1];
    assert_rejected(&value);
}
#[test]
fn nonzero_real_modifier_map_must_not_be_silently_cleared() {
    let mut value = native_definition();
    value.modmap = 4;
    assert_rejected(&value);
}
#[test]
fn nonzero_virtual_modifier_map_must_not_be_silently_cleared() {
    let mut value = native_definition();
    value.vmodmap = 1;
    assert_rejected(&value);
}
#[test]
fn keysym_must_fit_card32_before_public_constructor() {
    let result = validate_symbols(1, 1, [u64::from(u32::MAX) + 1].into_iter());
    assert!(
        result.is_err(),
        "wire CARD32 cannot truncate a native unsigned long"
    );
}
#[test]
fn symbol_count_must_fit_card16_before_public_constructor() {
    // Bounded 512 KiB; no pathological allocation or server request.
    let symbols = vec![0; usize::from(u16::MAX) + 1];
    assert!(
        validate_symbols(1, 1, symbols.into_iter()).is_err(),
        "generated KeySymMapBuf constructor narrows symbol count: validate first"
    );
}
#[test]
fn group_count_over_four_is_rejected_before_public_constructor() {
    assert!(validate_symbols(5, 1, [0; 5].into_iter()).is_err());
}
#[test]
fn group_width_symbol_geometry_is_validated_before_public_constructor() {
    assert!(validate_symbols(1, 2, [0].into_iter()).is_err());
}
#[test]
fn null_protocol_error_does_not_make_transport_loss_success() {
    let result = complete(Ok(()), Err(transport_error()));
    assert!(
        result.is_err(),
        "request check without a healthy connection is not acknowledgment"
    );
}
#[test]
fn transport_loss_is_not_hidden_behind_a_protocol_error() {
    let result = complete(Err(protocol_error()), Err(transport_error())).unwrap_err();
    assert!(
        result.message.contains("connection lost"),
        "transport uncertainty must remain visible, got: {result}"
    );
}

// Five controls: accepted geometry/results, not wire serialization evidence.
#[test]
fn ordinary_action_empty_unicode_payload_is_accepted() {
    validate_definition(&native_definition()).unwrap();
}
#[test]
fn empty_original_symbols_and_explicit_zero_are_representable() {
    let mut value = native_definition();
    value.symbols.clear();
    value.group_info = 0;
    value.width = 0;
    value.explicit = 0;
    validate_definition(&value).unwrap();
}
#[test]
fn restoration_may_have_different_symbol_count_from_install() {
    let mut value = native_definition();
    value.symbols = vec![0, 0];
    value.width = 2;
    value.types[0] = 1;
    value.explicit = 0;
    validate_definition(&value).unwrap();
}
#[test]
fn healthy_checked_completion_is_success() {
    complete(Ok(()), Ok(())).unwrap();
}
#[test]
fn protocol_error_is_preserved_on_a_healthy_connection() {
    let error = complete(Err(protocol_error()), Ok(())).unwrap_err();
    assert_eq!(error.code, "input_failed");
    assert_eq!(error.message, "SetMap protocol BadValue");
}

#[test]
fn bridge_error_before_callback_is_uncertain_without_connection_lost_text() {
    let error = InputError::new("input_failed", "native XCB bridge is unavailable");
    let result = complete(Ok(()), Err(error)).unwrap_err();
    assert_eq!(result.code, "input_failed");
    assert_eq!(result.message, "native XCB bridge is unavailable");
}
