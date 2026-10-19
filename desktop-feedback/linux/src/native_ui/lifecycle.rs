//! UI-thread lifecycle and ports shared by native code and deterministic tests.
use super::{startup_input::StartupInput, Style, UiFailure, WindowPolicy};
use rpa_desktop_feedback::{
    protocol::{
        CaptureExclusion, Heartbeat, Ready, RendererError, RendererMessage, Session, Snapshot,
        Stop, V1,
    },
    Diagnostic,
};
use rpa_desktop_feedback_linux::{
    geometry::{self, Layout, Monitor},
    model::{FeedbackModel, Presentation},
};
use std::{marker::PhantomData, rc::Rc, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSet {
    pub status: u32,
    pub stop: u32,
    pub ring: u32,
    pub epoch: u64,
}
#[derive(Debug, Clone)]
pub struct DisplayFacts {
    pub monitors: Vec<Monitor>,
    pub scale: f64,
    pub epoch: u64,
    pub input_shapes: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub control_revision: u64,
    pub control_session: Option<Session>,
    pub style: Style,
    pub stop_visual: ButtonVisual,
    pub layout: Layout,
    pub ring_center: Option<(i32, i32)>,
    pub stop_enabled: bool,
    pub presentation: Presentation,
}
/// Native adapter must derive these from checked X requests/server observations,
/// never assume them from a successful local allocation or from an intent flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Receipt {
    pub server_checked: bool,
    pub status_viewable: bool,
    pub stop_viewable: bool,
    pub ring_initialized: bool,
    pub window_epoch: u64,
    pub topology_epoch: u64,
}
pub trait Native {
    fn facts(&mut self) -> Result<DisplayFacts, UiFailure>;
    fn create(&mut self) -> Result<WindowSet, UiFailure>;
    fn configure(
        &mut self,
        windows: WindowSet,
        policies: &[WindowPolicy; 3],
    ) -> Result<(), UiFailure>;
    fn paint(&mut self, windows: WindowSet, scene: &Scene) -> Result<(), UiFailure>;
    fn present_checked(&mut self, windows: WindowSet, scene: &Scene) -> Result<Receipt, UiFailure>;
    /// Owns partial startup allocations too. Must not wait on a broken X server.
    fn destroy_all(&mut self);
}

