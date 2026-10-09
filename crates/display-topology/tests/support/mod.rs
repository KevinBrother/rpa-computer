#![allow(dead_code)]
// Shared construction only; expected values are hand-written in each test.
use rpa_display_topology::*;

pub(crate) fn display(
    id: &str,
    primary: bool,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    dpi: u32,
) -> DisplayDescriptor {
    DisplayDescriptor {
        id: id.into(),
        is_primary: primary,
        native_bounds: NativeRect {
            x,
            y,
            width: w,
            height: h,
        },
        capture_size: PixelSize {
            width: w * dpi,
            height: h * dpi,
        },
        scale: dpi as f64,
        rotation: Rotation::Deg0,
    }
}
pub(crate) fn snapshot(displays: Vec<DisplayDescriptor>) -> TopologySnapshot {
    TopologyTracker::new()
        .unwrap()
        .update(NativeUnit::Points, displays)
        .unwrap()
}
pub(crate) fn budget(w: u64, h: u64, pixels: u64) -> CaptureBudget {
    CaptureBudget::new(w, h, pixels).unwrap()
}
pub(crate) fn wide_budget() -> CaptureBudget {
    budget(10000, 10000, 100_000_000)
}
pub(crate) fn mapping(s: &TopologySnapshot, selection: Selection) -> ObservationMapping {
    let plan = CapturePlan::new(s, selection, wide_budget()).unwrap();
    ObservationMapping::new(&plan, plan.size(), None).unwrap()
}
pub(crate) fn image(x: f64, y: f64) -> ImagePoint {
    ImagePoint { x, y }
}
pub(crate) fn native(x: i32, y: i32) -> NativePoint {
    NativePoint { x, y }
}
