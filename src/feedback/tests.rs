//! Source-only tests: execution belongs to CC + GLM.
use super::*;
use crate::backend::{Backend, BackendError, Capture, Direction, Geometry, InputEvent};
use rpa_desktop_feedback::protocol::{Cleanup, Phase, PointerKind, Session};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub(super) fn geometry() -> Geometry {
    Geometry {
        surface_id: "windows:1".into(),
        version: "d1:o0,0:i800x600:c800x600:r0".into(),
        input_origin: (0, 0),
        input_size: (800, 600),
    }
}
pub(super) fn control() -> (FeedbackHandle, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    (
        FeedbackHandle::new(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }))
        .unwrap(),
        count,
    )
}
#[test]
fn off_does_not_resolve_missing_renderer_or_start_worker() {
    let (h, _) = control();
    assert!(FeedbackHost::start(&FeedbackConfig::default(), h)
        .unwrap()
        .is_none());
}
#[test]
fn stale_stop_is_rejected_and_terminal_stop_cannot_be_reopened() {
    let (h, count) = control();
    let a = h.grant("session-a", &geometry()).unwrap();
    let old = Session {
        generation: a.generation - 1,
        ..a.clone()
    };
    assert!(!h.stop(&old));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert!(h.stop(&a));
    assert!(!h.stop(&a));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(h.is_terminated());
    assert!(h.grant("session-b", &geometry()).is_err());
    assert_eq!(h.snapshot().phase, Phase::Stopping);
    h.complete_terminal(Cleanup::Failed);
    assert_eq!(h.snapshot().phase, Phase::Faulted);
    assert!(!h.snapshot().safe_completion());
}
#[test]
fn old_queue_stop_cannot_cancel_new_session_and_unknown_stays_faulted() {
    let (h, _) = control();
    let old = h.grant("session-a", &geometry()).unwrap();
    h.retire(Cleanup::Released);
    let current = h.grant("session-b", &geometry()).unwrap();
    assert!(current.generation > old.generation);
    assert!(!h.stop(&old));
    assert_eq!(h.snapshot().session, Some(current));
    h.terminate();
    h.complete_terminal(Cleanup::Unknown);
    h.complete_terminal(Cleanup::Released);
    assert_eq!(h.snapshot().cleanup, Cleanup::Unknown);
}
struct FakeBackend {
    fail_input: bool,
    fail_release: bool,
}
impl Backend for FakeBackend {
    fn platform(&self) -> &'static str {
        "windows"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        Ok(geometry())
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        Ok(Capture {
            png: vec![],
            width: 800,
            height: 600,
            geometry: geometry(),
        })
    }
    fn inject(&mut self, _: &InputEvent) -> Result<(), BackendError> {
        if self.fail_input {
            Err(BackendError::new("input_failed", "private-secret"))
        } else {
            Ok(())
        }
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        if self.fail_release {
            Err(BackendError::new("input_failed", "private-secret"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn actual_callbacks_only_confirm_successful_pointer_and_never_serialize_text() {
    let (h, _) = control();
    h.grant("session-a", &geometry()).unwrap();
    let mut b = FactBackend::new(
        Box::new(FakeBackend {
            fail_input: false,
            fail_release: false,
        }),
        h.clone(),
    );
    b.capture().unwrap();
    assert_eq!(h.snapshot().phase, Phase::Observing);
    b.inject(&InputEvent::Move { x: 42, y: 51 }).unwrap();
    b.inject(&InputEvent::Button {
        button: "left".into(),
        direction: Direction::Press,
        click_count: 1,
    })
    .unwrap();
    assert_eq!(h.snapshot().pointer.unwrap().kind, PointerKind::Click);
    b.inject(&InputEvent::Text {
        text: "private-secret".into(),
    })
    .unwrap();
    let encoded = rpa_desktop_feedback::codec::encode_host(
        &rpa_desktop_feedback::protocol::HostMessage::Snapshot(h.snapshot()),
    )
    .unwrap();
    assert!(!String::from_utf8(encoded)
        .unwrap()
        .contains("private-secret"));
    h.terminate();
    assert!(b.inject(&InputEvent::Move { x: 100, y: 100 }).is_err());
    assert!(h.snapshot().pointer.is_none());
}
#[test]
fn failed_dispatch_does_not_confirm_pointer() {
    let (h, _) = control();
    h.grant("session-a", &geometry()).unwrap();
    let mut b = FactBackend::new(
        Box::new(FakeBackend {
            fail_input: true,
            fail_release: true,
        }),
        h.clone(),
    );
    assert!(b.inject(&InputEvent::Move { x: 1, y: 2 }).is_err());
    assert!(h.snapshot().pointer.is_none());
    assert!(b.release_all().is_err());
    assert_eq!(h.snapshot().cleanup, Cleanup::Failed);
}
#[test]
fn unsupported_is_rejected_and_requested_does_not_claim_verified_capture() {
    use rpa_desktop_feedback::protocol::{CaptureExclusion, Ready, V1};
    let unsupported = Ready {
        version: V1,
        capture_exclusion: CaptureExclusion::Unsupported,
        pointer_feedback: true,
    };
    assert!(super::host::validate_ready(&unsupported, false).is_err());
    let requested = Ready {
        capture_exclusion: CaptureExclusion::Requested,
        ..unsupported
    };
    assert!(super::host::validate_ready(&requested, false).is_ok());
    assert!(super::host::validate_ready(&requested, true).is_err());
}
