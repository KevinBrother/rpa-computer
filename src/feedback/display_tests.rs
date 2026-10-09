//! Memory-only adapter checks; no renderer/process/GUI/Win32 call.
use super::{FactBackend, FeedbackHandle};
use crate::backend::display::{
    budget,
    test_support::{facts, MemoryDesktop},
};
use crate::backend::{Backend, InputEvent};
use rpa_display_topology::Selection;
use serde_json::json;
use std::sync::{atomic::AtomicBool, Arc};

#[test]
fn wrapper_forwards_complete_facts_and_query_capture_errors() {
    let f = facts();
    let h = FeedbackHandle::new(Arc::new(|| {})).unwrap();
    let mut backend = FactBackend::new(Box::new(MemoryDesktop::new(f.clone())), h.clone());
    assert_eq!(backend.display_selections(), &["primary", "id", "desktop"]);
    let before = backend.display_snapshot().unwrap().unwrap();
    assert_eq!(before.displays().len(), 2);
    let g = backend
        .select_display(&Selection::Display("right".into()))
        .unwrap();
    assert_eq!(g.surface_id, "right");
    assert_eq!(
        backend
            .select_display(&Selection::Display("missing".into()))
            .unwrap_err()
            .code,
        "display_not_found"
    );
    h.grant("session", &g).unwrap();
    f.borrow_mut().capture_fail = true;
    assert!(backend
        .capture_display(budget::requested(400, 300).unwrap())
        .is_err());
    f.borrow_mut().query_fail = true;
    assert!(backend.display_snapshot().is_err());
    assert!(backend.geometry().is_err());
}
#[test]
fn desktop_surface_has_union_bounds_and_internal_gap_move_has_no_ring_target() {
    let f = facts();
    let h = FeedbackHandle::new(Arc::new(|| {})).unwrap();
    let mut backend = FactBackend::new(Box::new(MemoryDesktop::new(f)), h.clone());
    let g = backend.select_display(&Selection::Desktop).unwrap();
    h.grant("session", &g).unwrap();
    backend
        .capture_display(budget::requested(400, 300).unwrap())
        .unwrap()
        .unwrap();
    let snapshot = h.snapshot();
    let s = snapshot.surface.unwrap();
    assert_eq!(s.id, "desktop");
    assert_eq!((s.x, s.y, s.width, s.height), (-100.0, 0.0, 200.0, 150.0));
    assert!(s.version.contains("tracker"));
    backend.inject(&InputEvent::Move { x: -50, y: 50 }).unwrap();
    let pointer = h.snapshot().pointer.unwrap();
    assert_eq!((pointer.x, pointer.y), (-50.0, 50.0));
    backend.inject(&InputEvent::Move { x: 20, y: 60 }).unwrap();
    let pointer = h.snapshot().pointer.unwrap();
    assert_eq!(
        (pointer.x, pointer.y),
        (-50.0, 50.0),
        "gap transit must not publish a false selected-screen target"
    );
}
#[test]
fn full_desktop_stop_still_permanently_prevents_open_or_resume() {
    let f = facts();
    let h = FeedbackHandle::new(Arc::new(|| {})).unwrap();
    let mut rt = crate::runtime::Runtime::new_with_feedback(
        Box::new(MemoryDesktop::new(f.clone())),
        Arc::new(AtomicBool::new(false)),
        h.clone(),
    );
    let opened = rt.call("computer_open", json!({"display":{"kind":"desktop"}}));
    assert!(!opened.is_error, "{}", opened.data);
    let current = h.snapshot().session.unwrap();
    assert!(h.stop(&current));
    for (tool, args) in [
        ("computer_open", json!({"display":{"kind":"primary"}})),
        (
            "computer_resume",
            json!({"session_id":opened.data["session_id"]}),
        ),
    ] {
        let reply = rt.call(tool, args);
        assert!(reply.is_error);
        assert_eq!(reply.data["error"]["code"], "cancelled");
    }
    assert!(f.borrow().events.is_empty());
}
