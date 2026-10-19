//! Pure live-fact validation and layout. No monitor enumeration or Host identity
//! inference: surface IDs/versions remain opaque, and geometry never authorizes input.
use rpa_desktop_feedback::protocol::{Pointer, Surface};

const MAX_MONITORS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }
    fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
    fn valid(self) -> bool {
        // Native X11 root coordinates are nonnegative. Validate core protocol
        // ranges before any future window/shape coordinate narrowing.
        self.x >= 0
            && self.y >= 0
            && i16::try_from(self.x).is_ok()
            && i16::try_from(self.y).is_ok()
            && self.width > 0
            && self.height > 0
            && u16::try_from(self.width).is_ok()
            && u16::try_from(self.height).is_ok()
            && self.right() <= i64::from(u16::MAX)
            && self.bottom() <= i64::from(u16::MAX)
    }
    fn contains_rect(self, other: Self) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
    fn contains(self, x: f64, y: f64) -> bool {
        x.is_finite()
            && y.is_finite()
            && x >= f64::from(self.x)
            && y >= f64::from(self.y)
            && x < self.right() as f64
            && y < self.bottom() as f64
    }
    fn overlaps(self, other: Self) -> bool {
        i64::from(self.x) < other.right()
            && i64::from(other.x) < self.right()
            && i64::from(self.y) < other.bottom()
            && i64::from(other.y) < self.bottom()
    }
    fn matches(self, surface: &Surface) -> bool {
        surface.x == f64::from(self.x)
            && surface.y == f64::from(self.y)
            && surface.width == f64::from(self.width)
            && surface.height == f64::from(self.height)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    /// Native fact identity for stable placement, NOT a Host attachment counter.
    pub key: String,
    pub bounds: Rect,
    pub workarea: Rect,
}
impl Monitor {
    fn validate(&self) -> Result<(), GeometryError> {
        if self.key.is_empty()
            || self.key.len() > 128
            || !self.bounds.valid()
            || !self.workarea.valid()
            || !self.bounds.contains_rect(self.workarea)
        {
            Err(GeometryError::InvalidFacts)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub anchor: usize,
    pub ring_center: Option<(i32, i32)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub bar: Rect,
    pub stop: Rect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryError {
    InvalidFacts,
    SurfaceMismatch,
    WorkareaTooSmall,
    InvalidScale,
}

pub fn resolve(
    surface: &Surface,
    pointer: Option<&Pointer>,
    monitors: &[Monitor],
) -> Result<Placement, GeometryError> {
    surface
        .validate()
        .map_err(|_| GeometryError::SurfaceMismatch)?;
    if monitors.is_empty() || monitors.len() > MAX_MONITORS {
        return Err(GeometryError::InvalidFacts);
    }
    for (index, monitor) in monitors.iter().enumerate() {
        monitor.validate()?;
        if monitors[..index]
            .iter()
            .any(|previous| previous.key == monitor.key || previous.bounds.overlaps(monitor.bounds))
        {
            return Err(GeometryError::InvalidFacts);
        }
    }
    let desktop = surface.id == "desktop";
    let anchor = if desktop {
        let mut left = monitors[0].bounds.x;
        let mut top = monitors[0].bounds.y;
        let mut right = monitors[0].bounds.right();
        let mut bottom = monitors[0].bounds.bottom();
        let mut anchor = 0;
        for (index, monitor) in monitors.iter().enumerate() {
            left = left.min(monitor.bounds.x);
            top = top.min(monitor.bounds.y);
            right = right.max(monitor.bounds.right());
            bottom = bottom.max(monitor.bounds.bottom());
            if monitor.key < monitors[anchor].key {
                anchor = index;
            }
        }
        if surface.x != f64::from(left)
            || surface.y != f64::from(top)
            || surface.width != (right - i64::from(left)) as f64
            || surface.height != (bottom - i64::from(top)) as f64
        {
            return Err(GeometryError::SurfaceMismatch);
        }
        anchor
    } else {
        monitors
            .iter()
            .position(|monitor| monitor.bounds.matches(surface))
            .ok_or(GeometryError::SurfaceMismatch)?
    };
    let ring_center = pointer.and_then(|pointer| {
        // In desktop mode choose an actual monitor, never a bbox gap. In
        // single-monitor mode never paint a ring on an unselected neighbor.
        let bounds = if desktop {
            monitors
                .iter()
                .find(|monitor| monitor.bounds.contains(pointer.x, pointer.y))?
                .bounds
        } else {
            monitors[anchor].bounds
        };
        if !bounds.contains(pointer.x, pointer.y) {
            return None;
        }
        Some((
            pointer
                .x
                .round()
                .clamp(f64::from(bounds.x), (bounds.right() - 1) as f64) as i32,
            pointer
                .y
                .round()
                .clamp(f64::from(bounds.y), (bounds.bottom() - 1) as f64) as i32,
        ))
    });
    Ok(Placement {
        anchor,
        ring_center,
    })
}

pub fn layout(monitor: &Monitor, scale: f64) -> Result<Layout, GeometryError> {
    monitor.validate()?;
    if !scale.is_finite() || scale <= 0. {
        return Err(GeometryError::InvalidScale);
    }
    let scaled = |logical: f64| -> Result<u32, GeometryError> {
        let value = (logical * scale).ceil();
        if !value.is_finite() || value < 1. || value > f64::from(u16::MAX) {
            Err(GeometryError::WorkareaTooSmall)
        } else {
            Ok(value as u32)
        }
    };
    let margin = scaled(16.)?;
    let height = scaled(48.)?;
    let button = scaled(88.)?;
    let gap = scaled(8.)?;
    let minimum_bar = scaled(80.)?;
    let preferred = scaled(448.)?;
    let area = monitor.workarea;
    // All scaled terms are <= u16::MAX, so these sums fit in u32.
    if area.width < 2 * margin + minimum_bar + gap + button || area.height < 2 * margin + height {
        return Err(GeometryError::WorkareaTooSmall);
    }
    let width = preferred
        .max(minimum_bar + gap + button)
        .min(area.width - 2 * margin);
    let x = i64::from(area.x) + i64::from(margin);
    let y = i64::from(area.y) + i64::from(margin);
    let bar_width = width - button - gap;
    let stop_x = x + i64::from(bar_width) + i64::from(gap);
    let position = |value| {
        i16::try_from(value)
            .map(i32::from)
            .map_err(|_| GeometryError::InvalidFacts)
    };
    let bar = Rect {
        x: position(x)?,
        y: position(y)?,
        width: bar_width,
        height,
    };
    let stop = Rect {
        x: position(stop_x)?,
        y: position(y)?,
        width: button,
        height,
    };
    if !bar.valid() || !stop.valid() || !area.contains_rect(bar) || !area.contains_rect(stop) {
        return Err(GeometryError::InvalidFacts);
    }
    Ok(Layout { bar, stop })
}
