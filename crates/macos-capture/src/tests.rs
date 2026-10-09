// Pure production seams only. No native backend, GUI, permissions or capture.
// Written for CC+GLM execution; Codex only compiles these tests.
use super::*;
use crate::flight::{Flight, FlightSlot, Poll};
use crate::selection::{select_targets, ApplicationId, DisplayId};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn request() -> CaptureRequest {
    CaptureRequest {
        display_id: 1,
        excluded_process_id: 42,
        width: 1512,
        height: 982,
        timeout: Duration::from_secs(2),
    }
}

#[test]
fn request_dimensions_budget_and_timeout_have_explicit_bounds() {
    assert!(request().validate().is_ok());
    for (width, height) in [
        (0, 10),
        (10, 0),
        (MAX_DIMENSION + 1, 1),
        (u32::MAX, u32::MAX),
        (MAX_DIMENSION, MAX_DIMENSION),
    ] {
        let req = CaptureRequest {
            width,
            height,
            ..request()
        };
        assert_eq!(req.validate().unwrap_err().kind, ErrorKind::InvalidRequest);
    }
    for timeout in [
        Duration::ZERO,
        MAX_TIMEOUT + Duration::from_nanos(1),
        Duration::MAX,
    ] {
        assert_eq!(
            CaptureRequest {
                timeout,
                ..request()
            }
            .validate()
            .unwrap_err()
            .kind,
            ErrorKind::InvalidRequest
        );
    }
    for pid in [0, i32::MAX as u32 + 1, u32::MAX] {
        assert_eq!(
            CaptureRequest {
                excluded_process_id: pid,
                ..request()
            }
            .validate()
            .unwrap_err()
            .kind,
            ErrorKind::InvalidRequest
        );
    }
    assert!(CaptureRequest {
        width: MAX_DIMENSION,
        height: 1,
        timeout: MAX_TIMEOUT,
        ..request()
    }
    .validate()
    .is_ok());
}

#[test]
fn filtering_selects_only_exact_pid_and_exact_display() {
    let displays = [DisplayId(9), DisplayId(1)];
    let apps = [ApplicationId(420), ApplicationId(42), ApplicationId(4)];
    let selected = select_targets(1, 42, &displays, &apps).unwrap();
    assert_eq!(selected.display_index, 1);
    assert_eq!(selected.application_index, 1);
    assert_eq!(
        select_targets(2, 42, &displays, &apps).unwrap_err().kind,
        ErrorKind::NoDisplay
    );
    assert_eq!(
        select_targets(1, 41, &displays, &apps).unwrap_err().kind,
        ErrorKind::ExcludedProcessMissing
    );
    assert_eq!(
        select_targets(1, 42, &displays, &[]).unwrap_err().kind,
        ErrorKind::ExcludedProcessMissing
    );
    assert_eq!(
        select_targets(1, 42, &displays, &[ApplicationId(42), ApplicationId(42)])
            .unwrap_err()
            .kind,
        ErrorKind::CaptureFailed
    );
}

#[test]
fn two_stages_share_one_absolute_deadline() {
    let start = Instant::now();
    let deadline = Deadline::new_at(start, Duration::from_secs(2)).unwrap();
    assert_eq!(
        deadline.remaining_at(start + Duration::from_millis(1500)),
        Some(Duration::from_millis(500))
    );
    assert!(deadline
        .remaining_at(start + Duration::from_secs(2))
        .is_none());
    assert!(deadline
        .remaining_at(start + Duration::from_secs(3))
        .is_none());
    assert!(Deadline::new_at(start, Duration::MAX).is_err());
}

