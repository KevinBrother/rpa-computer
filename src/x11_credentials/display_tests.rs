use super::display::{parse_display_number, MAX_DISPLAY_DIGITS};
use super::FailureKind;
use std::ffi::c_int;

#[test]
fn local_display_numbers_above_u16_and_lock_fixture_range_are_valid() {
    for number in [0, 65535, 65536, 100001, 100999, 1234567] {
        assert_eq!(parse_display_number(&number.to_string()), Ok(number));
    }
}

#[test]
fn checked_c_int_maximum_is_accepted_without_truncation() {
    assert_eq!(
        parse_display_number(&c_int::MAX.to_string()),
        Ok(c_int::MAX)
    );
}

#[test]
fn bounded_leading_zeros_normalize_without_changing_numeric_identity() {
    assert_eq!(parse_display_number("000100001"), Ok(100001));
    assert_eq!(parse_display_number(&"0".repeat(MAX_DISPLAY_DIGITS)), Ok(0));
}

#[test]
fn signs_whitespace_unicode_digits_and_display_syntax_are_not_decimal_numbers() {
    for value in [
        "",
        "-1",
        "-0",
        "+1",
        " 77",
        "77 ",
        "７７",
        ":77",
        "77.0",
        "remote:77",
        "7\0",
    ] {
        assert_eq!(
            parse_display_number(value),
            Err(FailureKind::InvalidRequest),
            "{value:?}"
        );
    }
}

#[test]
fn c_int_overflow_or_excess_input_length_is_rejected_not_wrapped() {
    let overflow = (i64::from(c_int::MAX) + 1).to_string();
    for value in [
        overflow,
        "9".repeat(MAX_DISPLAY_DIGITS),
        "0".repeat(MAX_DISPLAY_DIGITS + 1),
    ] {
        assert_eq!(
            parse_display_number(&value),
            Err(FailureKind::InvalidRequest)
        );
    }
}
