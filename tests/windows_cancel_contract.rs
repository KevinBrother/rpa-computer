//! Windows-only, pure Session-0-safe contracts. No native input, sockets or GUI.
#![cfg(target_os = "windows")]
#[path = "windows_cancel_contract/support.rs"]
mod support;
use rpa_computer::backend::{Direction, InputEvent};
use rpa_computer::mcp::{
    server::{Action, McpService},
    worker::{CancelVerdict, ShutdownStatus},
};
use serde_json::json;
use support::*;

#[test]
fn cancel01_drag_pressed_then_pause() {
    let mut h = Harness::new(Point::ButtonPress, false);
    let action = h.start(drag());
    let prefix = h.at_barrier();
    assert_eq!(prefix.len(), 2);
    assert!(matches!(
        &prefix[1],
        InputEvent::Button {
            direction: Direction::Press,
            ..
        }
    ));
    assert_eq!(h.trace.lock().unwrap().held.len(), 1);
    let pause = h.control("computer_pause");
    h.await_cancel();
    h.release.now();
    partial(&action.finish(), 2, "released");
    let reply = pause.finish();
    assert!(!reply.is_error);
    assert_eq!(reply.data["state"], "paused");
    assert_eq!(reply.data["cleanup_outcome"], "released");
    h.assert_tail(&prefix, false);
}

#[test]
fn cancel02_drag_moved_then_close() {
    let mut h = Harness::new(Point::DragMove, false);
    let action = h.start(drag());
    let prefix = h.at_barrier();
    assert_eq!(prefix.len(), 3);
    assert!(matches!(&prefix[2], InputEvent::Move { x: 8, y: 8 }));
    let close = h.control("computer_close");
    h.await_cancel();
    h.release.now();
    partial(&action.finish(), 3, "released");
    let reply = close.finish();
    assert!(!reply.is_error);
    assert_eq!(reply.data["state"], "closed");
    h.assert_tail(&prefix, false);
}

#[test]
fn cancel03_key_hold_pressed_then_pause() {
    let mut h = Harness::new(Point::KeyPress, false);
    let action = h.start(hold());
    let prefix = h.at_barrier();
    assert_eq!(
        prefix,
        vec![InputEvent::Key {
            key: "shift".into(),
            direction: Direction::Press
        }]
    );
    let pause = h.control("computer_pause");
    h.await_cancel();
    h.release.now();
    partial(&action.finish(), 1, "released");
    let reply = pause.finish();
    assert!(!reply.is_error);
    assert_eq!(reply.data["state"], "paused");
    h.assert_tail(&prefix, false);
}

#[test]
fn cancel04_chord_modifier_ingress_cancel_with_negative_ids() {
    let mut h = Harness::new(Point::KeyPress, false);
    let action = h.start(json!({"kind":"key_chord","modifiers":["ctrl","shift"],"key":"x"}));
    let prefix = h.at_barrier();
    assert_eq!(
        prefix,
        vec![InputEvent::Key {
            key: "ctrl".into(),
            direction: Direction::Press
        }]
    );
    let mut service = McpService::new(&h.worker, "cancel-contract".into());
    let epoch = h.worker.request_generation().current();
    for params in [
        json!({"requestId":999}),
        json!({"requestId":"101"}),
        json!({}),
        json!({"requestId":null}),
    ] {
        service.handle_line(
            &json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":params})
                .to_string(),
        );
        assert!(!h.worker.cancel_handle().is_cancelled());
        assert_eq!(h.worker.request_generation().current(), epoch);
        assert_eq!(h.trace.lock().unwrap().events, prefix);
    }
    service.handle_line(
        &json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":101}})
            .to_string(),
    );
    assert!(h.worker.cancel_handle().is_cancelled());
    h.release.now();
    partial(&action.finish(), 1, "released");
    h.assert_tail(&prefix, false);
    let generation = h.worker.request_generation().current();
    assert_eq!(
        h.worker
            .transport_gate()
            .cancel_request(&json!(101), &h.worker.cancel_handle()),
        CancelVerdict::NoMatch
    );
    assert_eq!(
        h.worker.request_generation().current(),
        generation,
        "late cancel must not bump stop epoch"
    );
}

#[test]
fn cancel05_text_first_scalar_then_cancel() {
    let mut h = Harness::new(Point::Text, false);
    let action = h.start(json!({"kind":"text_input","text":"汉abc第二段".repeat(128)}));
    let prefix = h.at_barrier();
    assert_eq!(prefix, vec![InputEvent::Text { text: "汉".into() }]);
    h.worker.cancel_handle().cancel();
    h.release.now();
    partial(&action.finish(), 1, "released");
    h.assert_tail(&prefix, false);
    assert_eq!(h.trace.lock().unwrap().cleanups, 1);
}

#[test]
fn cancel06_service_eof_during_action_then_production_shutdown() {
    let mut h = Harness::new(Point::ButtonPress, false);
    let action = h.start(drag());
    let prefix = h.at_barrier();
    let mut service = McpService::new(&h.worker, "cancel-contract".into());
    let (frames, directive) = service.handle_eof();
    assert!(frames.is_empty());
    assert!(matches!(directive, Action::Stop));
    assert!(h.worker.cancel_handle().is_cancelled());
    h.release.now();
    partial(&action.finish(), 2, "released");
    // handle_eof returns Stop; the transport owner must perform Worker shutdown.
    // Exercise that production continuation explicitly, not a fabricated EOF flag.
    assert_eq!(h.worker.shutdown(), ShutdownStatus::Clean);
    h.assert_tail(&prefix, false);
    assert_eq!(h.trace.lock().unwrap().cleanups, 2);
    assert_eq!(h.worker.shutdown(), ShutdownStatus::Clean);
    assert_eq!(h.trace.lock().unwrap().cleanups, 2);
}