#[test]
fn timeout_quarantines_flight_until_late_completion_without_stack_access() {
    let start = Instant::now();
    let deadline = Deadline::new_at(start, Duration::from_secs(1)).unwrap();
    let mut slot = FlightSlot::default();
    let flight = slot.begin(deadline).unwrap();
    let callback_owned = Arc::clone(&flight);
    assert!(matches!(
        flight.poll_at(start + Duration::from_secs(1)),
        Poll::Timeout
    ));
    assert_eq!(slot.begin(deadline).unwrap_err().kind, ErrorKind::Busy);
    assert!(!callback_owned.may_continue_at(start + Duration::from_secs(1)));
    callback_owned.complete_at(Ok(7_u32), start + Duration::from_secs(2));
    assert!(matches!(
        flight.poll_at(start + Duration::from_secs(2)),
        Poll::Timeout
    ));
    assert!(slot.begin(deadline).is_ok());
}

#[test]
fn missing_callback_stays_bounded_and_busy_not_retried() {
    let start = Instant::now();
    let deadline = Deadline::new_at(start, Duration::from_secs(1)).unwrap();
    let mut slot = FlightSlot::<u32>::default();
    let flight = slot.begin(deadline).unwrap();
    assert!(matches!(
        flight.poll_at(start + Duration::from_secs(1)),
        Poll::Timeout
    ));
    for _ in 0..1000 {
        assert_eq!(slot.begin(deadline).unwrap_err().kind, ErrorKind::Busy);
    }
}

#[test]
fn duplicate_completion_never_overwrites_first_or_panics() {
    let start = Instant::now();
    let flight = Flight::new(Deadline::new_at(start, Duration::from_secs(1)).unwrap());
    flight.complete_at(Ok(1_u32), start);
    flight.complete_at(Ok(2_u32), start);
    assert!(matches!(flight.poll_at(start), Poll::Ready(Ok(1))));
    assert!(matches!(flight.poll_at(start), Poll::Consumed));
}

#[test]
fn timeout_after_early_completion_does_not_return_a_late_success() {
    let start = Instant::now();
    let flight = Flight::new(Deadline::new_at(start, Duration::from_secs(1)).unwrap());
    flight.complete_at(Ok(3_u32), start);
    assert!(matches!(
        flight.poll_at(start + Duration::from_secs(2)),
        Poll::Timeout
    ));
}

#[test]
fn callback_after_client_drop_only_owns_heap_state() {
    let start = Instant::now();
    let callback = {
        let mut slot = FlightSlot::<u32>::default();
        slot.begin(Deadline::new_at(start, Duration::from_secs(1)).unwrap())
            .unwrap()
    };
    callback.complete_at(Ok(1), start);
    assert!(matches!(callback.poll_at(start), Poll::Ready(Ok(1))));
}

#[test]
fn failed_stage_is_completed_and_can_release_gate() {
    let start = Instant::now();
    let deadline = Deadline::new_at(start, Duration::from_secs(1)).unwrap();
    let mut slot = FlightSlot::<u32>::default();
    let flight = slot.begin(deadline).unwrap();
    flight.complete_at(
        Err(CaptureError::new(
            ErrorKind::PermissionDenied,
            Stage::Discovery,
        )),
        start,
    );
    assert!(matches!(
        flight.poll_at(start),
        Poll::Ready(Err(CaptureError {
            kind: ErrorKind::PermissionDenied,
            ..
        }))
    ));
    assert!(slot.begin(deadline).is_ok());
}

#[test]
fn png_header_uses_actual_dimensions_and_rejects_invalid_budget() {
    fn header(w: u32, h: u32) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(w.to_be_bytes());
        png.extend(h.to_be_bytes());
        // This helper tests the production header guard, not a complete decoder.
        png.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        png
    }
    assert_eq!(
        validate_png_header(&header(800, 600), 800, 600).unwrap(),
        (800, 600)
    );
    assert_eq!(
        validate_png_header(&header(800, 600), 1512, 982)
            .unwrap_err()
            .kind,
        ErrorKind::InvalidImage
    );
    for dimensions in [
        (0, 2),
        (MAX_DIMENSION + 1, 1),
        (MAX_DIMENSION, MAX_DIMENSION),
    ] {
        assert_eq!(
            validate_png_header(
                &header(dimensions.0, dimensions.1),
                dimensions.0,
                dimensions.1
            )
            .unwrap_err()
            .kind,
            ErrorKind::InvalidImage
        );
    }
    assert_eq!(
        validate_png_header(b"not png", 800, 600).unwrap_err().kind,
        ErrorKind::InvalidImage
    );
}

