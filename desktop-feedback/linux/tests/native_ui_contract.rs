//! Native lifecycle RED contract. No X connection or renderer process is opened.
//! Not #[should_panic]: missing behavior must fail the independent RED run.
mod common;
#[path = "../src/native_ui/mod.rs"]
mod native_ui;
mod native_ui_support;

use native_ui::*;
use native_ui_support::*;
use rpa_desktop_feedback::{protocol::*, Diagnostic};
use std::time::Duration;

#[test]
fn ready_follows_checked_creation_policy_paint_and_presentation() {
    let mut h = Harness::new();
    assert_eq!(h.runtime.start(ms(0)), Ok(()));
    assert_eq!(
        &*h.trace.borrow(),
        &[
            "facts",
            "create",
            "configure",
            "paint",
            "present_checked",
            "ready"
        ]
    );
    assert_eq!(
        h.messages(),
        vec![RendererMessage::Ready(Ready {
            version: V1,
            capture_exclusion: CaptureExclusion::Unsupported,
            pointer_feedback: true,
        })]
    );
    let scene = h.scene();
    assert!(!scene.stop_enabled);
    assert!(scene.ring_center.is_none());
    assert!(!scene.presentation.safe_completion);
}

#[test]
fn every_startup_failure_destroys_partial_resources_and_never_emits_ready() {
    for stage in ["facts", "create", "configure", "paint", "present_checked"] {
        let mut h = Harness::new();
        h.native.borrow_mut().fail_at = Some(stage);
        assert_eq!(h.runtime.start(ms(0)), Err(UiFailure::Native));
        assert!(h
            .messages()
            .iter()
            .all(|m| !matches!(m, RendererMessage::Ready(_))));
        assert_eq!(h.native.borrow().destroy_calls, 1, "stage={stage}");
        assert_eq!(
            h.runtime.step(ms(1), Event::Poll),
            Flow::Exited(Exit::Failure)
        );
    }
}

#[test]
fn duplicate_or_zero_window_ids_cannot_certify_ready() {
    for windows in [
        WindowSet {
            status: 0,
            stop: 12,
            ring: 13,
            epoch: 1,
        },
        WindowSet {
            status: 11,
            stop: 11,
            ring: 13,
            epoch: 1,
        },
    ] {
        let mut h = Harness::new();
        h.native.borrow_mut().windows = windows;
        assert!(h.runtime.start(ms(0)).is_err());
        assert!(!h
            .messages()
            .iter()
            .any(|m| matches!(m, RendererMessage::Ready(_))));
    }
}

#[test]
fn unchecked_or_incomplete_presentation_is_not_ready() {
    for receipt in [
        Receipt {
            server_checked: false,
            status_viewable: true,
            stop_viewable: true,
            ring_initialized: true,
            window_epoch: 1,
            topology_epoch: 1,
        },
        Receipt {
            server_checked: true,
            status_viewable: false,
            stop_viewable: true,
            ring_initialized: true,
            window_epoch: 1,
            topology_epoch: 1,
        },
        Receipt {
            server_checked: true,
            status_viewable: true,
            stop_viewable: false,
            ring_initialized: true,
            window_epoch: 1,
            topology_epoch: 1,
        },
        Receipt {
            server_checked: true,
            status_viewable: true,
            stop_viewable: true,
            ring_initialized: false,
            window_epoch: 1,
            topology_epoch: 1,
        },
        Receipt {
            server_checked: true,
            status_viewable: true,
            stop_viewable: true,
            ring_initialized: true,
            window_epoch: 99,
            topology_epoch: 1,
        },
        Receipt {
            server_checked: true,
            status_viewable: true,
            stop_viewable: true,
            ring_initialized: true,
            window_epoch: 1,
            topology_epoch: 99,
        },
    ] {
        let mut h = Harness::new();
        h.native.borrow_mut().receipt = receipt;
        assert!(h.runtime.start(ms(0)).is_err());
        assert!(!h
            .messages()
            .iter()
            .any(|m| matches!(m, RendererMessage::Ready(_))));
    }
}

#[test]
fn missing_input_shapes_fails_instead_of_intercepting_the_desktop() {
    let mut h = Harness::new();
    h.native.borrow_mut().facts.input_shapes = false;
    assert!(h.runtime.start(ms(0)).is_err());
    assert!(!h.trace.borrow().contains(&"ready"));
}

