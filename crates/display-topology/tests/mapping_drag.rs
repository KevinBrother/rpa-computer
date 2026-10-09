mod support;

use rpa_display_topology::*;
use support::*;

#[test]
fn upper_display_gap_and_padding_remain_unclickable() {
    let s = snapshot(vec![
        display("upper", false, 0, -120, 100, 100, 1),
        display("main", true, 0, 0, 100, 100, 1),
    ]);
    let plan = CapturePlan::new(&s, Selection::Desktop, wide_budget()).unwrap();
    assert_eq!(
        plan.size(),
        PixelSize {
            width: 100,
            height: 220
        }
    );
    let m = ObservationMapping::new(
        &plan,
        PixelSize {
            width: 120,
            height: 240,
        },
        Some(PixelRect {
            x: 10,
            y: 10,
            width: 100,
            height: 220,
        }),
    )
    .unwrap();
    assert_eq!(
        m.map_image(image(10.0, 10.0), s.generation()).unwrap(),
        native(0, -120)
    );
    assert_eq!(
        m.map_image(image(10.0, 109.999), s.generation()).unwrap(),
        native(0, -21)
    );
    for p in [
        image(10.0, 110.0),
        image(10.0, 129.999),
        image(0.0, 0.0),
        image(119.0, 239.0),
    ] {
        assert!(matches!(
            m.map_image(p, s.generation()),
            Err(Error::ImageGapOrPadding { .. })
        ));
    }
    assert_eq!(
        m.map_image(image(10.0, 130.0), s.generation()).unwrap(),
        native(0, 0)
    );
    assert!(matches!(
        m.map_native(native(0, -10), s.generation()),
        Err(Error::NativePointOutsideSelection { .. })
    ));
}

#[test]
fn selection_restricts_native_coords_and_image_requests_are_not_clamped() {
    let s = snapshot(vec![
        display("left", false, -100, 0, 100, 100, 1),
        display("main", true, 0, 0, 100, 100, 1),
    ]);
    let m = mapping(&s, Selection::Primary);
    assert!(matches!(
        m.map_native(native(-1, 0), s.generation()),
        Err(Error::NativePointOutsideSelection { .. })
    ));
    for p in [
        image(-0.1, 0.0),
        image(100.0, 0.0),
        image(0.0, 100.0),
        image(f64::NAN, 0.0),
        image(0.0, f64::INFINITY),
    ] {
        assert!(matches!(
            m.map_image(p, s.generation()),
            Err(Error::InvalidImagePoint { .. })
        ));
    }
    assert_eq!(
        m.map_image(image(99.999999999, 99.999999999), s.generation())
            .unwrap(),
        native(99, 99)
    );
}

#[test]
fn stale_observation_and_drag_stop_after_unplug() {
    let mut t = TopologyTracker::new().unwrap();
    let a = display("a", true, 0, 0, 100, 100, 1);
    let b = display("b", false, 200, 0, 100, 100, 1);
    let old = t.update(NativeUnit::Points, vec![a.clone(), b]).unwrap();
    let plan = CapturePlan::new(&old, Selection::Desktop, wide_budget()).unwrap();
    assert_eq!(plan.require_generation(old.generation()), Ok(()));
    let m = mapping(&old, Selection::Desktop);
    let drag =
        DragPlan::from_native(&m, &[native(99, 50), native(200, 50)], old.generation()).unwrap();
    let new = t.update(NativeUnit::Points, vec![a]).unwrap();
    assert!(matches!(
        plan.require_generation(new.generation()),
        Err(Error::StaleGeneration { .. })
    ));
    assert!(matches!(
        m.map_image(image(0.0, 0.0), new.generation()),
        Err(Error::StaleGeneration { .. })
    ));
    assert!(matches!(
        m.map_native(native(0, 0), new.generation()),
        Err(Error::StaleGeneration { .. })
    ));
    assert!(matches!(
        drag.sample(0, 1, 2, new.generation()),
        Err(Error::StaleGeneration { .. })
    ));
    assert_eq!(
        CapturePlan::new(&new, Selection::Display("b".into()), wide_budget()).unwrap_err(),
        Error::DisplayNotFound { id: "b".into() }
    );
}

