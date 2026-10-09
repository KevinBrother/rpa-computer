//! Facts, never request-name guesses. This wrapper stays on the native thread.
use super::{authority::DispatchStamp, FeedbackHandle};
use crate::backend::{Backend, BackendError, Capture, Direction, Geometry, InputEvent};
use rpa_desktop_feedback::protocol::{Cleanup, Phase, Pointer, PointerKind};

pub struct FactBackend {
    inner: Box<dyn Backend>,
    feedback: FeedbackHandle,
    position: Option<(DispatchStamp, i32, i32)>,
    held_buttons: Vec<String>,
    regions: Vec<rpa_display_topology::NativeRect>,
}
impl FactBackend {
    pub fn new(inner: Box<dyn Backend>, feedback: FeedbackHandle) -> Self {
        Self {
            inner,
            feedback,
            position: None,
            held_buttons: Vec::with_capacity(5),
            regions: Vec::new(),
        }
    }
}
impl Backend for FactBackend {
    fn display_selections(&self) -> &'static [&'static str] {
        self.inner.display_selections()
    }
    fn platform(&self) -> &'static str {
        self.inner.platform()
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.inner.geometry()
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        let tag = self
            .feedback
            .begin_dispatch(Phase::Observing)
            .map_err(revoked)?;
        let result = self.inner.capture();
        if let (Some(tag), Ok(capture)) = (tag.as_ref(), &result) {
            self.feedback.captured(tag, &capture.geometry);
        }
        result
    }
    fn display_snapshot(
        &mut self,
    ) -> Result<Option<rpa_display_topology::TopologySnapshot>, BackendError> {
        self.inner.display_snapshot()
    }
    fn select_display(
        &mut self,
        s: &rpa_display_topology::Selection,
    ) -> Result<Geometry, BackendError> {
        self.regions.clear();
        self.position = None;
        self.inner.select_display(s)
    }
    fn capture_display(
        &mut self,
        b: rpa_display_topology::CaptureBudget,
    ) -> Result<Option<crate::backend::display::DisplayCapture>, BackendError> {
        let tag = self
            .feedback
            .begin_dispatch(Phase::Observing)
            .map_err(revoked)?;
        let result = self.inner.capture_display(b);
        if let Ok(Some(c)) = &result {
            self.regions = c
                .mapping
                .regions()
                .iter()
                .map(|r| r.native_bounds)
                .collect();
            if let Some(tag) = tag {
                self.feedback.captured(&tag, &c.geometry);
            }
        }
        result
    }
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        let tag = self
            .feedback
            .begin_dispatch(Phase::Executing)
            .map_err(revoked)?;
        // Admission is revoked before each actual atomic input. No lock is held
        // across native calls; an already-entered native call remains in-flight
        // and cleanup/unknown is reported truthfully by the existing worker.
        if self.feedback.is_terminated() {
            return Err(revoked(super::FeedbackError::AuthorityRevoked));
        }
        self.inner.inject(event)?;
        let Some(tag) = tag else {
            return Ok(());
        };
        if self
            .position
            .as_ref()
            .is_some_and(|(old, _, _)| old != &tag)
        {
            self.position = None;
            self.held_buttons.clear();
        }
        match event {
            InputEvent::Move { x, y } => {
                if !self.regions.is_empty()
                    && !self.regions.iter().any(|r| {
                        i64::from(*x) >= i64::from(r.x)
                            && i64::from(*y) >= i64::from(r.y)
                            && i64::from(*x) < i64::from(r.x) + i64::from(r.width)
                            && i64::from(*y) < i64::from(r.y) + i64::from(r.height)
                    })
                {
                    self.position = None;
                    return Ok(());
                } // confirmed internal gap transit, no ring target
                self.position = Some((tag.clone(), *x, *y));
                self.feedback.confirmed(
                    &tag,
                    Pointer {
                        x: *x as f64,
                        y: *y as f64,
                        kind: if self.held_buttons.is_empty() {
                            PointerKind::Move
                        } else {
                            PointerKind::Drag
                        },
                    },
                );
            }
            InputEvent::Button {
                button, direction, ..
            } => {
                if *direction == Direction::Press {
                    if self.held_buttons.len() < 5 && !self.held_buttons.contains(button) {
                        self.held_buttons.push(button.clone());
                    }
                    if let Some((old, x, y)) = &self.position {
                        if old == &tag {
                            self.feedback.confirmed(
                                &tag,
                                Pointer {
                                    x: *x as f64,
                                    y: *y as f64,
                                    kind: PointerKind::Click,
                                },
                            );
                        }
                    }
                } else {
                    self.held_buttons.retain(|b| b != button);
                }
            }
            // Never retain text/key/wheel content, even on failure.
            _ => {}
        }
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        let result = self.inner.release_all();
        self.feedback.released(if result.is_ok() {
            Cleanup::Released
        } else {
            Cleanup::Failed
        });
        if result.is_ok() {
            self.held_buttons.clear();
            self.position = None;
        }
        result
    }
}
fn revoked(_: super::FeedbackError) -> BackendError {
    BackendError::new(
        "cancelled",
        "desktop feedback revoked this Host child's control",
    )
}