#[test]
fn ready_is_once_only_and_restart_is_rejected() {
    let mut h = Harness::started();
    assert_eq!(h.runtime.start(ms(1)), Err(UiFailure::AlreadyStarted));
    assert_eq!(
        h.messages()
            .iter()
            .filter(|m| matches!(m, RendererMessage::Ready(_)))
            .count(),
        1
    );
}

#[test]
fn heartbeat_is_generated_only_by_ui_turns_without_catchup_flood() {
    let mut h = Harness::started();
    h.runtime.step(ms(999), Event::Poll);
    assert_eq!(h.heartbeats(), 0);
    // Servicing the independent output worker cannot invent UI progress.
    h.drain_output();
    assert_eq!(h.heartbeats(), 0);
    h.runtime.step(ms(1000), Event::Poll);
    assert_eq!(h.heartbeats(), 1);
    h.runtime.step(ms(9000), Event::Poll);
    assert_eq!(h.heartbeats(), 2);
    h.runtime.step(ms(9000), Event::Poll);
    assert_eq!(h.heartbeats(), 2);
}

#[test]
fn stop_before_ready_or_without_session_is_ignored() {
    let mut h = Harness::new();
    h.click(1);
    assert!(h.stops().is_empty());
    h.runtime.start(ms(2)).unwrap();
    h.click(3);
    assert!(h.stops().is_empty());
}

#[test]
fn first_valid_click_emits_exact_session_once_and_disables_stop() {
    let mut h = Harness::started();
    let s = common::snapshot(1);
    h.apply(s.clone(), 10);
    h.click(11);
    h.click(13);
    assert_eq!(h.stops(), vec![s.session.unwrap()]);
    assert!(!h.scene().stop_enabled);
    assert!(!h.scene().presentation.safe_completion);
}

#[test]
fn press_cannot_be_rebound_to_a_new_session_or_generation() {
    for different_id in [false, true] {
        let mut h = Harness::started();
        h.apply(common::snapshot(1), 10);
        h.runtime.step(ms(11), press());
        let mut next = common::snapshot(2);
        if different_id {
            next.session.as_mut().unwrap().id = "session-b".into();
        } else {
            next.session.as_mut().unwrap().generation += 1;
        }
        h.apply(next.clone(), 12);
        h.runtime.step(ms(13), release());
        assert!(h.stops().is_empty());
        h.click(14);
        assert_eq!(h.stops(), vec![next.session.unwrap()]);
    }
}

#[test]
fn pending_snapshot_is_consumed_before_release_authorization() {
    let mut h = Harness::started();
    h.apply(common::snapshot(1), 10);
    h.runtime.step(ms(11), press());
    let mut next = common::snapshot(2);
    next.session.as_mut().unwrap().generation = u64::MAX;
    h.transport.borrow_mut().input = Input::Snapshot(next);
    h.runtime.step(ms(12), release());
    assert!(h.stops().is_empty());
}

#[test]
fn stale_snapshot_does_not_rearm_once_only_stop() {
    let mut h = Harness::started();
    h.apply(common::snapshot(10), 1);
    h.click(2);
    h.apply(common::snapshot(9), 4);
    h.click(5);
    assert_eq!(h.stops().len(), 1);
}

#[test]
fn wrong_window_epoch_nonprimary_and_release_outside_do_not_stop() {
    let mut h = Harness::started();
    h.apply(common::snapshot(1), 1);
    for (index, bad) in [
        Button {
            window: 11,
            epoch: 1,
            button: 1,
            x: 10,
            y: 10,
        },
        Button {
            window: 12,
            epoch: 0,
            button: 1,
            x: 10,
            y: 10,
        },
        Button {
            window: 12,
            epoch: 1,
            button: 3,
            x: 10,
            y: 10,
        },
        Button {
            window: 12,
            epoch: 1,
            button: 1,
            x: 88,
            y: 10,
        },
        Button {
            window: 12,
            epoch: 1,
            button: 1,
            x: 10,
            y: 48,
        },
    ]
    .into_iter()
    .enumerate()
    {
        h.runtime.step(ms(2 + index as u64 * 2), Event::Press(bad));
        h.runtime
            .step(ms(3 + index as u64 * 2), Event::Release(bad));
    }
    h.runtime.step(ms(20), press());
    h.runtime.step(
        ms(21),
        Event::Release(Button {
            x: -1,
            ..valid_button()
        }),
    );
    h.runtime.step(ms(22), release()); // a release without a fresh press cannot stop
    assert!(h.stops().is_empty());
}

