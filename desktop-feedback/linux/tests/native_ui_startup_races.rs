//! Additional GREEN-candidate guards; not part of the unchanged observed RED28.
//! No worker, subprocess or native X connection is started by these tests.
mod common;
#[path = "../src/native_ui/mod.rs"]
mod native_ui;
use native_ui::{process_boundary::*, *};
use rpa_desktop_feedback::{codec::encode_host, protocol::*, Diagnostic};
use rpa_desktop_feedback_linux::geometry::{Monitor, Rect};
use std::{
    cell::RefCell,
    io::{self, Cursor, Write},
    rc::Rc,
    time::Duration,
};

#[derive(Default)]
struct Observations {
    calls: Vec<&'static str>,
    scene: Option<Scene>,
}
struct NativeProbe {
    bridge: Rc<RefCell<PipeBridge>>,
    seen: Rc<RefCell<Observations>>,
    action: Option<fn(&mut PipeBridge)>,
    fail_facts: bool,
}
impl Native for NativeProbe {
    fn facts(&mut self) -> Result<DisplayFacts, UiFailure> {
        self.seen.borrow_mut().calls.push("facts");
        if let Some(action) = self.action.take() {
            action(&mut self.bridge.borrow_mut());
        }
        if self.fail_facts {
            return Err(UiFailure::Native);
        }
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        Ok(DisplayFacts {
            monitors: vec![Monitor {
                key: "a".into(),
                bounds,
                workarea: bounds,
            }],
            scale: 1.,
            epoch: 1,
            input_shapes: true,
        })
    }
    fn create(&mut self) -> Result<WindowSet, UiFailure> {
        self.seen.borrow_mut().calls.push("create");
        Ok(WindowSet {
            status: 1,
            stop: 2,
            ring: 3,
            epoch: 1,
        })
    }
    fn configure(&mut self, _: WindowSet, _: &[WindowPolicy; 3]) -> Result<(), UiFailure> {
        Ok(())
    }
    fn paint(&mut self, _: WindowSet, scene: &Scene) -> Result<(), UiFailure> {
        self.seen.borrow_mut().scene = Some(scene.clone());
        Ok(())
    }
    fn present_checked(&mut self, _: WindowSet, _: &Scene) -> Result<Receipt, UiFailure> {
        Ok(Receipt {
            server_checked: true,
            status_viewable: true,
            stop_viewable: true,
            ring_initialized: true,
            window_epoch: 1,
            topology_epoch: 1,
        })
    }
    fn destroy_all(&mut self) {
        self.seen.borrow_mut().calls.push("destroy");
    }
}
struct Parent {
    bridge: Rc<RefCell<PipeBridge>>,
    eof_at_ready: bool,
}
impl Transport for Parent {
    fn failure(&self) -> Option<Diagnostic> {
        self.bridge.borrow().failure()
    }
    fn take_input(&mut self) -> Input {
        self.bridge.borrow_mut().take_input()
    }
    fn enqueue(&mut self, message: RendererMessage) -> Result<(), Diagnostic> {
        self.bridge.borrow_mut().enqueue(message)
    }
    fn enqueue_ready(&mut self, ready: Ready) -> Result<ReadyEnqueue, Diagnostic> {
        let mut bridge = self.bridge.borrow_mut();
        if self.eof_at_ready {
            bridge.read_once(&mut io::empty()).unwrap();
        }
        bridge.enqueue_ready(ready)
    }
    fn finish(&mut self) {
        self.bridge.borrow_mut().finish();
    }
}
fn ready() -> Ready {
    Ready {
        version: V1,
        capture_exclusion: CaptureExclusion::Unsupported,
        pointer_feedback: true,
    }
}
fn feed(bridge: &mut PipeBridge, seq: u64) {
    let mut snapshot = common::snapshot(seq);
    snapshot.session.as_mut().unwrap().generation = seq;
    bridge
        .read_once(&mut Cursor::new(
            encode_host(&HostMessage::Snapshot(snapshot)).unwrap(),
        ))
        .unwrap();
}
fn setup() -> (Rc<RefCell<PipeBridge>>, Rc<RefCell<Observations>>) {
    (
        Rc::new(RefCell::new(PipeBridge::new())),
        Rc::new(RefCell::new(Observations::default())),
    )
}

