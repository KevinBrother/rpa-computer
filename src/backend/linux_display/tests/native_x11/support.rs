use super::{
    fixture::{Paint, WINDOW_SIZE},
    X11Provider,
};
use image::RgbaImage;
use rpa_display_topology::{
    CaptureBudget, DisplayDescriptor, ImagePoint, NativePoint, NativeRect, ObservationMapping,
};
use rpa_windows_display::{DisplayError, FrameProvider, MemoryBudget, RgbaFrame, SourcePermit};
use std::{cell::RefCell, rc::Rc};

pub(super) fn bytes(b: NativeRect) -> u64 {
    u64::from(b.width) * u64::from(b.height) * 4
}
pub(super) fn exact(b: NativeRect) -> CaptureBudget {
    CaptureBudget::new(
        u64::from(b.width),
        u64::from(b.height),
        u64::from(b.width) * u64::from(b.height),
    )
    .unwrap()
}
pub(super) fn generous() -> MemoryBudget {
    MemoryBudget::new(8 * 1024 * 1024, 64 * 1024 * 1024).unwrap()
}
pub(super) fn decode(png: &[u8], bounds: NativeRect) -> RgbaImage {
    let decoded = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .expect("valid real capture PNG");
    assert_eq!(
        decoded.color(),
        image::ColorType::Rgba8,
        "PNG must encode RGBA8"
    );
    let decoded = decoded.to_rgba8();
    assert_eq!(
        decoded.dimensions(),
        (bounds.width, bounds.height),
        "no implicit DPI scaling/resampling"
    );
    decoded
}
pub(super) fn assert_paint(
    png: &RgbaImage,
    mapping: &ObservationMapping,
    capture: NativeRect,
    paint: Paint,
) {
    // Every painted pixel is checked against a fixture-side oracle, independent
    // of production channel decoding and canvas composition. Alpha is exact too.
    for y in 0..u32::from(WINDOW_SIZE.1) {
        for x in 0..u32::from(WINDOW_SIZE.0) {
            let color = if (12..44).contains(&x) && (20..52).contains(&y) {
                paint.colors[2]
            } else if x >= 60 {
                paint.colors[1]
            } else {
                paint.colors[0]
            };
            let ix = (paint.origin.0 - capture.x) as u32 + x;
            let iy = (paint.origin.1 - capture.y) as u32 + y;
            assert_eq!(
                png.get_pixel(ix, iy).0,
                color,
                "wrong root pixel ({},{})",
                paint.origin.0 + x as i32,
                paint.origin.1 + y as i32
            );
        }
    }
    for (x, y, expected) in paint.samples() {
        let ix = (x - capture.x) as u32;
        let iy = (y - capture.y) as u32;
        assert_eq!(png.get_pixel(ix, iy).0, expected);
        assert_eq!(
            mapping
                .map_image(
                    ImagePoint {
                        x: f64::from(ix),
                        y: f64::from(iy)
                    },
                    mapping.generation()
                )
                .unwrap(),
            NativePoint { x, y }
        );
    }
}

/// Delegation only: all topology/frames/errors come from the real provider.
pub(super) struct Shared(pub Rc<RefCell<X11Provider>>);
impl FrameProvider for Shared {
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        self.0.borrow_mut().enumerate()
    }
    fn capture(
        &mut self,
        display: &DisplayDescriptor,
        permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        self.0.borrow_mut().capture(display, permit)
    }
}