#[test]
fn close_requests_stop_once_but_does_not_assert_host_cleanup() {
    let mut h = Harness::started();
    h.apply(common::snapshot(1), 1);
    for _ in 0..2 {
        h.runtime.step(
            ms(2),
            Event::Close {
                window: 11,
                epoch: 1,
            },
        );
    }
    assert_eq!(h.stops().len(), 1);
    assert!(!h.scene().presentation.safe_completion);
}

#[test]
fn clean_parent_eof_destroys_windows_without_stop_or_cleanup_claim() {
    let mut h = Harness::started();
    h.apply(common::snapshot(1), 1);
    h.transport.borrow_mut().input = Input::Eof;
    assert_eq!(
        h.runtime.step(ms(2), release()),
        Flow::Exited(Exit::ParentEof)
    );
    assert!(h.stops().is_empty());
    assert_eq!(h.native.borrow().destroy_calls, 1);
    let count = h.messages().len();
    h.runtime.step(ms(9000), Event::Poll);
    assert_eq!(h.messages().len(), count);
    assert_eq!(h.native.borrow().destroy_calls, 1);
    assert!(!h.scene().presentation.safe_completion);
}

#[test]
fn poisoned_input_and_output_transport_loss_are_terminal() {
    for error in [
        Diagnostic::LineTooLong,
        Diagnostic::InvalidJson,
        Diagnostic::TruncatedLine,
        Diagnostic::TransportLost,
    ] {
        let mut h = Harness::started();
        h.transport.borrow_mut().input = Input::Failed(error);
        assert_eq!(
            h.runtime.step(ms(1), Event::Poll),
            Flow::Exited(Exit::Failure)
        );
        assert_eq!(h.native.borrow().destroy_calls, 1);
        assert!(h.stops().is_empty());
    }
    let mut h = Harness::started();
    h.transport.borrow_mut().failed = Some(Diagnostic::TransportLost);
    assert_eq!(
        h.runtime.step(ms(1), Event::Poll),
        Flow::Exited(Exit::Failure)
    );
    assert_eq!(h.native.borrow().destroy_calls, 1);
}

#[test]
fn bounded_output_overflow_terminates_instead_of_dropping_stop() {
    let mut h = Harness::started();
    h.apply(common::snapshot(1), 1);
    // Use the real frozen Outbox; ready already occupies one of its 32 slots.
    for _ in 1..32 {
        h.transport
            .borrow_mut()
            .outbox
            .enqueue(RendererMessage::Heartbeat(Heartbeat { version: V1 }))
            .unwrap();
    }
    h.runtime.step(ms(2), press());
    assert_eq!(
        h.runtime.step(ms(3), release()),
        Flow::Exited(Exit::Failure)
    );
    assert_eq!(h.native.borrow().destroy_calls, 1);
    assert!(h.stops().is_empty()); // not silently counted as delivered
}

#[test]
fn cleanup_error_remains_visible_over_closed_phase() {
    let mut h = Harness::started();
    let mut s = common::snapshot(1);
    s.phase = Phase::Closed;
    s.cleanup = Cleanup::Failed;
    h.apply(s, 1);
    assert_eq!(
        h.scene().presentation.severity,
        rpa_desktop_feedback_linux::model::Severity::Error
    );
    assert!(!h.scene().presentation.safe_completion);
    assert!(!h.scene().stop_enabled);
}

#[test]
fn ui_scaling_does_not_scale_native_pointer_coordinates() {
    let mut h = Harness::new();
    h.native.borrow_mut().facts.scale = 1.5;
    h.runtime.start(ms(0)).unwrap();
    let mut s = common::snapshot(1);
    s.pointer = Some(Pointer {
        x: 123.,
        y: 234.,
        kind: PointerKind::Click,
    });
    h.apply(s, 1);
    let scene = h.scene();
    assert_eq!(
        (scene.layout.stop.width, scene.layout.stop.height),
        (132, 72)
    );
    assert_eq!(scene.ring_center, Some((123, 234)));
}

