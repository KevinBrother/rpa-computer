//! Existing dependency regression guards, not claimed as new-behavior RED evidence.
mod common;
use common::{frame, snapshot};
use rpa_desktop_feedback::{
    codec::{decode_host_line, decode_renderer_line, encode_host, encode_renderer},
    protocol::{CaptureExclusion, HostMessage, Ready, RendererMessage, V1},
};

#[test]
fn dependency_rejects_unknown_duplicate_missing_fields_and_wrong_types() {
    let bytes = frame(1);
    let valid = std::str::from_utf8(&bytes[..bytes.len() - 1]).unwrap();
    for invalid in [
        valid.replacen("\"version\":1", "\"version\":1,\"extra\":true", 1),
        valid.replacen("\"sequence\":1", "\"sequence\":1,\"sequence\":2", 1),
        valid.replace("\"pointer\":null", "\"other\":null"),
        valid.replacen("\"sequence\":1", "\"sequence\":1.0", 1),
        valid.replacen("\"sequence\":1", "\"sequence\":true", 1),
        valid.replacen("\"version\":1", "\"version\":2", 1),
    ] {
        assert!(decode_host_line(invalid.as_bytes()).is_err());
    }
}

#[test]
fn dependency_preserves_full_u64_identity_and_sequence() {
    let mut expected = snapshot(u64::MAX);
    expected.session.as_mut().unwrap().generation = u64::MAX;
    let frame = encode_host(&HostMessage::Snapshot(expected.clone())).unwrap();
    assert_eq!(
        decode_host_line(&frame[..frame.len() - 1]).unwrap(),
        HostMessage::Snapshot(expected)
    );
}

#[test]
fn linux_ready_contract_is_unsupported_without_invented_v1_fields() {
    // Pure in-memory serialization, NOT native readiness and NOT stdout emission.
    let message = RendererMessage::Ready(Ready {
        version: V1,
        capture_exclusion: CaptureExclusion::Unsupported,
        pointer_feedback: true,
    });
    let bytes = encode_renderer(&message).unwrap();
    assert_eq!(bytes, b"{\"type\":\"ready\",\"version\":1,\"capture_exclusion\":\"unsupported\",\"pointer_feedback\":true}\n");
    assert_eq!(
        decode_renderer_line(&bytes[..bytes.len() - 1]).unwrap(),
        message
    );
}
