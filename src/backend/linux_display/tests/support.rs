use crate::backend::capture::{expected_capture_size_for_platform, primary_display_index};
use rpa_display_topology::{DisplayDescriptor, NativeRect, PixelSize, Rotation};
use rpa_windows_display::{DisplayError, FrameProvider, RgbaFrame, SourcePermit};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub(super) struct Monitor {
    pub id: String,
    pub primary: bool,
    pub bounds: NativeRect,
    pub xft_scale: f32,
    pub rotation: Rotation,
    pub color: [u8; 4],
}
impl Monitor {
    pub fn new(id: &str, primary: bool, x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            id: id.into(),
            primary,
            bounds: NativeRect {
                x,
                y,
                width,
                height,
            },
            xft_scale: 1.0,
            rotation: Rotation::Deg0,
            color: [17, 31, 53, 255],
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Change {
    Move,
    ReplaceIdentity,
    Primary,
    Rotation,
    XftDpiOnly,
}
pub(super) struct Facts {
    pub monitors: Vec<Monitor>,
    pub queries: usize,
    pub reads: usize,
    pub allocations: usize,
    pub fail_query: bool,
    pub fail_read: bool,
    pub change_on_query: Option<(usize, Change)>,
    pub change_on_read: Option<Change>,
}
pub(super) type Shared = Rc<RefCell<Facts>>;

pub(super) fn facts(monitors: Vec<Monitor>) -> Shared {
    Rc::new(RefCell::new(Facts {
        monitors,
        queries: 0,
        reads: 0,
        allocations: 0,
        fail_query: false,
        fail_read: false,
        change_on_query: None,
        change_on_read: None,
    }))
}
pub(super) fn pair() -> Vec<Monitor> {
    let mut left = Monitor::new("x11:root:1:output:11", true, 0, 0, 4, 3);
    left.color = [240, 10, 20, 255];
    let mut right = Monitor::new("x11:root:1:output:12", false, 6, 1, 2, 4);
    right.color = [10, 20, 240, 255];
    vec![left, right]
}
fn change(facts: &mut Facts, change: Change) {
    match change {
        Change::Move => facts.monitors[0].bounds.x += 1,
        Change::ReplaceIdentity => facts.monitors[0].id.push_str(":replacement"),
        Change::Primary => {
            facts.monitors[0].primary = false;
            facts.monitors[1].primary = true;
        }
        Change::Rotation => facts.monitors[0].rotation = Rotation::Deg90,
        Change::XftDpiOnly => facts.monitors[0].xft_scale = 2.0,
    }
}

/// Test source: facts pass through the same pure sizing/primary decisions as
/// target_screen(), then the real FrameProvider engine owns admission/mapping.
/// This is NOT the native RandR enumeration/GetImage implementation.
pub(super) struct Frames(pub Shared);
impl FrameProvider for Frames {
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        let mut facts = self.0.borrow_mut();
        facts.queries += 1;
        if facts.fail_query {
            return Err(DisplayError::Native {
                operation: "test RandR query",
                code: 1,
            });
        }
        if let Some((at, mutation)) = facts.change_on_query {
            if facts.queries == at {
                change(&mut facts, mutation);
                facts.change_on_query = None;
            }
        }
        let primary =
            primary_display_index(&facts.monitors.iter().map(|m| m.primary).collect::<Vec<_>>())
                .map_err(|_| DisplayError::UnsupportedDisplay {
                    reason: "ambiguous X11 primary",
                })?;
        Ok(facts
            .monitors
            .iter()
            .enumerate()
            .map(|(index, monitor)| {
                let (width, height) = expected_capture_size_for_platform(
                    "linux",
                    monitor.bounds.width,
                    monitor.bounds.height,
                    monitor.xft_scale,
                );
                DisplayDescriptor {
                    id: monitor.id.clone(),
                    is_primary: index == primary,
                    native_bounds: monitor.bounds,
                    capture_size: PixelSize { width, height },
                    scale: f64::from(width) / f64::from(monitor.bounds.width),
                    rotation: monitor.rotation,
                }
            })
            .collect())
    }
    fn capture(
        &mut self,
        display: &DisplayDescriptor,
        permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        let mut facts = self.0.borrow_mut();
        facts.reads += 1;
        assert_eq!(permit.size(), display.capture_size);
        assert_eq!(
            permit.bytes(),
            u64::from(permit.size().width) * u64::from(permit.size().height) * 4
        );
        if facts.fail_read {
            return Err(DisplayError::Native {
                operation: "test GetImage",
                code: 2,
            });
        }
        let color = facts
            .monitors
            .iter()
            .find(|m| m.id == display.id)
            .unwrap()
            .color;
        if let Some(mutation) = facts.change_on_read.take() {
            change(&mut facts, mutation);
        }
        facts.allocations += 1;
        RgbaFrame::new(permit.size(), color.repeat((permit.bytes() / 4) as usize))
    }
}
