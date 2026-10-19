use super::auth::select_cookie;
use super::FailureKind;

fn record(family: u16, host: &[u8], display: &[u8], protocol: &[u8], cookie: &[u8]) -> Vec<u8> {
    let mut bytes = family.to_be_bytes().to_vec();
    for field in [host, display, protocol, cookie] {
        bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
        bytes.extend_from_slice(field);
    }
    bytes
}

#[test]
fn parent_preserves_binary_cookie_including_nuls() {
    let bytes = [0, 1, 2, 3, 0, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0];
    let authority = record(256, b"host", b"77", b"MIT-MAGIC-COOKIE-1", &bytes);
    let cookie = select_cookie(&authority, b"host", "77").unwrap().unwrap();
    assert_eq!(cookie.as_bytes(), &bytes);
}

#[test]
fn familywild_and_first_match_policy_is_preserved_without_desktop_grant() {
    let mut authority = record(65535, b"ignored", b"77", b"MIT-MAGIC-COOKIE-1", &[1; 16]);
    authority.extend(record(256, b"host", b"77", b"MIT-MAGIC-COOKIE-1", &[2; 16]));
    assert_eq!(
        select_cookie(&authority, b"host", "77")
            .unwrap()
            .unwrap()
            .as_bytes(),
        &[1; 16]
    );
}

#[test]
fn wrong_host_display_or_protocol_returns_no_cookie() {
    for bytes in [
        record(256, b"other", b"77", b"MIT-MAGIC-COOKIE-1", &[1; 16]),
        record(256, b"host", b"78", b"MIT-MAGIC-COOKIE-1", &[1; 16]),
        record(256, b"host", b"77", b"OTHER", &[1; 16]),
    ] {
        assert!(select_cookie(&bytes, b"host", "77").unwrap().is_none());
    }
}

#[test]
fn malformed_trailing_record_or_cookie_length_fails_without_leaking_data() {
    let mut bytes = record(256, b"host", b"77", b"MIT-MAGIC-COOKIE-1", &[9; 16]);
    bytes.push(0); // Must validate remainder even after finding a matching record.
    assert_eq!(
        select_cookie(&bytes, b"host", "77").unwrap_err(),
        FailureKind::MalformedAuthority
    );
    let bytes = record(256, b"host", b"77", b"MIT-MAGIC-COOKIE-1", &[9; 15]);
    assert_eq!(
        select_cookie(&bytes, b"host", "77").unwrap_err(),
        FailureKind::MalformedAuthority
    );
}

#[test]
fn authority_matching_preserves_display_numbers_above_65535() {
    let authority = record(256, b"host", b"100001", b"MIT-MAGIC-COOKIE-1", &[7; 16]);
    assert_eq!(
        select_cookie(&authority, b"host", "100001")
            .unwrap()
            .unwrap()
            .as_bytes(),
        &[7; 16]
    );
    assert!(
        select_cookie(&authority, b"host", "34465")
            .unwrap()
            .is_none(),
        "display number must not be truncated to 16 bits"
    );
}