#[test]
fn cross_screen_drag_can_interpolate_gap_but_explicit_gap_waypoint_is_rejected() {
    let s = snapshot(vec![
        display("a", true, -100, 0, 100, 100, 1),
        display("b", false, 100, 0, 100, 100, 1),
    ]);
    let m = mapping(&s, Selection::Desktop);
    let drag =
        DragPlan::from_native(&m, &[native(-50, 50), native(150, 50)], s.generation()).unwrap();
    assert_eq!(
        drag.segments(),
        &[NativeSegment {
            start: native(-50, 50),
            end: native(150, 50)
        }]
    );
    assert_eq!(
        drag.sample(0, 0, 2, s.generation()).unwrap(),
        native(-50, 50)
    );
    assert_eq!(
        drag.sample(0, 1, 2, s.generation()).unwrap(),
        native(50, 50)
    );
    assert_eq!(
        drag.sample(0, 2, 2, s.generation()).unwrap(),
        native(150, 50)
    );
    assert!(matches!(
        m.map_native(native(50, 50), s.generation()),
        Err(Error::NativePointOutsideSelection { .. })
    ));
    assert!(matches!(
        DragPlan::from_native(
            &m,
            &[native(-50, 50), native(50, 50), native(150, 50)],
            s.generation()
        ),
        Err(Error::NativePointOutsideSelection { .. })
    ));
    assert!(matches!(
        DragPlan::from_image(
            &m,
            &[image(50.0, 50.0), image(150.0, 50.0), image(250.0, 50.0)],
            s.generation()
        ),
        Err(Error::ImageGapOrPadding { .. })
    ));
    assert_eq!(
        drag.sample(0, 3, 2, s.generation()).unwrap_err(),
        Error::InvalidInterpolation { step: 3, steps: 2 }
    );
    assert!(matches!(
        drag.sample(0, 0, 0, s.generation()),
        Err(Error::InvalidInterpolation { .. })
    ));
    assert_eq!(
        drag.sample(1, 0, 2, s.generation()).unwrap_err(),
        Error::SegmentNotFound { index: 1 }
    );
    assert_eq!(
        DragPlan::from_native(&m, &[native(-50, 50)], s.generation()).unwrap_err(),
        Error::TooFewWaypoints
    );
}

#[test]
fn extreme_origins_and_sizes_do_not_overflow_or_round_outside() {
    let s = snapshot(vec![DisplayDescriptor {
        id: "huge".into(),
        is_primary: true,
        native_bounds: NativeRect {
            x: i32::MIN,
            y: -1,
            width: u32::MAX,
            height: 2,
        },
        capture_size: PixelSize {
            width: u32::MAX,
            height: 2,
        },
        scale: 1.0,
        rotation: Rotation::Deg0,
    }]);
    let plan =
        CapturePlan::new(&s, Selection::Primary, budget(u32::MAX as u64, 2, u64::MAX)).unwrap();
    assert_eq!(
        plan.size(),
        PixelSize {
            width: u32::MAX,
            height: 2
        }
    );
    let m = ObservationMapping::new(&plan, plan.size(), None).unwrap();
    assert_eq!(
        m.map_image(image(0.0, 0.0), s.generation()).unwrap(),
        native(i32::MIN, -1)
    );
    assert_eq!(
        m.map_image(image(u32::MAX as f64 - 0.001, 1.999), s.generation())
            .unwrap(),
        native(i32::MAX - 1, 0)
    );
}

#[test]
fn invalid_observation_content_rect_is_rejected() {
    let s = snapshot(vec![display("a", true, 0, 0, 10, 10, 1)]);
    let plan = CapturePlan::new(&s, Selection::Primary, wide_budget()).unwrap();
    assert_eq!(
        ObservationMapping::new(
            &plan,
            PixelSize {
                width: 0,
                height: 10
            },
            None
        )
        .unwrap_err(),
        Error::InvalidObservationSize
    );
    assert_eq!(
        ObservationMapping::new(
            &plan,
            PixelSize {
                width: 10,
                height: 10
            },
            Some(PixelRect {
                x: u32::MAX,
                y: 0,
                width: 1,
                height: 1
            })
        )
        .unwrap_err(),
        Error::InvalidContentRect
    );
    assert_eq!(
        ObservationMapping::new(
            &plan,
            PixelSize {
                width: 10,
                height: 10
            },
            Some(PixelRect {
                x: 1,
                y: 0,
                width: 10,
                height: 10
            })
        )
        .unwrap_err(),
        Error::InvalidContentRect
    );
}