#[derive(Debug, Clone)]
pub enum Input {
    Idle,
    Snapshot(Snapshot),
    Eof,
    Failed(Diagnostic),
}
/// Ready commit result, distinct from a generic transport enqueue failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyEnqueue {
    Enqueued,
    ParentEof,
}
/// UI uses only nonblocking mailbox operations. The concrete transport MUST reuse
/// core Inbox/Outbox; it may not call read/write/flush/join on the UI thread.
pub trait Transport {
    fn failure(&self) -> Option<Diagnostic>;
    fn take_input(&mut self) -> Input;
    fn enqueue(&mut self, message: RendererMessage) -> Result<(), Diagnostic>;
    /// The OS mailbox overrides this to observe EOF and commit Ready under one
    /// lock. Generic implementations never reinterpret enqueue errors as EOF.
    fn enqueue_ready(&mut self, ready: Ready) -> Result<ReadyEnqueue, Diagnostic> {
        self.enqueue(RendererMessage::Ready(ready))?;
        Ok(ReadyEnqueue::Enqueued)
    }
    fn finish(&mut self);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Button {
    pub window: u32,
    pub epoch: u64,
    pub button: u8,
    /// Stop-window-local physical coordinates, not logical layout units.
    pub x: i32,
    pub y: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVisual {
    Normal,
    Hover,
    Pressed,
    Disabled,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Hover {
        window: u32,
        epoch: u64,
        inside: bool,
    },
    Poll,
    Expose,
    Press(Button),
    NativePress {
        button: Button,
        session: Session,
        revision: u64,
    },
    NativeRelease {
        button: Button,
        session: Session,
        revision: u64,
    },
    NativeClose {
        window: u32,
        epoch: u64,
        session: Session,
        revision: u64,
    },
    Release(Button),
    Close {
        window: u32,
        epoch: u64,
    },
    TopologyChanged,
    NativeLost,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    ParentEof,
    Failure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Running,
    Exited(Exit),
}

/// UI ownership cannot be transferred to a pipe worker. Heartbeat is produced
/// only after a successful native/UI turn, never by the transport thread.
pub struct Runtime<N: Native, T: Transport> {
    native: N,
    transport: T,
    startup_input: StartupInput,
    style: Style,
    windows: Option<WindowSet>,
    facts: Option<DisplayFacts>,
    model: FeedbackModel,
    snapshot: Option<Snapshot>,
    scene: Option<Scene>,
    pressed: Option<Session>,
    hovered: bool,
    started: bool,
    exited: Option<Exit>,
    last: Duration,
    heartbeat: Duration,
    revision: u64,
    ui_thread_only: PhantomData<Rc<()>>,
}
impl<N: Native, T: Transport> Runtime<N, T> {
    pub fn new(native: N, transport: T, style: Style) -> Self {
        Self {
            native,
            transport,
            startup_input: StartupInput::default(),
            style,
            windows: None,
            facts: None,
            model: FeedbackModel::new(),
            snapshot: None,
            scene: None,
            pressed: None,
            hovered: false,
            started: false,
            exited: None,
            last: Duration::ZERO,
            heartbeat: Duration::ZERO,
            revision: 0,
            ui_thread_only: PhantomData,
        }
    }
    pub fn native_mut(&mut self) -> &mut N {
        &mut self.native
    }
    pub(super) fn seed_startup_input(&mut self, input: StartupInput) {
        self.startup_input = input;
    }
    pub fn exit_reason(&self) -> Option<Exit> {
        self.exited
    }
    /// No native operation may follow an observed startup cancellation. Partial
    /// ownership still goes through the same once-only native/transport cleanup.
    fn startup_checkpoint(&mut self) -> Result<bool, UiFailure> {
        if self.exited.is_some() {
            return Ok(false);
        }
        if self.startup_input.continuing(&mut self.transport)? {
            Ok(true)
        } else {
            self.terminate(Exit::ParentEof);
            Ok(false)
        }
    }
    pub fn start(&mut self, now: Duration) -> Result<(), UiFailure> {
        if self.started || self.exited.is_some() {
            return Err(UiFailure::AlreadyStarted);
        }
        self.started = true;
        self.last = now;
        self.heartbeat = now;
        let result = self.initialize(now);
        if result.is_err() {
            self.terminate(Exit::Failure);
        }
        result
    }
    fn initialize(&mut self, now: Duration) -> Result<(), UiFailure> {
        if !self.startup_checkpoint()? {
            return Ok(());
        }
        let facts = self.native.facts();
        if !self.startup_checkpoint()? {
            return Ok(());
        }
        let facts = facts?;
        self.validate_facts(&facts)?;
        self.facts = Some(facts);
        let windows = self.native.create();
        if !self.startup_checkpoint()? {
            return Ok(());
        }
        let windows = windows?;
        if windows.status == 0
            || windows.stop == 0
            || windows.ring == 0
            || windows.epoch == 0
            || windows.status == windows.stop
            || windows.status == windows.ring
            || windows.stop == windows.ring
        {
            return Err(UiFailure::InvalidWindows);
        }
        self.windows = Some(windows);
        let configured = self.native.configure(windows, &super::window_policies());
        if !self.startup_checkpoint()? {
            return Ok(());
        }
        configured?;
        self.render(now, true, true)?;
        if !self.startup_checkpoint()? {
            return Ok(());
        }
        match self
            .transport
            .enqueue_ready(Ready {
                version: V1,
                capture_exclusion: CaptureExclusion::Unsupported,
                pointer_feedback: true,
            })
            .map_err(|_| UiFailure::Transport)?
        {
            ReadyEnqueue::Enqueued => {}
            ReadyEnqueue::ParentEof => {
                self.terminate(Exit::ParentEof);
            }
        }
        Ok(())
    }
    fn validate_facts(&self, facts: &DisplayFacts) -> Result<(), UiFailure> {
        if !facts.input_shapes {
            return Err(UiFailure::InputShapeUnavailable);
        }
        if facts.monitors.is_empty() || facts.monitors.len() > 64 || facts.epoch == 0 {
            return Err(UiFailure::Geometry);
        }
        for (i, m) in facts.monitors.iter().enumerate() {
            geometry::layout(m, facts.scale).map_err(|_| UiFailure::Geometry)?;
            for n in &facts.monitors[..i] {
                let a = m.bounds;
                let b = n.bounds;
                if m.key == n.key
                    || (i64::from(a.x) < i64::from(b.x) + i64::from(b.width)
                        && i64::from(b.x) < i64::from(a.x) + i64::from(a.width)
                        && i64::from(a.y) < i64::from(b.y) + i64::from(b.height)
                        && i64::from(b.y) < i64::from(a.y) + i64::from(a.height))
                {
                    return Err(UiFailure::Geometry);
                }
            }
        }
        Ok(())
    }
    fn render(&mut self, now: Duration, force: bool, startup: bool) -> Result<(), UiFailure> {
        let facts = self.facts.as_ref().ok_or(UiFailure::Geometry)?;
        let topology_epoch = facts.epoch;
        let (anchor, ring_center) =
            if let Some(surface) = self.snapshot.as_ref().and_then(|s| s.surface.as_ref()) {
                let pointer = self.model.ring(now);
                let placement = geometry::resolve(surface, pointer.as_ref(), &facts.monitors)
                    .map_err(|_| UiFailure::Geometry)?;
                (placement.anchor, placement.ring_center)
            } else {
                (
                    facts
                        .monitors
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| a.key.cmp(&b.key))
                        .ok_or(UiFailure::Geometry)?
                        .0,
                    None,
                )
            };
        let stop_enabled = self.model.can_stop();
        let scene = Scene {
            control_revision: self.revision,
            control_session: self.snapshot.as_ref().and_then(|s| s.session.clone()),
            style: self.style.clone(),
            stop_visual: if !stop_enabled {
                ButtonVisual::Disabled
            } else if self.pressed.is_some() {
                ButtonVisual::Pressed
            } else if self.hovered {
                ButtonVisual::Hover
            } else {
                ButtonVisual::Normal
            },
            layout: geometry::layout(&facts.monitors[anchor], facts.scale)
                .map_err(|_| UiFailure::Geometry)?,
            ring_center,
            stop_enabled,
            presentation: self.model.presentation(),
        };
        if !force && self.scene.as_ref() == Some(&scene) {
            return Ok(());
        }
        let windows = self.windows.ok_or(UiFailure::InvalidWindows)?;
        if startup && !self.startup_checkpoint()? {
            return Ok(());
        }
        let painted = self.native.paint(windows, &scene);
        if startup && !self.startup_checkpoint()? {
            return Ok(());
        }
        painted?;
        let receipt = self.native.present_checked(windows, &scene);
        if startup && !self.startup_checkpoint()? {
            return Ok(());
        }
        let receipt = receipt?;
        if !receipt.server_checked
            || !receipt.status_viewable
            || !receipt.stop_viewable
            || !receipt.ring_initialized
            || receipt.window_epoch != windows.epoch
            || receipt.topology_epoch != topology_epoch
        {
            return Err(UiFailure::IncompletePresentation);
        }
        self.scene = Some(scene);
        Ok(())
    }
    pub fn step(&mut self, now: Duration, event: Event) -> Flow {
        if let Some(exit) = self.exited {
            return Flow::Exited(exit);
        }
        if !self.started {
            return Flow::Running;
        }
        if now < self.last || self.transport.failure().is_some() {
            return self.terminate(Exit::Failure);
        }
        self.last = now;
        match self.startup_input.take(&mut self.transport) {
            Input::Eof => return self.terminate(Exit::ParentEof),
            Input::Failed(_) => return self.terminate(Exit::Failure),
            Input::Snapshot(snapshot) => match self.model.apply(snapshot.clone(), now) {
                Ok(true) => {
                    if self.snapshot.as_ref().and_then(|s| s.session.as_ref())
                        != snapshot.session.as_ref()
                        || self.snapshot.as_ref().and_then(|s| s.surface.as_ref())
                            != snapshot.surface.as_ref()
                    {
                        self.revision = match self.revision.checked_add(1) {
                            Some(v) => v,
                            None => return self.terminate(Exit::Failure),
                        };
                        self.pressed = None;
                        self.hovered = false;
                    }
                    self.snapshot = Some(snapshot);
                }
                Ok(false) => {}
                Err(_) => return self.terminate(Exit::Failure),
            },
            Input::Idle => {}
        }
        // Geometry/layout changes must be resolved before hit-testing a queued event.
        if matches!(event, Event::TopologyChanged) {
            self.revision = match self.revision.checked_add(1) {
                Some(v) => v,
                None => return self.terminate(Exit::Failure),
            };
            self.pressed = None;
            self.hovered = false;
            self.model.invalidate_geometry();
            let facts = match self.native.facts() {
                Ok(facts) => facts,
                Err(_) => return self.terminate(Exit::Failure),
            };
            if self.validate_facts(&facts).is_err() {
                return self.terminate(Exit::Failure);
            }
            self.facts = Some(facts);
        }
        if self
            .render(
                now,
                matches!(event, Event::TopologyChanged | Event::Expose),
                false,
            )
            .is_err()
        {
            return self.terminate(Exit::Failure);
        }
        let windows = self.windows.expect("initialized runtime owns windows");
        let event = match event {
            Event::NativePress {
                button,
                session,
                revision,
            } => {
                if revision == self.revision
                    && self.snapshot.as_ref().and_then(|s| s.session.as_ref()) == Some(&session)
                {
                    Event::Press(button)
                } else {
                    self.pressed = None;
                    Event::Poll
                }
            }
            Event::NativeRelease {
                button,
                session,
                revision,
            } => {
                if revision == self.revision
                    && self.snapshot.as_ref().and_then(|s| s.session.as_ref()) == Some(&session)
                {
                    Event::Release(button)
                } else {
                    self.pressed = None;
                    Event::Poll
                }
            }
            Event::NativeClose {
                window,
                epoch,
                session,
                revision,
            } => {
                if revision == self.revision
                    && self.snapshot.as_ref().and_then(|s| s.session.as_ref()) == Some(&session)
                {
                    Event::Close { window, epoch }
                } else {
                    Event::Poll
                }
            }
            other => other,
        };
        match event {
            Event::NativeLost => return self.terminate(Exit::Failure),
            Event::Press(button) => {
                self.pressed = if self.valid_button(button) && self.model.can_stop() {
                    self.snapshot.as_ref().and_then(|s| s.session.clone())
                } else {
                    None
                };
            }
            Event::Release(button) => {
                let pressed = self.pressed.take();
                if self.valid_button(button)
                    && pressed.is_some()
                    && pressed.as_ref() == self.snapshot.as_ref().and_then(|s| s.session.as_ref())
                    && self.send_stop().is_err()
                {
                    return self.terminate(Exit::Failure);
                }
            }
            Event::Hover {
                window,
                epoch,
                inside,
            } if window == windows.stop && epoch == windows.epoch => self.hovered = inside,
            Event::Close { window, epoch }
                if epoch == windows.epoch
                    && (window == windows.status || window == windows.stop) =>
            {
                self.pressed = None;
                if self.send_stop().is_err() {
                    return self.terminate(Exit::Failure);
                }
            }
            _ => {}
        }
        if self.render(now, false, false).is_err() {
            return self.terminate(Exit::Failure);
        }
        if now.saturating_sub(self.heartbeat) >= Duration::from_secs(1) {
            if self
                .transport
                .enqueue(RendererMessage::Heartbeat(Heartbeat { version: V1 }))
                .is_err()
            {
                return self.terminate(Exit::Failure);
            }
            self.heartbeat = now;
        }
        Flow::Running
    }
    fn valid_button(&self, b: Button) -> bool {
        let Some(w) = self.windows else {
            return false;
        };
        let Some(scene) = &self.scene else {
            return false;
        };
        b.window == w.stop
            && b.epoch == w.epoch
            && b.button == 1
            && b.x >= 0
            && b.y >= 0
            && (b.x as u32) < scene.layout.stop.width
            && (b.y as u32) < scene.layout.stop.height
    }
    fn send_stop(&mut self) -> Result<(), Diagnostic> {
        if let Some(session) = self.model.request_stop() {
            self.transport.enqueue(RendererMessage::Stop(Stop {
                version: V1,
                session,
            }))?;
        }
        Ok(())
    }
    pub fn shutdown(&mut self, exit: Exit) -> Flow {
        self.terminate(exit)
    }
    fn terminate(&mut self, exit: Exit) -> Flow {
        if let Some(exit) = self.exited {
            return Flow::Exited(exit);
        }
        self.exited = Some(exit);
        self.pressed = None;
        if exit == Exit::Failure {
            let _ = self
                .transport
                .enqueue(RendererMessage::Error(RendererError {
                    version: V1,
                    code: "renderer_ui_failed".into(),
                }));
        }
        self.native.destroy_all();
        self.transport.finish();
        Flow::Exited(exit)
    }
}
