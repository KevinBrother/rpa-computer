use super::*;
use std::time::Duration;

fn request(path: Option<&Path>) -> Request<'_> {
    Request {
        authority_path: path,
        display_number: "77",
        hostname: b"fixture-host",
    }
}

#[test]
fn expired_call_is_timeout_not_a_late_credential_result() {
    let now = Instant::now();
    let error = acquire(
        request(Some(Path::new("/not-opened-by-this-scaffold"))),
        now,
    )
    .expect_err("expired setup must fail");
    assert_eq!(
        error,
        Failure {
            kind: FailureKind::Timeout,
            cleanup: Cleanup::NotStarted
        }
    );
}

#[test]
fn absent_path_returns_no_cookie_without_starting_helper() {
    let outcome = acquire(request(None), Instant::now() + Duration::from_secs(1)).expect(
        "no authority path is an explicitly unauthenticated attempt, not an implicit grant",
    );
    assert!(outcome.cookie.is_none());
    assert_eq!(outcome.cleanup, Cleanup::NotStarted);
}

#[test]
fn malformed_display_is_rejected_without_process_or_filesystem_work() {
    for display_number in ["", "-1", "2147483648", "remote:0", "77.0"] {
        let error = acquire(
            Request {
                display_number,
                ..request(None)
            },
            Instant::now() + Duration::from_secs(1),
        )
        .expect_err("normalized local display number required");
        assert_eq!(
            error,
            Failure {
                kind: FailureKind::InvalidRequest,
                cleanup: Cleanup::NotStarted
            }
        );
    }
}

#[test]
fn oversized_parent_inputs_are_rejected_before_helper_setup() {
    let host = vec![b'h'; MAX_HOSTNAME_BYTES + 1];
    let error = acquire(
        Request {
            hostname: &host,
            ..request(None)
        },
        Instant::now() + Duration::from_secs(1),
    )
    .expect_err("hostname must fit precomputed parent storage");
    assert_eq!(error.kind, FailureKind::InvalidRequest);
    let path = "x".repeat(MAX_PATH_BYTES + 1);
    let error = acquire(
        request(Some(Path::new(&path))),
        Instant::now() + Duration::from_secs(1),
    )
    .expect_err("path must fit precomputed parent storage");
    assert_eq!(error.kind, FailureKind::InvalidRequest);
}

#[test]
fn credential_debug_is_redacted() {
    let cookie = Cookie(*b"no-private-logs!");
    assert_eq!(format!("{cookie:?}"), "Cookie([REDACTED])");
    assert_eq!(cookie.as_bytes(), b"no-private-logs!");
}

#[test]
fn high_local_display_without_authority_path_is_not_artificially_rejected() {
    let outcome = acquire(
        Request {
            display_number: "100001",
            ..request(None)
        },
        Instant::now() + Duration::from_secs(1),
    )
    .expect("lock fixtures above 100000 are valid local display numbers");
    assert!(outcome.cookie.is_none());
    assert_eq!(outcome.cleanup, Cleanup::NotStarted);
}
