use super::support::*;
use crate::backend::{capture::primary_display_index, display::DisplayProvider};
use rpa_display_topology::{CaptureBudget, ImagePoint, NativePoint, Selection};

#[test]
fn single_active_output_without_primary_is_usable() {
    assert_eq!(primary_display_index(&[false]).unwrap(), 0);
    let shared = facts(vec![Monitor::new(
        "x11:root:1:output:31",
        false,
        0,
        0,
        8,
        6,
    )]);
    let mut provider = DisplayProvider::new(Frames(shared)).unwrap();
    assert_eq!(
        provider.select(&Selection::Primary).unwrap().input_size,
        (8, 6)
    );
    assert!(provider.snapshot().unwrap().displays()[0].is_primary);
}

#[test]
fn missing_or_ambiguous_primary_is_not_arbitrary_first_output() {
    assert!(primary_display_index(&[]).is_err());
    assert!(primary_display_index(&[false, false]).is_err());
    assert!(
        primary_display_index(&[true, true]).is_err(),
        "multiple primaries must not silently choose index zero"
    );
    assert_eq!(primary_display_index(&[false, true]).unwrap(), 1);
}

#[test]
fn provider_rejects_multiple_primary_claims_before_reading_pixels() {
    let mut monitors = pair();
    monitors[1].primary = true;
    let shared = facts(monitors);
    let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
    assert!(
        provider.snapshot().is_err(),
        "native primary ambiguity must reach the caller"
    );
    assert_eq!(shared.borrow().reads, 0);
}

#[test]
fn primary_exact_id_and_desktop_use_same_complete_topology() {
    let monitors = pair();
    let second_id = monitors[1].id.clone();
    let shared = facts(monitors);
    let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
    assert_eq!(provider.geometry().unwrap().input_size, (4, 3));
    let selected = provider.select(&Selection::Display(second_id)).unwrap();
    assert_eq!(selected.input_origin, (6, 1));
    assert_eq!(selected.input_size, (2, 4));
    assert_eq!(
        provider
            .select(&Selection::Display("absent".into()))
            .unwrap_err()
            .code,
        "display_not_found"
    );
    assert_eq!(
        provider.geometry().unwrap(),
        selected,
        "failed selection must not retarget"
    );
    let desktop = provider.select(&Selection::Desktop).unwrap();
    assert_eq!(desktop.surface_id, "desktop");
    assert_eq!(desktop.input_origin, (0, 0));
    assert_eq!(desktop.input_size, (8, 5));
    let capture = provider
        .capture(CaptureBudget::new(8, 5, 40).unwrap())
        .unwrap();
    assert_eq!(capture.mapping.regions().len(), 2);
    let image = image::load_from_memory(&capture.png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (8, 5));
    assert_eq!(image.get_pixel(1, 1).0, [240, 10, 20, 255]);
    assert_eq!(image.get_pixel(6, 2).0, [10, 20, 240, 255]);
    assert_eq!(image.get_pixel(4, 2).0, [0, 0, 0, 255]);
    let generation = capture.mapping.generation();
    assert_eq!(
        capture
            .mapping
            .map_image(ImagePoint { x: 7.0, y: 4.0 }, generation)
            .unwrap(),
        NativePoint { x: 7, y: 4 }
    );
    assert!(capture
        .mapping
        .map_image(ImagePoint { x: 4.0, y: 2.0 }, generation)
        .is_err());
    assert!(capture
        .mapping
        .map_image(ImagePoint { x: 8.0, y: 2.0 }, generation)
        .is_err());
    assert_eq!(shared.borrow().reads, 2);
}

#[test]
fn enumeration_order_does_not_change_identity_or_generation() {
    let shared = facts(pair());
    let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
    let before = provider.snapshot().unwrap();
    shared.borrow_mut().monitors.reverse();
    let after = provider.snapshot().unwrap();
    assert_eq!(before.generation(), after.generation());
    assert_eq!(before.displays(), after.displays());
}
