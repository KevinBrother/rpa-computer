//! R2: the same connect/start path as the binary, with real Inbox/Outbox semantics.
mod common;
#[path = "../src/native_ui/mod.rs"]
mod native_ui;

use native_ui::{process_boundary::*, *};
use rpa_desktop_feedback::{protocol::*, Diagnostic};
use rpa_desktop_feedback_linux::geometry::{Monitor, Rect};
use std::{
    cell::RefCell,
    io::{self, Cursor, Write},
    rc::Rc,
    time::Duration,
};

const STAGES: [&str; 6] = [
    "connect",
    "facts",
    "create",
    "configure",
    "paint",
    "present_checked",
];
#[derive(Clone, Copy)]
enum Injection {
    CleanEof,
    Truncated,
    Malformed,
    BrokenOutput,
}
struct State {
    bridge: PipeBridge,
    trace: Vec<&'static str>,
    accepted: Vec<RendererMessage>,
    after: Option<&'static str>,
    injection: Injection,
    fail_native: Option<&'static str>,
}
impl State {
    fn inject(&mut self) {
        match self.injection {
            Injection::CleanEof => self.bridge.read_once(&mut io::empty()).unwrap(),
            Injection::Truncated => {
                self.bridge
                    .read_once(&mut Cursor::new(b"{\"type\":".as_slice()))
                    .unwrap();
                assert_eq!(
                    self.bridge.read_once(&mut io::empty()),
                    Err(Diagnostic::TruncatedLine)
                );
            }
            Injection::Malformed => {
                assert_eq!(
                    self.bridge
                        .read_once(&mut Cursor::new(b"not-json\n".as_slice())),
                    Err(Diagnostic::InvalidJson)
                );
            }
            Injection::BrokenOutput => {
                // Exercise the real output path, not just assigning a fake error flag.
                self.bridge
                    .enqueue(RendererMessage::Heartbeat(Heartbeat { version: V1 }))
                    .unwrap();
                assert_eq!(
                    self.bridge.write_once(&mut Broken),
                    Err(Diagnostic::TransportLost)
                );
            }
        }
    }
    fn call(&mut self, stage: &'static str) -> Result<(), UiFailure> {
        self.trace.push(stage);
        if self.after == Some(stage) {
            self.inject();
        }
        if self.fail_native == Some(stage) {
            Err(UiFailure::Native)
        } else {
            Ok(())
        }
    }
}
struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Clone)]
struct Bridge(Rc<RefCell<State>>);
impl Transport for Bridge {
    fn failure(&self) -> Option<Diagnostic> {
        self.0.borrow().bridge.failure()
    }
    fn take_input(&mut self) -> Input {
        self.0.borrow_mut().bridge.take_input()
    }
    fn enqueue(&mut self, message: RendererMessage) -> Result<(), Diagnostic> {
        let mut s = self.0.borrow_mut();
        s.trace.push(match &message {
            RendererMessage::Ready(_) => "ready_attempt",
            RendererMessage::Heartbeat(_) => "heartbeat_attempt",
            RendererMessage::Stop(_) => "stop_attempt",
            RendererMessage::Error(_) => "error_attempt",
        });
        s.bridge.enqueue(message.clone())?;
        s.accepted.push(message);
        Ok(())
    }
    fn finish(&mut self) {
        let mut s = self.0.borrow_mut();
        s.trace.push("finish");
        s.bridge.finish();
    }
}
struct ObservedNative(Rc<RefCell<State>>);
impl Native for ObservedNative {
    fn facts(&mut self) -> Result<DisplayFacts, UiFailure> {
        self.0.borrow_mut().call("facts")?;
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        Ok(DisplayFacts {
            monitors: vec![Monitor {
                key: "native-a".into(),
                bounds,
                workarea: bounds,
            }],
            scale: 1.,
            epoch: 1,
            input_shapes: true,
        })
    }
    fn create(&mut self) -> Result<WindowSet, UiFailure> {
        self.0.borrow_mut().call("create")?;
        Ok(WindowSet {
            status: 11,
            stop: 12,
            ring: 13,
            epoch: 1,
        })
    }
    fn configure(&mut self, _: WindowSet, _: &[WindowPolicy; 3]) -> Result<(), UiFailure> {
        self.0.borrow_mut().call("configure")
    }
    fn paint(&mut self, _: WindowSet, _: &Scene) -> Result<(), UiFailure> {
        self.0.borrow_mut().call("paint")
    }
    fn present_checked(&mut self, _: WindowSet, _: &Scene) -> Result<Receipt, UiFailure> {
        self.0.borrow_mut().call("present_checked")?;
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
        self.0.borrow_mut().trace.push("destroy");
    }
}
struct Harness(Rc<RefCell<State>>);
impl Harness {
    fn new(after: Option<&'static str>, injection: Injection) -> Self {
        let mut state = State {
            bridge: PipeBridge::new(),
            trace: vec![],
            accepted: vec![],
            after,
            injection,
            fail_native: None,
        };
        if after == Some("before_connect") {
            state.inject();
        }
        Self(Rc::new(RefCell::new(state)))
    }
    fn start(&self) -> Result<Startup<ObservedNative, Bridge>, UiFailure> {
        let native = self.0.clone();
        let wait = self.0.clone();
        start_renderer(
            Bridge(self.0.clone()),
            parse_style(&[]).unwrap(),
            move || {
                native.borrow_mut().call("connect")?;
                Ok(ObservedNative(native))
            },
            || Duration::ZERO,
            move || wait.borrow_mut().trace.push("connection_failure_wait"),
        )
    }
}
fn assert_clean_cancellation(after: &'static str) {
    let h = Harness::new(Some(after), Injection::CleanEof);
    let result = h.start();
    let outcome = match result {
        Ok(Startup::Closed(Exit::ParentEof)) => "clean cancellation",
        Ok(Startup::Closed(Exit::Failure)) => "failure exit",
        Ok(Startup::Running(_)) => "running",
        Err(_) => "startup error",
    };
    let expected_calls = if after == "before_connect" {
        vec![]
    } else {
        STAGES[..=STAGES.iter().position(|s| *s == after).unwrap()].to_vec()
    };
    let s = h.0.borrow();
    let calls: Vec<_> = s
        .trace
        .iter()
        .copied()
        .filter(|s| STAGES.contains(s))
        .collect();
    assert_eq!((outcome, calls), ("clean cancellation", expected_calls),
        "R2: clean EOF after {after} must prevent later native work and remain distinct from failure; trace={:?}", s.trace);
    assert!(!s.trace.contains(&"ready_attempt") && !s.trace.contains(&"error_attempt"));
    assert!(s.accepted.is_empty());
    assert!(s.trace.contains(&"finish"));
    assert_eq!(
        s.trace.iter().filter(|s| **s == "destroy").count(),
        usize::from(after != "before_connect"),
        "partial native ownership must be cleaned exactly once"
    );
    assert_eq!(
        process_exit_code(
            Exit::ParentEof,
            CompletionSnapshot {
                done: true,
                fault: false,
                pending_frames: Some(0),
            }
        ),
        0
    );
}