#[test]
fn cancel08_failed_release_faults_session_and_quarantines_shutdown() {
    let mut h = Harness::new(Point::ButtonPress, true);
    let action = h.start(drag());
    let prefix = h.at_barrier();
    h.worker.cancel_handle().cancel();
    h.release.now();
    partial(&action.finish(), 2, "failed");
    h.assert_tail(&prefix, true);
    assert_eq!(h.trace.lock().unwrap().cleanups, 1);
    let blocked = call(&h.worker, "computer_step", h.args("must-not-run", hold()));
    assert_eq!(code(&blocked), "session_state");
    assert_eq!(h.trace.lock().unwrap().events, prefix);
    let close = call(&h.worker, "computer_close", json!({"session_id":h.sid}));
    assert!(close.is_error);
    assert_eq!(close.data["state"], "faulted");
    assert_eq!(close.data["cleanup_outcome"], "failed");
    assert_eq!(code(&close), "input_error");
    assert_eq!(h.trace.lock().unwrap().cleanups, 2);
    assert_eq!(h.worker.shutdown(), ShutdownStatus::Unknown);
    assert_eq!(h.trace.lock().unwrap().cleanups, 3);
    let refused = call(&h.worker, "computer_step", h.args("quarantined", hold()));
    assert_eq!(code(&refused), "worker_faulted");
    assert_eq!(h.worker.shutdown(), ShutdownStatus::Unknown);
    assert_eq!(h.trace.lock().unwrap().cleanups, 3);
    h.assert_tail(&prefix, true);
}

#[test]
fn cancel09_repeated_close_no_duplicate_cleanup() {
    let mut h = Harness::new(Point::DragMove, false);
    let action = h.start(drag());
    let prefix = h.at_barrier();
    let close = h.control("computer_close");
    h.await_cancel();
    h.release.now();
    partial(&action.finish(), 3, "released");
    let first = close.finish();
    assert!(!first.is_error);
    assert_eq!(first.data["state"], "closed");
    assert_eq!(first.data["cleanup_outcome"], "released");
    assert_eq!(h.trace.lock().unwrap().cleanups, 2);
    for _ in 0..3 {
        let repeat = call(&h.worker, "computer_close", json!({"session_id":h.sid}));
        assert!(!repeat.is_error);
        assert_eq!(repeat.data["state"], "closed");
        assert_eq!(repeat.data["cleanup_outcome"], "not_needed");
        assert_eq!(h.trace.lock().unwrap().cleanups, 2);
    }
    h.assert_tail(&prefix, false);
}

#[test]
fn cancel10_resume_requires_fresh_observation_without_replay() {
    let mut h = Harness::new(Point::KeyPress, false);
    let action = h.start(hold());
    let prefix = h.at_barrier();
    let pause = h.control("computer_pause");
    h.await_cancel();
    h.release.now();
    partial(&action.finish(), 1, "released");
    assert!(!pause.finish().is_error);
    h.assert_tail(&prefix, false);
    let stopped_events = h.trace.lock().unwrap().events.clone();
    let resume = call(&h.worker, "computer_resume", json!({"session_id":h.sid}));
    assert!(!resume.is_error);
    assert_eq!(resume.data["state"], "ready");
    assert_eq!(resume.data["requires_fresh_observation"], true);
    assert_eq!(h.trace.lock().unwrap().events, stopped_events);
    let old_replay = call(&h.worker, "computer_step", h.args("partial", hold()));
    partial(&old_replay, 1, "released");
    assert_eq!(
        h.trace.lock().unwrap().events,
        stopped_events,
        "old cancelled action must only replay its ledger result"
    );
    let stale = call(
        &h.worker,
        "computer_step",
        h.args("old-base", json!({"kind":"text_input","text":"old"})),
    );
    assert_eq!(code(&stale), "stale_observation");
    assert_eq!(stale.data["input_outcome"], "not_started");
    assert_eq!(h.trace.lock().unwrap().events, stopped_events);
    let obs = call(&h.worker, "computer_observe", json!({"session_id":h.sid}));
    assert!(!obs.is_error);
    let fresh = obs.data["observation_id"].as_str().unwrap().to_string();
    assert_ne!(fresh, h.base);
    h.base = fresh;
    let args = h.args("fresh", json!({"kind":"text_input","text":"z"}));
    let recovered = call(&h.worker, "computer_step", args.clone());
    assert!(!recovered.is_error);
    assert_eq!(recovered.data["input_outcome"], "dispatched");
    let events = h.trace.lock().unwrap().events.clone();
    assert_eq!(&events[..stopped_events.len()], &stopped_events);
    assert_eq!(
        &events[stopped_events.len()..],
        &[InputEvent::Text { text: "z".into() }]
    );
    let replay = call(&h.worker, "computer_step", args);
    assert!(!replay.is_error);
    assert_eq!(replay.data["input_outcome"], "dispatched");
    assert_eq!(
        h.trace.lock().unwrap().events,
        events,
        "ledger replay must never inject again"
    );
}