#[test]
fn startup_keeps_newest_snapshot_from_before_connect_and_during_facts() {
    let (bridge, seen) = setup();
    feed(&mut bridge.borrow_mut(), 1);
    let native = NativeProbe {
        bridge: bridge.clone(),
        seen: seen.clone(),
        action: Some(|b| feed(b, 2)),
        fail_facts: false,
    };
    let result = start_renderer(
        Parent {
            bridge,
            eof_at_ready: false,
        },
        parse_style(&[]).unwrap(),
        || Ok(native),
        || Duration::ZERO,
        || panic!("no failure drain"),
    );
    let Ok(Startup::Running(mut runtime)) = result else {
        panic!("must start")
    };
    assert_eq!(
        runtime.step(Duration::from_millis(1), Event::Poll),
        Flow::Running
    );
    assert_eq!(
        seen.borrow()
            .scene
            .as_ref()
            .unwrap()
            .control_session
            .as_ref()
            .unwrap()
            .generation,
        2
    );
}

#[test]
fn newer_live_snapshot_supersedes_stashed_startup_snapshot_on_first_turn() {
    let (bridge, seen) = setup();
    feed(&mut bridge.borrow_mut(), 1);
    let native = NativeProbe {
        bridge: bridge.clone(),
        seen: seen.clone(),
        action: None,
        fail_facts: false,
    };
    let result = start_renderer(
        Parent {
            bridge: bridge.clone(),
            eof_at_ready: false,
        },
        parse_style(&[]).unwrap(),
        || Ok(native),
        || Duration::ZERO,
        || panic!("no failure drain"),
    );
    let Ok(Startup::Running(mut runtime)) = result else {
        panic!("must start")
    };
    feed(&mut bridge.borrow_mut(), 3);
    assert_eq!(
        runtime.step(Duration::from_millis(1), Event::Poll),
        Flow::Running
    );
    assert_eq!(
        seen.borrow()
            .scene
            .as_ref()
            .unwrap()
            .control_session
            .as_ref()
            .unwrap()
            .generation,
        3
    );
}

#[test]
fn eof_supersedes_stashed_snapshot_on_first_turn_without_repaint_or_control() {
    let (bridge, seen) = setup();
    feed(&mut bridge.borrow_mut(), 1);
    let native = NativeProbe {
        bridge: bridge.clone(),
        seen: seen.clone(),
        action: None,
        fail_facts: false,
    };
    let result = start_renderer(
        Parent {
            bridge: bridge.clone(),
            eof_at_ready: false,
        },
        parse_style(&[]).unwrap(),
        || Ok(native),
        || Duration::ZERO,
        || panic!("no failure drain"),
    );
    let Ok(Startup::Running(mut runtime)) = result else {
        panic!("must start")
    };
    bridge.borrow_mut().read_once(&mut io::empty()).unwrap();
    assert_eq!(
        runtime.step(Duration::from_millis(1), Event::Poll),
        Flow::Exited(Exit::ParentEof)
    );
    assert!(seen
        .borrow()
        .scene
        .as_ref()
        .unwrap()
        .control_session
        .is_none());
    assert_eq!(bridge.borrow().buffered_frames(), 1); // only already accepted Ready
}

#[test]
fn eof_racing_ready_enqueue_is_clean_without_a_ready_frame() {
    let (bridge, seen) = setup();
    let native = NativeProbe {
        bridge: bridge.clone(),
        seen: seen.clone(),
        action: None,
        fail_facts: false,
    };
    let result = start_renderer(
        Parent {
            bridge: bridge.clone(),
            eof_at_ready: true,
        },
        parse_style(&[]).unwrap(),
        || Ok(native),
        || Duration::ZERO,
        || panic!("no failure drain"),
    );
    assert!(matches!(result, Ok(Startup::Closed(Exit::ParentEof))));
    assert_eq!(bridge.borrow().buffered_frames(), 0);
    assert_eq!(
        seen.borrow()
            .calls
            .iter()
            .filter(|c| **c == "destroy")
            .count(),
        1
    );
}