#[test]
fn right_and_below_layouts_bound_each_tile_and_reject_l_shaped_holes() {
    let s = snapshot(vec![
        display("a", true, 0, 0, 100, 100, 1),
        display("b", false, 100, 100, 100, 100, 1),
    ]);
    let m = mapping(&s, Selection::Desktop);
    assert_eq!(
        m.size(),
        PixelSize {
            width: 200,
            height: 200
        }
    );
    assert_eq!(
        m.map_image(image(199.999, 199.999), s.generation())
            .unwrap(),
        native(199, 199)
    );
    assert_eq!(
        m.map_image(image(100.0, 100.0), s.generation()).unwrap(),
        native(100, 100)
    );
    for p in [image(150.0, 50.0), image(50.0, 150.0)] {
        assert!(matches!(
            m.map_image(p, s.generation()),
            Err(Error::ImageGapOrPadding { .. })
        ));
    }
    assert_eq!(
        m.map_image(image(99.999, 99.999), s.generation()).unwrap(),
        native(99, 99)
    );
}

#[test]
fn interpolation_uses_wide_checked_arithmetic_and_exact_endpoints() {
    let s = snapshot(vec![DisplayDescriptor {
        id: "a".into(),
        is_primary: true,
        native_bounds: NativeRect {
            x: i32::MIN,
            y: i32::MIN,
            width: u32::MAX,
            height: u32::MAX,
        },
        capture_size: PixelSize {
            width: 10,
            height: 10,
        },
        scale: 1.0,
        rotation: Rotation::Deg0,
    }]);
    let m = mapping(&s, Selection::Primary);
    let drag = DragPlan::from_native(
        &m,
        &[
            native(i32::MIN, i32::MAX - 1),
            native(i32::MAX - 1, i32::MIN),
        ],
        s.generation(),
    )
    .unwrap();
    assert_eq!(
        drag.sample(0, 1, 2, s.generation()).unwrap(),
        native(-1, -1)
    );
    assert_eq!(
        drag.sample(0, u32::MAX, u32::MAX, s.generation()).unwrap(),
        native(i32::MAX - 1, i32::MIN)
    );
    assert_eq!(
        drag.sample(0, 0, u32::MAX, s.generation()).unwrap(),
        native(i32::MIN, i32::MAX - 1)
    );
}

#[test]
fn legitimate_image_drag_crosses_gap_without_authorizing_gap_clicks() {
    let s = snapshot(vec![
        display("a", true, -100, 0, 100, 100, 1),
        display("b", false, 100, 0, 100, 100, 1),
    ]);
    let m = mapping(&s, Selection::Desktop);
    let d =
        DragPlan::from_image(&m, &[image(50.0, 50.0), image(250.0, 50.0)], s.generation()).unwrap();
    assert_eq!(d.sample(0, 1, 2, s.generation()).unwrap(), native(50, 50));
    assert!(matches!(
        m.map_image(image(150.0, 50.0), s.generation()),
        Err(Error::ImageGapOrPadding { .. })
    ));
}

#[test]
fn half_open_mapping_cannot_round_to_the_next_display() {
    let s = snapshot(vec![
        display("a", true, -3, -3, 3, 3, 1),
        display("b", false, 0, -3, 3, 3, 1),
    ]);
    let plan = CapturePlan::new(&s, Selection::Desktop, wide_budget()).unwrap();
    let m = ObservationMapping::new(
        &plan,
        PixelSize {
            width: 4,
            height: 2,
        },
        None,
    )
    .unwrap();
    assert_eq!(
        m.regions()[0].image_rect,
        PixelRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2
        }
    );
    assert_eq!(
        m.map_image(image(1.999999, 1.999999), s.generation())
            .unwrap(),
        native(-1, -1)
    );
    assert_eq!(
        m.map_image(image(2.0, 0.0), s.generation()).unwrap(),
        native(0, -3)
    );
    assert_eq!(
        m.map_image(image(3.999999, 1.999999), s.generation())
            .unwrap(),
        native(2, -1)
    );
}
