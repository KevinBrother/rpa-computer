use super::ipc::{AuthorityRead, ReplyGate, HEADER_BYTES};
use super::{FailureKind, MAX_AUTH_BYTES};
use std::time::{Duration, Instant};

fn frame(status: u8, body: &[u8]) -> Vec<u8> {
    let mut frame = b"XACR\x01\x00\x00\x00".to_vec();
    frame[5] = status;
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(body);
    frame
}

#[test]
fn partial_binary_reply_is_assembled_without_text_or_nul_conversion() {
    let now = Instant::now();
    let mut gate = ReplyGate::new(now + Duration::from_secs(1));
    let body = [0, 255, 0, 1, 2];
    for chunk in frame(0, &body).chunks(3) {
        gate.ingest(chunk, now).unwrap();
    }
    assert_eq!(
        gate.finish(now).unwrap(),
        AuthorityRead::Bytes(body.to_vec())
    );
}

#[test]
fn timeout_with_no_bytes_is_terminal_and_keeps_no_credentials() {
    let now = Instant::now();
    let mut gate = ReplyGate::new(now);
    assert_eq!(gate.expire(now), Err(FailureKind::Timeout));
    assert_eq!(gate.finish(now), Err(FailureKind::Timeout));
    assert_eq!(gate.buffered_bytes(), 0);
}

#[test]
fn late_complete_reply_cannot_revive_a_timed_out_attempt() {
    let now = Instant::now();
    let deadline = now + Duration::from_secs(1);
    let mut gate = ReplyGate::new(deadline);
    let _ = gate.expire(deadline);
    assert_eq!(
        gate.ingest(&frame(0, b"late-secret"), now),
        Err(FailureKind::Timeout),
        "even a stale earlier timestamp cannot reopen terminal timeout"
    );
    assert_eq!(gate.finish(now), Err(FailureKind::Timeout));
    assert_eq!(gate.buffered_bytes(), 0);
}

#[test]
fn complete_reply_received_at_deadline_is_not_success() {
    let now = Instant::now();
    let mut gate = ReplyGate::new(now);
    assert_eq!(
        gate.ingest(&frame(0, b"secret"), now),
        Err(FailureKind::Timeout)
    );
}

#[test]
fn buffered_reply_is_discarded_if_eof_arrives_after_deadline() {
    let now = Instant::now();
    let deadline = now + Duration::from_secs(1);
    let mut gate = ReplyGate::new(deadline);
    gate.ingest(&frame(0, b"buffered-secret"), now).unwrap();
    assert_eq!(gate.finish(deadline), Err(FailureKind::Timeout));
    assert_eq!(gate.buffered_bytes(), 0);
}

#[test]
fn oversized_declared_length_is_rejected_before_body_allocation() {
    let now = Instant::now();
    let mut gate = ReplyGate::new(now + Duration::from_secs(1));
    let mut header = frame(0, &[]);
    header[8..12].copy_from_slice(&((MAX_AUTH_BYTES + 1) as u32).to_be_bytes());
    assert_eq!(gate.ingest(&header, now), Err(FailureKind::Oversized));
    assert!(gate.buffered_bytes() <= HEADER_BYTES);
}

#[test]
fn extra_frame_or_trailing_bytes_are_protocol_failure_not_second_result() {
    let now = Instant::now();
    let mut gate = ReplyGate::new(now + Duration::from_secs(1));
    let mut bytes = frame(0, b"first");
    bytes.extend(frame(0, b"second"));
    assert_eq!(gate.ingest(&bytes, now), Err(FailureKind::Protocol));
}

#[test]
fn truncated_header_or_body_at_eof_is_not_partial_success() {
    let now = Instant::now();
    for length in [0, 3, 11, 13] {
        let mut gate = ReplyGate::new(now + Duration::from_secs(1));
        gate.ingest(&frame(0, b"data")[..length], now).unwrap();
        assert_eq!(gate.finish(now), Err(FailureKind::IncompleteReply));
    }
}

#[test]
fn missing_file_is_distinct_from_read_failure_and_not_regular() {
    let now = Instant::now();
    for (status, expected) in [
        (1, Ok(AuthorityRead::Missing)),
        (2, Err(FailureKind::ReadFailed)),
        (3, Err(FailureKind::NotRegular)),
        (4, Err(FailureKind::Oversized)),
    ] {
        let mut gate = ReplyGate::new(now + Duration::from_secs(1));
        // Status errors are surfaced at finish; framing is still validated.
        gate.ingest(&frame(status, &[]), now).unwrap();
        assert_eq!(gate.finish(now), expected);
    }
}

#[test]
fn malformed_magic_version_reserved_status_or_error_payload_is_rejected() {
    let now = Instant::now();
    for (offset, value) in [(0, b'?'), (4, 2), (6, 1), (5, 255)] {
        let mut bytes = frame(0, &[]);
        bytes[offset] = value;
        let mut gate = ReplyGate::new(now + Duration::from_secs(1));
        assert_eq!(gate.ingest(&bytes, now), Err(FailureKind::Protocol));
    }
    let mut gate = ReplyGate::new(now + Duration::from_secs(1));
    assert_eq!(
        gate.ingest(&frame(1, b"unexpected"), now),
        Err(FailureKind::Protocol)
    );
}