#[test]
fn eof_during_failed_native_call_cancels_without_later_native_calls() {
    let (bridge, seen) = setup();
    let native = NativeProbe {
        bridge: bridge.clone(),
        seen: seen.clone(),
        action: Some(|b| b.read_once(&mut io::empty()).unwrap()),
        fail_facts: true,
    };
    let result = start_renderer(
        Parent {
            bridge: bridge.clone(),
            eof_at_ready: false,
        },
        parse_style(&[]).unwrap(),
        || Ok(native),
        || Duration::ZERO,
        || panic!("no failure drain"),
    );
    assert!(matches!(result, Ok(Startup::Closed(Exit::ParentEof))));
    assert_eq!(seen.borrow().calls, ["facts", "destroy"]);
    assert_eq!(bridge.borrow().buffered_frames(), 0);
}

#[test]
fn clean_eof_during_failed_connect_is_not_a_renderer_error() {
    let (bridge, _) = setup();
    let result = start_renderer::<NativeProbe, _>(
        Parent {
            bridge: bridge.clone(),
            eof_at_ready: false,
        },
        parse_style(&[]).unwrap(),
        || {
            bridge.borrow_mut().read_once(&mut io::empty()).unwrap();
            Err(UiFailure::Native)
        },
        || Duration::ZERO,
        || panic!("clean cancellation must not take error drain path"),
    );
    assert!(matches!(result, Ok(Startup::Closed(Exit::ParentEof))));
    assert_eq!(bridge.borrow().buffered_frames(), 0);
}

#[test]
fn transport_fault_wins_over_already_observed_clean_eof_at_ready_commit() {
    let mut bridge = PipeBridge::new();
    bridge.read_once(&mut io::empty()).unwrap();
    bridge.poison(Diagnostic::TransportLost);
    assert_eq!(
        bridge.enqueue_ready(ready()),
        Err(Diagnostic::TransportLost)
    );
    assert_eq!(bridge.buffered_frames(), 0);
}

#[test]
fn ready_commit_distinguishes_clean_eof_from_finish_without_eof() {
    let mut bridge = PipeBridge::new();
    bridge.read_once(&mut io::empty()).unwrap();
    assert_eq!(bridge.enqueue_ready(ready()), Ok(ReadyEnqueue::ParentEof));
    let mut finished = PipeBridge::new();
    finished.finish();
    assert_eq!(
        finished.enqueue_ready(ready()),
        Err(Diagnostic::TransportLost)
    );
}

#[test]
fn ready_commit_never_turns_outbox_overflow_into_clean_cancellation() {
    let mut bridge = PipeBridge::new();
    for _ in 0..32 {
        bridge
            .enqueue(RendererMessage::Heartbeat(Heartbeat { version: V1 }))
            .unwrap();
    }
    assert_eq!(
        bridge.enqueue_ready(ready()),
        Err(Diagnostic::ControlOverflow)
    );
    assert!(bridge.failure().is_some());
}

#[test]
fn empty_queue_after_write_fault_still_cannot_be_a_clean_receipt() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut bridge = PipeBridge::new();
    bridge.enqueue_ready(ready()).unwrap();
    assert_eq!(
        bridge.write_once(&mut Broken),
        Err(Diagnostic::TransportLost)
    );
    assert_eq!(bridge.buffered_frames(), 0); // core Outbox is allowed to drop on failure
    assert_ne!(
        process_exit_code(
            Exit::ParentEof,
            CompletionSnapshot {
                done: true,
                fault: true,
                pending_frames: Some(0),
            }
        ),
        0
    );
}
