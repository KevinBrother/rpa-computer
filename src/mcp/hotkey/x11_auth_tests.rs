use super::x11_auth::{local_display_number, select_cookie};

fn record(family: u16, host: &[u8], display: &[u8], name: &[u8], cookie: &[u8]) -> Vec<u8> {
    let mut result = family.to_be_bytes().to_vec();
    for field in [host, display, name, cookie] {
        result.extend_from_slice(&(field.len() as u16).to_be_bytes());
        result.extend_from_slice(field);
    }
    result
}

#[test]
fn local_display_only_rejects_remote_or_malformed_addresses() {
    for display in [":7", ":7.0", "unix:7", "unix/:7.1"] {
        assert_eq!(local_display_number(display).unwrap(), "7");
    }
    for display in [
        "localhost:7",
        "host:0",
        "tcp/host:0",
        ":",
        ":7.bad",
        ":-1",
        ":7.0.1",
        " :7 ",
    ] {
        assert!(local_display_number(display).is_err(), "{display}");
    }
}

#[test]
fn binary_cookie_with_zero_bytes_is_preserved_and_exact_record_is_selected() {
    let cookie = [0, 1, 2, 0, 4, 5, 6, 7, 8, 9, 10, 0, 12, 13, 14, 255];
    let mut records = record(256, b"host", b"9", b"MIT-MAGIC-COOKIE-1", &[99; 16]);
    records.extend(record(256, b"host", b"7", b"MIT-MAGIC-COOKIE-1", &cookie));
    assert_eq!(
        select_cookie(&records, b"host", "7").unwrap(),
        Some(cookie.to_vec())
    );
    assert!(select_cookie(&records, b"other", "7").unwrap().is_none());
}

#[test]
fn wildcard_family_is_allowed_but_remote_family_cookie_is_not_reused() {
    let remote = record(0, b"host", b"7", b"MIT-MAGIC-COOKIE-1", &[1; 16]);
    assert!(select_cookie(&remote, b"host", "7").unwrap().is_none());
    let wildcard = record(65535, b"", b"7", b"MIT-MAGIC-COOKIE-1", &[2; 16]);
    assert_eq!(
        select_cookie(&wildcard, b"host", "7").unwrap(),
        Some(vec![2; 16])
    );
}

#[test]
fn truncated_records_and_invalid_cookie_lengths_are_errors_not_panics() {
    let bytes = record(256, b"host", b"7", b"MIT-MAGIC-COOKIE-1", &[3; 16]);
    for length in 1..bytes.len() {
        assert!(select_cookie(&bytes[..length], b"host", "7").is_err());
    }
    let invalid = record(256, b"host", b"7", b"MIT-MAGIC-COOKIE-1", &[3; 15]);
    assert!(select_cookie(&invalid, b"host", "7").is_err());
    assert!(select_cookie(&[], b"host", "7").unwrap().is_none());
}
