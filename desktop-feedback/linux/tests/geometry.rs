mod common;
use common::snapshot;
use rpa_desktop_feedback::protocol::{Pointer, PointerKind};
use rpa_desktop_feedback_linux::geometry::{layout, resolve, GeometryError, Monitor, Rect};

fn monitor(key: &str, x: i32, y: i32, width: u32, height: u32) -> Monitor {
    let bounds = Rect {
        x,
        y,
        width,
        height,
    };
    Monitor {
        key: key.into(),
        bounds,
        workarea: bounds,
    }
}
fn pointer(x: f64, y: f64) -> Pointer {
    Pointer {
        x,
        y,
        kind: PointerKind::Move,
    }
}

#[test]
fn exact_live_geometry_required_not_screen_id_or_dpi_guess() {
    let surface = snapshot(1).surface.unwrap();
    let screens = [monitor("native-a", 0, 0, 1920, 1080)];
    let result = resolve(&surface, Some(&pointer(100., 200.)), &screens).unwrap();
    assert_eq!(result.anchor, 0);
    assert_eq!(result.ring_center, Some((100, 200)));
    let mut wrong = surface.clone();
    wrong.width = 1919.;
    assert_eq!(
        resolve(&wrong, None, &screens),
        Err(GeometryError::SurfaceMismatch)
    );
    wrong.width = 1920.25;
    assert!(resolve(&wrong, None, &screens).is_err());
}

#[test]
fn desktop_gap_suppresses_ring_and_anchor_is_independent_of_pointer() {
    let screens = [
        monitor("a", 0, 0, 800, 600),
        monitor("b", 1000, 200, 800, 600),
    ];
    let mut surface = snapshot(1).surface.unwrap();
    surface.id = "desktop".into();
    surface.width = 1800.;
    surface.height = 800.;
    for (p, expected) in [
        (pointer(900., 300.), None),
        (pointer(1100., 300.), Some((1100, 300))),
        (pointer(1800., 300.), None),
    ] {
        let result = resolve(&surface, Some(&p), &screens).unwrap();
        assert_eq!(screens[result.anchor].key, "a");
        assert_eq!(result.ring_center, expected);
    }
    let reversed = [screens[1].clone(), screens[0].clone()];
    let result = resolve(&surface, None, &reversed).unwrap();
    assert_eq!(reversed[result.anchor].key, "a");
}

#[test]
fn fractional_pointer_near_edge_cannot_round_into_gap() {
    let screens = [monitor("a", 0, 0, 800, 600)];
    let mut surface = snapshot(1).surface.unwrap();
    surface.width = 800.;
    surface.height = 600.;
    let result = resolve(&surface, Some(&pointer(799.75, 599.75)), &screens).unwrap();
    assert_eq!(result.ring_center, Some((799, 599)));
    for p in [
        pointer(800., 100.),
        pointer(-0.1, 100.),
        pointer(f64::NAN, 100.),
    ] {
        assert!(resolve(&surface, Some(&p), &screens)
            .unwrap()
            .ring_center
            .is_none());
    }
}

#[test]
fn overlapping_or_duplicate_monitor_facts_are_rejected_not_guessed() {
    let mut surface = snapshot(1).surface.unwrap();
    surface.id = "desktop".into();
    for screens in [
        vec![
            monitor("a", 0, 0, 1920, 1080),
            monitor("b", 0, 0, 1920, 1080),
        ],
        vec![
            monitor("a", 0, 0, 960, 1080),
            monitor("a", 960, 0, 960, 1080),
        ],
        vec![],
    ] {
        assert!(resolve(&surface, None, &screens).is_err());
    }
}

#[test]
fn stop_hit_target_scales_but_native_pointer_coordinates_do_not() {
    let screen = monitor("a", 120, 80, 1920, 1080);
    for scale in [1., 1.25, 2.] {
        let result = layout(&screen, scale).unwrap();
        assert_eq!(result.stop.width, (88.0_f64 * scale).ceil() as u32);
        assert_eq!(result.stop.height, (48.0_f64 * scale).ceil() as u32);
        assert_eq!(result.bar.x, 120 + (16.0_f64 * scale).ceil() as i32);
        assert_eq!(result.stop.y, result.bar.y);
        assert!(result.stop.x > result.bar.x + result.bar.width as i32);
    }
}

#[test]
fn small_workarea_and_invalid_scale_fail_instead_of_shrinking_stop() {
    let screen = monitor("a", 0, 0, 180, 60);
    assert_eq!(layout(&screen, 1.), Err(GeometryError::WorkareaTooSmall));
    let large = monitor("a", 0, 0, 1920, 1080);
    for scale in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(layout(&large, scale), Err(GeometryError::InvalidScale));
    }
}

#[test]
fn invalid_workarea_and_overflowing_native_bounds_fail_before_narrowing() {
    let mut screen = monitor("a", 0, 0, 1920, 1080);
    screen.workarea.x = -1;
    assert!(layout(&screen, 1.).is_err());
    for invalid in [
        monitor("a", i32::MAX, 0, u32::MAX, 1080),
        monitor("a", -1, 0, 1920, 1080),
        monitor("a", 0, 0, 0, 1080),
    ] {
        assert!(layout(&invalid, 1.).is_err());
    }
}