#[test]
fn topology_change_clears_ring_and_cancels_pressed_stop_before_new_layout() {
    let mut h = Harness::started();
    let mut s = common::snapshot(1);
    s.pointer = Some(Pointer {
        x: 100.,
        y: 100.,
        kind: PointerKind::Click,
    });
    h.apply(s, 1);
    h.runtime.step(ms(2), press());
    h.native.borrow_mut().facts.epoch = 2;
    h.native.borrow_mut().receipt.topology_epoch = 2;
    assert_eq!(h.runtime.step(ms(3), Event::TopologyChanged), Flow::Running);
    assert!(h.scene().ring_center.is_none());
    h.runtime.step(ms(4), release());
    assert!(h.stops().is_empty());
}

#[test]
fn undersized_monitor_fails_without_shrinking_stop() {
    let mut h = Harness::new();
    h.native.borrow_mut().facts.monitors[0].workarea.width = 87;
    assert!(h.runtime.start(ms(0)).is_err());
    assert!(!h.trace.borrow().contains(&"ready"));
}

#[test]
fn expired_ring_is_removed_by_ui_tick_without_capture_signal() {
    let mut h = Harness::started();
    let mut s = common::snapshot(1);
    s.pointer = Some(Pointer {
        x: 100.,
        y: 100.,
        kind: PointerKind::Click,
    });
    h.apply(s, 10);
    h.runtime.step(ms(659), Event::Poll);
    assert_eq!(h.scene().ring_center, Some((100, 100)));
    h.runtime.step(ms(660), Event::Poll);
    assert!(h.scene().ring_center.is_none());
}

#[test]
fn native_disconnect_and_clock_regression_fail_closed() {
    let mut h = Harness::started();
    assert_eq!(
        h.runtime.step(ms(1), Event::NativeLost),
        Flow::Exited(Exit::Failure)
    );
    assert_eq!(h.native.borrow().destroy_calls, 1);
    let mut h = Harness::started();
    h.runtime.step(Duration::from_secs(2), Event::Poll);
    assert_eq!(
        h.runtime.step(ms(1), Event::Poll),
        Flow::Exited(Exit::Failure)
    );
}

#[test]
fn ready_enqueue_failure_destroys_ui_instead_of_running_unannounced() {
    let mut h = Harness::new();
    h.transport.borrow_mut().failed = Some(Diagnostic::TransportLost);
    assert!(h.runtime.start(ms(0)).is_err());
    assert_eq!(h.native.borrow().destroy_calls, 1);
    assert!(h.messages().is_empty());
}

#[test]
fn stale_close_event_cannot_stop_a_current_window_set() {
    let mut h = Harness::started();
    h.apply(common::snapshot(1), 1);
    h.runtime.step(
        ms(2),
        Event::Close {
            window: 11,
            epoch: 0,
        },
    );
    h.runtime.step(
        ms(3),
        Event::Close {
            window: 999,
            epoch: 1,
        },
    );
    assert!(h.stops().is_empty());
}

#[test]
fn post_ready_native_paint_failure_is_terminal() {
    let mut h = Harness::started();
    h.native.borrow_mut().fail_at = Some("paint");
    h.transport.borrow_mut().input = Input::Snapshot(common::snapshot(1));
    assert_eq!(
        h.runtime.step(ms(1), Event::Poll),
        Flow::Exited(Exit::Failure)
    );
    assert_eq!(h.native.borrow().destroy_calls, 1);
}

#[test]
fn hover_pressed_and_disabled_visuals_do_not_change_stop_identity() {
    let mut h = Harness::started();
    assert_eq!(h.scene().stop_visual, ButtonVisual::Disabled);
    h.apply(common::snapshot(1), 1);
    assert_eq!(h.scene().stop_visual, ButtonVisual::Normal);
    h.runtime.step(
        ms(2),
        Event::Hover {
            window: 12,
            epoch: 1,
            inside: true,
        },
    );
    assert_eq!(h.scene().stop_visual, ButtonVisual::Hover);
    assert!(h.stops().is_empty());
    h.runtime.step(ms(3), press());
    assert_eq!(h.scene().stop_visual, ButtonVisual::Pressed);
    h.runtime.step(ms(4), release());
    assert_eq!(h.scene().stop_visual, ButtonVisual::Disabled);
    assert_eq!(h.stops(), vec![common::snapshot(1).session.unwrap()]);
    assert_eq!(h.scene().style.label, "AI Control");
}
