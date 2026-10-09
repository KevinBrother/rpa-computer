//! Pure root-provider -> Geometry -> feedback authority/codec regression.
//! No native provider, renderer, process, socket, or desktop operations.
use super::*;
use crate::backend::display::{
    budget,
    test_support::{facts, MemoryFrames},
    DisplayProvider,
};
use crate::feedback::{FeedbackError, FeedbackHandle};
use rpa_desktop_feedback::{
    codec::{decode_host_line, encode_host},
    protocol::HostMessage,
    Diagnostic,
};
use std::sync::Arc;

#[test]
fn root_geometry_full_generation_survives_feedback_codec() {
    for selection in [
        Selection::Primary,
        Selection::Display("right".into()),
        Selection::Desktop,
    ] {
        let mut provider = DisplayProvider::new(MemoryFrames(facts())).unwrap();
        let geometry = provider.select(&selection).unwrap();
        let captured = provider
            .capture(budget::requested(400, 300).unwrap())
            .unwrap();
        let full = captured.topology.generation();
        assert_eq!(geometry, captured.geometry);
        assert_eq!(geometry.version, full.to_token());
        assert_eq!(
            topology(&captured.topology)["topology_generation"],
            geometry.version
        );
        assert_eq!(
            observation(&captured.mapping)["topology_generation"],
            geometry.version
        );

        let handle = FeedbackHandle::new(Arc::new(|| {})).unwrap();
        handle.grant("token-repair-session", &geometry).unwrap();
        let snapshot = handle.snapshot();
        let surface = snapshot.surface.as_ref().unwrap();
        assert_eq!(surface.version, geometry.version);
        assert_eq!(surface.id, geometry.surface_id);
        // Exercise the frozen production validators AND full NDJSON codec,
        // rather than reproducing their character rules in a root helper.
        let message = HostMessage::Snapshot(snapshot);
        let encoded = encode_host(&message).unwrap();
        assert_eq!(encoded.last(), Some(&b'\n'));
        assert_eq!(
            decode_host_line(&encoded[..encoded.len() - 1]).unwrap(),
            message
        );
    }
}

#[test]
fn feedback_codec_still_rejects_legacy_debug_version() {
    let mut provider = DisplayProvider::new(MemoryFrames(facts())).unwrap();
    let mut geometry = provider.select(&Selection::Desktop).unwrap();
    let handle = FeedbackHandle::new(Arc::new(|| {})).unwrap();
    handle.grant("token-repair-session", &geometry).unwrap();
    let mut snapshot = handle.snapshot();
    // Pin the known invalid old wire value; never sanitize it or broaden rules.
    geometry.version = "topology:Generation { tracker: 1, revision: 1 }".into();
    assert!(matches!(
        handle.grant("invalid-version-session", &geometry),
        Err(FeedbackError::Protocol(Diagnostic::InvalidField))
    ));
    snapshot.surface.as_mut().unwrap().version = geometry.version;
    let invalid = HostMessage::Snapshot(snapshot);
    assert_eq!(encode_host(&invalid), Err(Diagnostic::InvalidField));
    let raw = serde_json::to_vec(&invalid).unwrap();
    assert_eq!(decode_host_line(&raw), Err(Diagnostic::InvalidField));
}