#[test]
fn r2_eof_before_connect_prevents_connection_and_ui() {
    assert_clean_cancellation("before_connect");
}
#[test]
fn r2_eof_during_connect_prevents_facts_and_ui() {
    assert_clean_cancellation("connect");
}
#[test]
fn r2_eof_during_facts_prevents_window_creation() {
    assert_clean_cancellation("facts");
}
#[test]
fn r2_eof_during_create_prevents_configuration_and_paint() {
    assert_clean_cancellation("create");
}
#[test]
fn r2_eof_during_configure_prevents_paint_and_presentation() {
    assert_clean_cancellation("configure");
}
#[test]
fn r2_eof_during_paint_prevents_presentation() {
    assert_clean_cancellation("paint");
}
#[test]
fn r2_eof_during_presentation_prevents_ready() {
    assert_clean_cancellation("present_checked");
}

fn assert_failure(after: &'static str, injection: Injection) {
    let h = Harness::new(Some(after), injection);
    assert!(
        matches!(h.start(), Err(_) | Ok(Startup::Closed(Exit::Failure))),
        "malformed/truncated input or transport failure must not become clean cancellation"
    );
    assert!(h
        .0
        .borrow()
        .accepted
        .iter()
        .all(|m| !matches!(m, RendererMessage::Ready(_))));
}
#[test]
fn r2_truncated_eof_before_start_remains_failure() {
    assert_failure("before_connect", Injection::Truncated);
}
#[test]
fn r2_truncated_eof_during_start_remains_failure() {
    assert_failure("paint", Injection::Truncated);
}
#[test]
fn r2_malformed_before_start_remains_failure() {
    assert_failure("before_connect", Injection::Malformed);
}
#[test]
fn r2_malformed_during_start_remains_failure() {
    assert_failure("create", Injection::Malformed);
}
#[test]
fn r2_output_failure_before_start_remains_failure() {
    assert_failure("before_connect", Injection::BrokenOutput);
}
#[test]
fn r2_output_failure_during_start_remains_failure() {
    assert_failure("present_checked", Injection::BrokenOutput);
}

#[test]
fn r2_connected_normal_start_preserves_presentation_before_unsupported_ready() {
    let h = Harness::new(None, Injection::CleanEof);
    assert!(matches!(h.start(), Ok(Startup::Running(_))));
    let s = h.0.borrow();
    assert_eq!(
        s.trace,
        [
            "connect",
            "facts",
            "create",
            "configure",
            "paint",
            "present_checked",
            "ready_attempt"
        ]
    );
    assert!(matches!(
        s.accepted.as_slice(),
        [RendererMessage::Ready(Ready {
            capture_exclusion: CaptureExclusion::Unsupported,
            ..
        })]
    ));
}

#[test]
fn r2_startup_observation_does_not_lose_pending_snapshot() {
    let h = Harness::new(None, Injection::CleanEof);
    h.0.borrow_mut()
        .bridge
        .read_once(&mut Cursor::new(common::frame(1)))
        .unwrap();
    let Ok(Startup::Running(mut runtime)) = h.start() else {
        panic!("normal startup required")
    };
    assert_eq!(
        runtime.step(Duration::from_millis(1), Event::Poll),
        Flow::Running
    );
    let s = h.0.borrow();
    assert_eq!(
        s.trace.iter().filter(|s| **s == "paint").count(),
        2,
        "startup status checks must not consume and lose a valid snapshot"
    );
}

#[test]
fn r2_connection_failure_retains_native_error_and_bounded_drain_callback() {
    let h = Harness::new(None, Injection::CleanEof);
    h.0.borrow_mut().fail_native = Some("connect");
    assert!(matches!(h.start(), Err(UiFailure::Native)));
    let s = h.0.borrow();
    assert_eq!(
        s.trace,
        [
            "connect",
            "error_attempt",
            "finish",
            "connection_failure_wait"
        ]
    );
    assert!(
        matches!(s.accepted.as_slice(), [RendererMessage::Error(RendererError { code, .. })]
        if code == "renderer_native_failed")
    );
}
