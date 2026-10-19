//! Observable native fake, real bounded Outbox, and deterministic clock inputs.
//! A fake receipt is NOT evidence that a real X window exists or is visible.
use crate::native_ui::*;
use rpa_desktop_feedback::{protocol::*, Diagnostic};
use rpa_desktop_feedback_linux::{
    geometry::{Monitor, Rect},
    ipc::{Outbox, WriteProgress},
};
use std::{cell::RefCell, rc::Rc, time::Duration};

type Trace = Rc<RefCell<Vec<&'static str>>>;
pub fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}
pub fn valid_button() -> Button {
    Button {
        window: 12,
        epoch: 1,
        button: 1,
        x: 10,
        y: 10,
    }
}
pub fn press() -> Event {
    Event::Press(valid_button())
}
pub fn release() -> Event {
    Event::Release(valid_button())
}

pub struct NativeState {
    pub facts: DisplayFacts,
    pub windows: WindowSet,
    pub receipt: Receipt,
    pub scene: Option<Scene>,
    pub fail_at: Option<&'static str>,
    pub destroy_calls: usize,
}
pub struct FakeNative {
    state: Rc<RefCell<NativeState>>,
    trace: Trace,
}
impl FakeNative {
    fn operation(&self, stage: &'static str) -> Result<(), UiFailure> {
        self.trace.borrow_mut().push(stage);
        if self.state.borrow().fail_at == Some(stage) {
            Err(UiFailure::Native)
        } else {
            Ok(())
        }
    }
}
impl Native for FakeNative {
    fn facts(&mut self) -> Result<DisplayFacts, UiFailure> {
        self.operation("facts")?;
        Ok(self.state.borrow().facts.clone())
    }
    fn create(&mut self) -> Result<WindowSet, UiFailure> {
        self.operation("create")?;
        Ok(self.state.borrow().windows)
    }
    fn configure(
        &mut self,
        _windows: WindowSet,
        policies: &[WindowPolicy; 3],
    ) -> Result<(), UiFailure> {
        self.operation("configure")?;
        assert_eq!(
            policies.map(|p| p.input),
            [
                InputRegion::Empty,
                InputRegion::StopRectangle,
                InputRegion::Empty
            ]
        );
        for p in policies {
            assert!(!p.accepts_focus && !p.requests_activation && !p.grabs_keyboard_or_pointer);
        }
        Ok(())
    }
    fn paint(&mut self, _windows: WindowSet, scene: &Scene) -> Result<(), UiFailure> {
        self.operation("paint")?;
        self.state.borrow_mut().scene = Some(scene.clone());
        Ok(())
    }
    fn present_checked(
        &mut self,
        _windows: WindowSet,
        _scene: &Scene,
    ) -> Result<Receipt, UiFailure> {
        self.operation("present_checked")?;
        Ok(self.state.borrow().receipt)
    }
    fn destroy_all(&mut self) {
        self.trace.borrow_mut().push("destroy");
        self.state.borrow_mut().destroy_calls += 1;
    }
}

pub struct TransportState {
    pub input: Input,
    pub outbox: Outbox,
    pub accepted: Vec<RendererMessage>,
    pub failed: Option<Diagnostic>,
    pub finished: bool,
}
pub struct FakeTransport {
    state: Rc<RefCell<TransportState>>,
    trace: Trace,
}
impl Transport for FakeTransport {
    fn failure(&self) -> Option<Diagnostic> {
        self.state.borrow().failed
    }
    fn take_input(&mut self) -> Input {
        std::mem::replace(&mut self.state.borrow_mut().input, Input::Idle)
    }
    fn enqueue(&mut self, message: RendererMessage) -> Result<(), Diagnostic> {
        let mut state = self.state.borrow_mut();
        if state.finished {
            return Err(Diagnostic::TransportLost);
        }
        if let Some(error) = state.failed {
            return Err(error);
        }
        state.outbox.enqueue(message.clone())?;
        self.trace.borrow_mut().push(match &message {
            RendererMessage::Ready(_) => "ready",
            RendererMessage::Stop(_) => "stop",
            RendererMessage::Heartbeat(_) => "heartbeat",
            RendererMessage::Error(_) => "error",
        });
        state.accepted.push(message);
        Ok(())
    }
    fn finish(&mut self) {
        self.state.borrow_mut().finished = true;
    }
}

pub struct Harness {
    pub runtime: Runtime<FakeNative, FakeTransport>,
    pub native: Rc<RefCell<NativeState>>,
    pub transport: Rc<RefCell<TransportState>>,
    pub trace: Trace,
}
impl Harness {
    pub fn new() -> Self {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let native = Rc::new(RefCell::new(NativeState {
            facts: DisplayFacts {
                monitors: vec![Monitor {
                    key: "native-a".into(),
                    bounds,
                    workarea: bounds,
                }],
                scale: 1.,
                epoch: 1,
                input_shapes: true,
            },
            windows: WindowSet {
                status: 11,
                stop: 12,
                ring: 13,
                epoch: 1,
            },
            receipt: Receipt {
                server_checked: true,
                status_viewable: true,
                stop_viewable: true,
                ring_initialized: true,
                window_epoch: 1,
                topology_epoch: 1,
            },
            scene: None,
            fail_at: None,
            destroy_calls: 0,
        }));
        let transport = Rc::new(RefCell::new(TransportState {
            input: Input::Idle,
            outbox: Outbox::new(),
            accepted: Vec::new(),
            failed: None,
            finished: false,
        }));
        let runtime = Runtime::new(
            FakeNative {
                state: native.clone(),
                trace: trace.clone(),
            },
            FakeTransport {
                state: transport.clone(),
                trace: trace.clone(),
            },
            Style {
                label: "AI Control".into(),
                accent: [0x25, 0x63, 0xeb],
            },
        );
        Self {
            runtime,
            native,
            transport,
            trace,
        }
    }
    pub fn started() -> Self {
        let mut h = Self::new();
        h.runtime.start(ms(0)).unwrap();
        h
    }
    pub fn messages(&self) -> Vec<RendererMessage> {
        self.transport.borrow().accepted.clone()
    }
    pub fn stops(&self) -> Vec<Session> {
        self.messages()
            .into_iter()
            .filter_map(|m| match m {
                RendererMessage::Stop(s) => Some(s.session),
                _ => None,
            })
            .collect()
    }
    pub fn heartbeats(&self) -> usize {
        self.messages()
            .iter()
            .filter(|m| matches!(m, RendererMessage::Heartbeat(_)))
            .count()
    }
    pub fn scene(&self) -> Scene {
        self.native
            .borrow()
            .scene
            .clone()
            .expect("native paint must receive a scene")
    }
    pub fn apply(&mut self, snapshot: Snapshot, time: u64) {
        self.transport.borrow_mut().input = Input::Snapshot(snapshot);
        assert_eq!(self.runtime.step(ms(time), Event::Poll), Flow::Running);
    }
    pub fn click(&mut self, time: u64) {
        self.runtime.step(ms(time), press());
        self.runtime.step(ms(time + 1), release());
    }
    pub fn drain_output(&mut self) {
        let mut bytes = Vec::new();
        let mut state = self.transport.borrow_mut();
        while state.outbox.pump_once(&mut bytes).unwrap() != WriteProgress::Idle {}
        assert!(bytes.split(|b| *b == b'\n').all(|line| line.len() < 16384));
    }
}
