#![allow(dead_code)]
use rpa_desktop_feedback::{
    codec::encode_host,
    protocol::{Cleanup, HostMessage, Phase, Session, Snapshot, Surface, V1},
};

pub fn snapshot(sequence: u64) -> Snapshot {
    Snapshot {
        version: V1,
        sequence,
        session: Some(Session {
            id: "session-a".into(),
            generation: 1,
        }),
        phase: Phase::Idle,
        cleanup: Cleanup::NotNeeded,
        surface: Some(Surface {
            id: "test-monitor-a".into(),
            version: "d1:o0,0:i1920x1080:c1920x1080:r0".into(),
            x: 0.,
            y: 0.,
            width: 1920.,
            height: 1080.,
        }),
        pointer: None,
    }
}

pub fn frame(sequence: u64) -> Vec<u8> {
    encode_host(&HostMessage::Snapshot(snapshot(sequence))).unwrap()
}
