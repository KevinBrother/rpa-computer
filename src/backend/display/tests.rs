use super::*;
use test_support::*;

#[test]
fn real_adapter_composes_mixed_scale_vertical_negative_origins_with_gap() {
    let shared = facts();
    let mut p = DisplayProvider::new(MemoryFrames(shared.clone())).unwrap();
    let surface = p.select(&Selection::Desktop).unwrap();
    assert_eq!(surface.surface_id, "desktop");
    assert_eq!(surface.input_origin, (-100, 0));
    assert_eq!(surface.input_size, (200, 150));
    let c = p.capture(budget::requested(400, 300).unwrap()).unwrap();
    assert_eq!(
        c.mapping.size(),
        PixelSize {
            width: 400,
            height: 300
        }
    );
    let image = image::load_from_memory(&c.png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (400, 300));
    assert_eq!(image.get_pixel(100, 100).0, [230, 10, 20, 255]);
    assert_eq!(image.get_pixel(340, 180).0, [20, 40, 240, 255]);
    assert_eq!(image.get_pixel(240, 100).0, [0, 0, 0, 255]);
    assert_eq!(shared.borrow().captures, 2);
    let gen = c.mapping.generation();
    assert_eq!(
        c.mapping
            .map_image(ImagePoint { x: 100.0, y: 100.0 }, gen)
            .unwrap(),
        NativePoint { x: -50, y: 50 }
    );
    assert_eq!(
        c.mapping
            .map_image(ImagePoint { x: 340.0, y: 180.0 }, gen)
            .unwrap(),
        NativePoint { x: 70, y: 90 }
    );
    assert_eq!(
        c.mapping
            .map_image(ImagePoint { x: 399.0, y: 299.0 }, gen)
            .unwrap(),
        NativePoint { x: 99, y: 149 }
    );
    for (x, y) in [(240.0, 100.0), (220.0, 10.0), (400.0, 100.0), (-1.0, 0.0)] {
        assert!(c.mapping.map_image(ImagePoint { x, y }, gen).is_err());
    }
}
#[test]
fn exact_id_missing_is_error_and_does_not_change_previous_selection() {
    let mut p = DisplayProvider::new(MemoryFrames(facts())).unwrap();
    let selected = p.select(&Selection::Display("right".into())).unwrap();
    assert_eq!(selected.surface_id, "right");
    assert_eq!(
        p.select(&Selection::Display("absent".into()))
            .unwrap_err()
            .code,
        "display_not_found"
    );
    assert_eq!(p.geometry().unwrap(), selected);
}
#[test]
fn fresh_generation_uses_full_identity_and_queries_never_fall_back() {
    let shared = facts();
    let mut p = DisplayProvider::new(MemoryFrames(shared.clone())).unwrap();
    let before = p.snapshot().unwrap();
    let repeated = p.snapshot().unwrap();
    assert_eq!(before.generation(), repeated.generation());
    let independent = DisplayProvider::new(MemoryFrames(shared.clone()))
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(
        before.generation().revision(),
        independent.generation().revision()
    );
    assert_ne!(before.generation(), independent.generation());
    shared.borrow_mut().query_fail = true;
    assert!(p.snapshot().is_err());
    assert!(p.capture(budget::requested(400, 300).unwrap()).is_err());
    assert_eq!(shared.borrow().captures, 0);
}
#[test]
fn source_budget_rejects_before_any_provider_capture() {
    let shared = facts();
    shared.borrow_mut().displays[0].capture_size = PixelSize {
        width: 8192,
        height: 8192,
    };
    let mut p = DisplayProvider::new(MemoryFrames(shared.clone())).unwrap();
    assert!(p.capture(budget::requested(64, 64).unwrap()).is_err());
    assert_eq!(shared.borrow().captures, 0);
}
#[test]
fn output_budget_shrinks_real_plan_not_just_metadata() {
    let requested = budget::requested(4096, 4096).unwrap();
    assert!(requested.max_pixels() < 4096 * 4096);
    // Validate worst admitted shapes without allocating giant screenshots.
    for size in [
        PixelSize {
            width: 4096,
            height: requested.max_pixels() as u32 / 4096,
        },
        PixelSize {
            width: 1,
            height: 4096,
        },
    ] {
        let estimate = rpa_windows_display::estimate_encoded_size(size).unwrap();
        assert!(estimate.png_bytes <= budget::MAX_PNG_BYTES as u64);
        assert!(
            estimate.base64_bytes + 8 * 1024 * 1024
                < crate::mcp::jsonrpc::MAX_OUT_LINE_BYTES as u64
        );
    }
    let shared = facts();
    let mut p = DisplayProvider::new(MemoryFrames(shared)).unwrap();
    p.select(&Selection::Desktop).unwrap();
    let c = p.capture(budget::requested(200, 150).unwrap()).unwrap();
    assert_eq!(image::load_from_memory(&c.png).unwrap().width(), 200);
    assert_eq!(
        c.mapping.regions()[1].image_rect,
        PixelRect {
            x: 140,
            y: 30,
            width: 60,
            height: 120
        }
    );
    assert!(budget::check_png(budget::MAX_PNG_BYTES + 1).is_err());
}
#[test]
fn capture_failure_or_post_capture_topology_change_never_returns_pixels() {
    for change in [false, true] {
        let shared = facts();
        shared.borrow_mut().capture_fail = !change;
        shared.borrow_mut().change_during_capture = change;
        let mut p = DisplayProvider::new(MemoryFrames(shared)).unwrap();
        assert!(p.capture(budget::requested(400, 300).unwrap()).is_err());
    }
}