#[test]
fn errors_do_not_include_native_app_or_window_descriptions() {
    for kind in [
        ErrorKind::Unsupported,
        ErrorKind::PermissionDenied,
        ErrorKind::NoDisplay,
        ErrorKind::ExcludedProcessMissing,
        ErrorKind::InvalidRequest,
        ErrorKind::Busy,
        ErrorKind::Timeout,
        ErrorKind::CaptureFailed,
        ErrorKind::EncodingFailed,
        ErrorKind::InvalidImage,
    ] {
        let error = CaptureError::with_native(
            kind,
            Stage::Capture,
            NativeErrorDomain::ScreenCaptureKit,
            -3801,
        );
        assert!(!format!("{error}").contains("localized"));
        assert_eq!(error.native_code, Some(-3801));
        assert!(!error.code().is_empty());
    }
}

#[test]
fn apple_error_classification_is_production_mapping_not_message_text() {
    assert_eq!(
        classify_native_error(NativeErrorDomain::ScreenCaptureKit, -3801),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        classify_native_error(NativeErrorDomain::ScreenCaptureKit, -3814),
        ErrorKind::NoDisplay
    );
    assert_eq!(
        classify_native_error(NativeErrorDomain::Other, -3801),
        ErrorKind::CaptureFailed
    );
    assert_eq!(
        classify_native_error(NativeErrorDomain::ScreenCaptureKit, -9999),
        ErrorKind::CaptureFailed
    );
}

#[test]
fn duplicate_discovery_or_image_callback_cannot_start_additional_work() {
    let start = Instant::now();
    let flight = Flight::<u32>::new(Deadline::new_at(start, Duration::from_secs(1)).unwrap());
    assert!(flight.claim_discovery());
    assert!(!flight.claim_discovery());
    assert!(flight.claim_image());
    assert!(!flight.claim_image());
    flight.complete_at(Ok(1), start);
    assert!(!flight.claim_discovery());
    assert!(!flight.claim_image());
}

#[test]
fn old_callback_cannot_overwrite_a_new_flight() {
    let start = Instant::now();
    let deadline = Deadline::new_at(start, Duration::from_secs(1)).unwrap();
    let mut slot = FlightSlot::<u32>::default();
    let old = slot.begin(deadline).unwrap();
    old.complete_at(Ok(1), start);
    let new = slot.begin(deadline).unwrap();
    old.complete_at(Ok(99), start);
    assert!(matches!(new.poll_at(start), Poll::Pending));
    new.complete_at(Ok(2), start);
    assert!(matches!(new.poll_at(start), Poll::Ready(Ok(2))));
}

#[test]
fn new_client_is_rust_only_and_starts_with_no_flight() {
    let client = CaptureClient::new();
    assert!(!client.is_busy());
}

#[test]
fn oversized_native_content_lists_fail_closed() {
    assert_eq!(
        select_targets(
            1,
            42,
            &vec![DisplayId(1); crate::selection::MAX_DISPLAYS + 1],
            &[ApplicationId(42)]
        )
        .unwrap_err()
        .kind,
        ErrorKind::CaptureFailed
    );
    assert_eq!(
        select_targets(
            1,
            42,
            &[DisplayId(1)],
            &vec![ApplicationId(42); crate::selection::MAX_APPLICATIONS + 1]
        )
        .unwrap_err()
        .kind,
        ErrorKind::CaptureFailed
    );
}

#[test]
fn native_missing_entitlements_is_permission_not_a_successful_fallback() {
    assert_eq!(
        classify_native_error(NativeErrorDomain::ScreenCaptureKit, -3803),
        ErrorKind::PermissionDenied
    );
}
