use super::support::*;
use crate::backend::{capture::expected_capture_size_for_platform, display::DisplayProvider};
use rpa_display_topology::{NativeUnit, PixelSize, Selection};

#[test]
fn root_pixel_size_ignores_xft_dpi_and_fractional_scale() {
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        assert_eq!(
            expected_capture_size_for_platform("linux", 1920, 1080, scale),
            (1920, 1080),
            "Xft scale {scale} must not rescale X11 root pixels",
        );
    }
}

#[test]
fn non_linux_backing_resolution_policy_is_unchanged() {
    assert_eq!(
        expected_capture_size_for_platform("macos", 1512, 982, 2.0),
        (3024, 1964)
    );
    assert_eq!(
        expected_capture_size_for_platform("windows", 1920, 1080, 1.25),
        (2400, 1350)
    );
}

#[test]
fn xft_dpi_change_does_not_invalidate_root_pixel_geometry() {
    let shared = facts(pair());
    let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
    let before = provider.geometry().unwrap();
    shared.borrow_mut().monitors[0].xft_scale = 2.0;
    assert_eq!(
        provider.geometry().unwrap(),
        before,
        "font DPI alone is not a root topology change"
    );
}

#[test]
fn randr_geometry_is_physical_and_preserves_root_origin() {
    let mut monitor = Monitor::new("x11:root:1:output:21", true, 1600, 120, 1440, 900);
    monitor.xft_scale = 1.5;
    let mut provider = DisplayProvider::new(Frames(facts(vec![monitor]))).unwrap();
    let geometry = provider.select(&Selection::Primary).unwrap();
    assert_eq!(geometry.input_origin, (1600, 120));
    assert_eq!(geometry.input_size, (1440, 900));
    let snapshot = provider.snapshot().unwrap();
    assert_eq!(snapshot.unit(), NativeUnit::PhysicalPixels);
    assert_eq!(
        snapshot.displays()[0].capture_size,
        PixelSize {
            width: 1440,
            height: 900
        }
    );
    assert_eq!(snapshot.displays()[0].scale, 1.0);
}
