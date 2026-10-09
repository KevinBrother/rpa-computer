use rpa_desktop_feedback::codec::{
    decode_host_line, decode_renderer_line, encode_host, NdjsonDecoder, MAX_LINE_BYTES,
};
use rpa_desktop_feedback::protocol::*;
use rpa_desktop_feedback::Diagnostic;

fn snapshot() -> Snapshot {
    Snapshot {
        version: V1,
        sequence: 1,
        session: Some(Session {
            id: "session-test".into(),
            generation: 1,
        }),
        phase: Phase::Idle,
        cleanup: Cleanup::NotNeeded,
        surface: Some(Surface {
            id: "macos:1".into(),
            version: "v1".into(),
            x: 0.,
            y: 0.,
            width: 1512.,
            height: 982.,
        }),
        pointer: None,
    }
}

#[test]
fn exact_wire_shape_roundtrips_and_optional_fields_are_required_nullable() {
    let message = HostMessage::Snapshot(snapshot());
    let frame = encode_host(&message).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&frame).unwrap();
    assert_eq!(json["type"], "snapshot");
    assert_eq!(json["version"], 1);
    assert_eq!(json["cleanup"], "not_needed");
    assert!(json["pointer"].is_null());
    assert_eq!(
        decode_host_line(&frame[..frame.len() - 1]).unwrap(),
        message
    );
    for missing in ["session", "surface", "pointer"] {
        let mut value = json.clone();
        value.as_object_mut().unwrap().remove(missing);
        assert!(decode_host_line(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

#[test]
fn versions_unknown_sensitive_fields_and_duplicate_fields_are_rejected() {
    for line in [
        r#"{"type":"heartbeat","version":2}"#,
        r#"{"type":"heartbeat","version":0}"#,
        r#"{"type":"heartbeat","version":1,"text":"secret"}"#,
        r#"{"type":"heartbeat","version":1,"version":1}"#,
        r#"{"type":"stop","version":1}"#,
        r#"{"type":"stop","version":1,"session":null}"#,
        r#"{"type":"stop","version":1,"session":{"id":"","generation":1}}"#,
        r#"{"type":"stop","version":1,"session":{"id":"a","generation":-1}}"#,
        r#"{"type":"stop","version":1,"session":{"id":"a","generation":1,"key":"secret"}}"#,
        r#"{"type":"ready","version":1,"capture_exclusion":"verified","pointer_feedback":true}"#,
    ] {
        assert!(decode_renderer_line(line.as_bytes()).is_err(), "{line}");
    }
    assert_eq!(
        decode_renderer_line(br#"{"type":"heartbeat","version":42}"#),
        Err(Diagnostic::UnsupportedVersion)
    );
    let json = serde_json::to_value(snapshot()).unwrap();
    for sensitive in ["text", "key", "credentials", "screenshot", "title"] {
        let mut bad = json.clone();
        bad["type"] = "snapshot".into();
        bad[sensitive] = "sentinel-secret".into();
        let error = decode_host_line(&serde_json::to_vec(&bad).unwrap()).unwrap_err();
        assert!(!error.to_string().contains("sentinel-secret"));
    }
}

#[test]
fn invalid_geometry_and_pointer_without_surface_fail_validation() {
    let mut s = snapshot();
    s.surface.as_mut().unwrap().width = 0.;
    assert_eq!(s.validate(), Err(Diagnostic::InvalidField));
    s.surface.as_mut().unwrap().width = f64::INFINITY;
    assert!(s.validate().is_err());
    s.surface = None;
    s.pointer = Some(Pointer {
        x: 1.,
        y: 2.,
        kind: PointerKind::Move,
    });
    assert!(s.validate().is_err());
    s.surface = snapshot().surface;
    s.pointer.as_mut().unwrap().x = f64::NAN;
    assert!(encode_host(&HostMessage::Snapshot(s)).is_err());
}

#[test]
fn incremental_decoder_handles_split_frames_and_bounds_unterminated_input() {
    let mut decoder = NdjsonDecoder::new(MAX_LINE_BYTES).unwrap();
    let mut lines = Vec::new();
    decoder
        .feed(b"abc", |line| {
            lines.push(line.to_vec());
            Ok(())
        })
        .unwrap();
    decoder
        .feed(b"\ndef\n", |line| {
            lines.push(line.to_vec());
            Ok(())
        })
        .unwrap();
    assert_eq!(lines, [b"abc".to_vec(), b"def".to_vec()]);
    assert_eq!(decoder.finish(), Ok(()));
    let mut decoder = NdjsonDecoder::new(8).unwrap();
    let oversized = vec![b'x'; 1_000_000];
    assert_eq!(
        decoder.feed(&oversized, |_| Ok(())),
        Err(Diagnostic::LineTooLong)
    );
    assert!(decoder.buffered_bytes() <= 8);
    assert!(decoder.buffer_capacity() <= 8);
    assert_eq!(
        decoder.feed(b"ok\n", |_| Ok(())),
        Err(Diagnostic::LineTooLong)
    );
}

#[test]
fn exact_limit_and_eof_are_unambiguous_and_callback_errors_are_terminal() {
    let mut decoder = NdjsonDecoder::new(4).unwrap();
    decoder.feed(b"abcd\n", |_| Ok(())).unwrap();
    decoder.feed(b"x", |_| Ok(())).unwrap();
    assert_eq!(decoder.finish(), Err(Diagnostic::TruncatedLine));
    let mut decoder = NdjsonDecoder::new(4).unwrap();
    assert_eq!(
        decoder.feed(b"a\nb\n", |_| Err(Diagnostic::InvalidJson)),
        Err(Diagnostic::InvalidJson)
    );
    assert_eq!(decoder.finish(), Err(Diagnostic::InvalidJson));
    assert!(NdjsonDecoder::new(MAX_LINE_BYTES + 1).is_err());
}

#[test]
fn only_locked_enum_values_and_bounded_tokens_are_accepted() {
    let mut s = snapshot();
    s.session.as_mut().unwrap().id = "x".repeat(129);
    assert!(s.validate().is_err());
    assert!(decode_renderer_line(
        br#"{"type":"error","version":1,"code":"window creation failed: user title"}"#
    )
    .is_err());
    let ready = decode_renderer_line(br#"{"type":"ready","version":1,"capture_exclusion":"unsupported","pointer_feedback":false}"#).unwrap();
    assert!(matches!(ready, RendererMessage::Ready(_)));
    for (field, value) in [("phase", "typing"), ("cleanup", "safe")] {
        let mut value_json = serde_json::to_value(HostMessage::Snapshot(snapshot())).unwrap();
        value_json[field] = value.into();
        assert!(decode_host_line(&serde_json::to_vec(&value_json).unwrap()).is_err());
    }
}

#[test]
fn numeric_overflow_duplicate_nested_keys_invalid_utf8_and_json_depth_fail_closed() {
    for line in [
        br#"{"type":"stop","version":1,"session":{"id":"a","generation":18446744073709551616}}"#
            .as_slice(),
        br#"{"type":"stop","version":1,"session":{"id":"a","id":"b","generation":1}}"#.as_slice(),
        br#"{"type":"stop","version":1,"session":{"id":"a","generation":1.5}}"#.as_slice(),
        br#"{"type":"heartbeat","type":"ready","version":1}"#.as_slice(),
        b"\xff\n".as_slice(),
    ] {
        assert!(decode_renderer_line(line).is_err());
    }
    let nested = format!("{}0{}", "[".repeat(200), "]".repeat(200));
    assert!(decode_renderer_line(nested.as_bytes()).is_err());
    let oversized = vec![b'x'; MAX_LINE_BYTES + 1];
    assert_eq!(
        decode_renderer_line(&oversized),
        Err(Diagnostic::LineTooLong)
    );
}

#[test]
fn all_renderer_message_variants_roundtrip_without_extensible_payloads() {
    use rpa_desktop_feedback::codec::encode_renderer;
    let messages = [
        RendererMessage::Ready(Ready {
            version: V1,
            capture_exclusion: CaptureExclusion::Unsupported,
            pointer_feedback: false,
        }),
        RendererMessage::Heartbeat(Heartbeat { version: V1 }),
        RendererMessage::Stop(Stop {
            version: V1,
            session: Session {
                id: "a".into(),
                generation: u64::MAX,
            },
        }),
        RendererMessage::Error(RendererError {
            version: V1,
            code: "platform_permission_denied".into(),
        }),
    ];
    for message in messages {
        let frame = encode_renderer(&message).unwrap();
        assert!(frame.len() <= rpa_desktop_feedback::codec::MAX_FRAME_BYTES);
        assert_eq!(
            decode_renderer_line(&frame[..frame.len() - 1]).unwrap(),
            message
        );
    }
}

#[test]
fn actual_host_geometry_versions_roundtrip_without_widening_session_identity() {
    // Exact shape from src/backend/capture.rs::geometry_version, not a dependency
    // on that module (this crate must remain independent of screenshot/input).
    for version in [
        "d1:o0,0:i1512x982:c3024x1964:r0",
        "d2:o-1920,-1080:i1920x1080:c1920x1080:r90",
    ] {
        let mut s = snapshot();
        s.surface.as_mut().unwrap().version = version.into();
        s.validate().unwrap();
        let message = HostMessage::Snapshot(s);
        let frame = encode_host(&message).unwrap();
        assert_eq!(
            decode_host_line(&frame[..frame.len() - 1]).unwrap(),
            message
        );
        // Commas belong to geometry versions, NOT to the identity allowlist.
        assert_eq!(
            Session {
                id: version.into(),
                generation: 1
            }
            .validate(),
            Err(Diagnostic::InvalidField)
        );
    }
    for bad_version in [
        "".to_owned(),
        "x".repeat(129),
        "v1\nprivate-title".into(),
        "版本1".into(),
        "user title".into(),
    ] {
        let mut s = snapshot();
        s.surface.as_mut().unwrap().version = bad_version;
        assert_eq!(s.validate(), Err(Diagnostic::InvalidField));
    }
}
