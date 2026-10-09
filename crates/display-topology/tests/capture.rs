mod support;

use rpa_display_topology::*;
use support::*;

#[test]
fn mixed_dpi_left_display_has_explicit_regions_and_source_sizes() {
    let s = snapshot(vec![
        display("left", false, -100, 0, 100, 100, 2),
        display("main", true, 0, 0, 100, 100, 1),
    ]);
    let plan = CapturePlan::new(&s, Selection::Desktop, wide_budget()).unwrap();
    assert_eq!(
        plan.size(),
        PixelSize {
            width: 400,
            height: 200
        }
    );
    assert_eq!(
        plan.tiles()[0].composite_rect,
        PixelRect {
            x: 0,
            y: 0,
            width: 200,
            height: 200
        }
    );
    assert_eq!(
        plan.tiles()[1].composite_rect,
        PixelRect {
            x: 200,
            y: 0,
            width: 200,
            height: 200
        }
    );
    assert_eq!(
        plan.tiles()[0].source_size,
        PixelSize {
            width: 200,
            height: 200
        }
    );
    assert_eq!(
        plan.tiles()[1].source_size,
        PixelSize {
            width: 100,
            height: 100
        }
    );
    let m = ObservationMapping::new(
        &plan,
        PixelSize {
            width: 200,
            height: 100,
        },
        None,
    )
    .unwrap();
    assert_eq!(
        m.map_image(image(0.0, 0.0), s.generation()).unwrap(),
        native(-100, 0)
    );
    assert_eq!(
        m.map_image(image(99.999, 99.999), s.generation()).unwrap(),
        native(-1, 99)
    );
    assert_eq!(
        m.map_image(image(100.0, 0.0), s.generation()).unwrap(),
        native(0, 0)
    );
    assert_eq!(
        m.map_image(image(199.999, 99.999), s.generation()).unwrap(),
        native(99, 99)
    );
    let single = CapturePlan::new(&s, Selection::Display("left".into()), wide_budget()).unwrap();
    assert_eq!(
        single.size(),
        PixelSize {
            width: 200,
            height: 200
        }
    );
}

#[test]
fn pixel_budget_limits_both_single_and_composite_without_allocating() {
    let s = snapshot(vec![
        display("a", true, 0, 0, 100, 100, 2),
        display("b", false, 100, 0, 100, 100, 1),
    ]);
    let desktop = CapturePlan::new(&s, Selection::Desktop, budget(300, 300, 20_000)).unwrap();
    assert_eq!(
        desktop.size(),
        PixelSize {
            width: 200,
            height: 100
        }
    );
    let single = CapturePlan::new(&s, Selection::Primary, budget(300, 300, 10_000)).unwrap();
    assert_eq!(
        single.size(),
        PixelSize {
            width: 100,
            height: 100
        }
    );
    assert_eq!(
        CaptureBudget::new(0, 1, 1).unwrap_err(),
        Error::InvalidBudget
    );
    assert_eq!(
        CaptureBudget::new(1, 1, 0).unwrap_err(),
        Error::InvalidBudget
    );
    assert!(matches!(
        CaptureBudget::new(u64::MAX, 2, 1),
        Err(Error::ArithmeticOverflow { .. })
    ));
    assert_eq!(
        CaptureBudget::new(u32::MAX as u64 + 1, 1, 1).unwrap_err(),
        Error::BudgetDimensionsTooLarge
    );
}

#[test]
fn sparse_layout_and_quantized_zero_tiles_are_not_silently_accepted() {
    let s = snapshot(vec![
        display("a", true, i32::MIN, 0, 1, 1, 1),
        display("b", false, i32::MAX, 0, 1, 1, 1),
    ]);
    assert!(matches!(
        CapturePlan::new(&s, Selection::Desktop, budget(100, 100, 10_000)),
        Err(Error::CollapsedRegion { .. })
    ));
    let single = mapping(&s, Selection::Display("b".into()));
    assert_eq!(
        single
            .map_image(image(0.999, 0.999), s.generation())
            .unwrap(),
        native(i32::MAX, 0)
    );
    let s = snapshot(vec![
        display("a", true, 0, 0, 100, 100, 1),
        display("b", false, 100, 0, 1, 100, 1),
    ]);
    let plan = CapturePlan::new(&s, Selection::Desktop, wide_budget()).unwrap();
    assert!(matches!(
        ObservationMapping::new(
            &plan,
            PixelSize {
                width: 1,
                height: 1
            },
            None
        ),
        Err(Error::CollapsedRegion { .. })
    ));
}

#[test]
fn overlaps_and_mirrors_have_no_silent_primary_rule() {
    for x in [0, 50] {
        let s = snapshot(vec![
            display("a", true, 0, 0, 100, 100, 1),
            display("b", false, x, 0, 100, 100, 1),
        ]);
        assert_eq!(
            CapturePlan::new(&s, Selection::Desktop, wide_budget()).unwrap_err(),
            Error::OverlappingDisplays {
                first: "a".into(),
                second: "b".into()
            }
        );
        // An explicit surface remains unambiguous, even in a mirrored topology.
        assert_eq!(
            CapturePlan::new(&s, Selection::Display("b".into()), wide_budget())
                .unwrap()
                .tiles()
                .len(),
            1
        );
    }
}

#[test]
fn bounded_layout_preserves_spatial_gaps_or_returns_a_diagnostic() {
    let s = snapshot(vec![
        display("a", true, 0, 0, 100, 100, 1),
        display("b", false, 101, 0, 100, 100, 1),
    ]);
    let plan = CapturePlan::new(&s, Selection::Desktop, wide_budget()).unwrap();
    assert_eq!(
        plan.size(),
        PixelSize {
            width: 201,
            height: 100
        }
    );
    assert!(matches!(
        ObservationMapping::new(
            &plan,
            PixelSize {
                width: 99,
                height: 50
            },
            None
        ),
        Err(Error::CollapsedGap { .. })
    ));
    assert!(matches!(
        CapturePlan::new(&s, Selection::Desktop, budget(99, 50, 4950)),
        Err(Error::CollapsedGap { .. })
    ));
}

#[test]
fn source_dimensions_are_actual_facts_not_scale_times_native_bounds() {
    let mut d = display("a", true, 0, 0, 100, 100, 1);
    d.scale = 2.0;
    d.rotation = Rotation::Deg90;
    d.capture_size = PixelSize {
        width: 300,
        height: 200,
    };
    let s = snapshot(vec![d]);
    let plan = CapturePlan::new(&s, Selection::Primary, wide_budget()).unwrap();
    assert_eq!(
        plan.size(),
        PixelSize {
            width: 300,
            height: 200
        }
    );
    assert_eq!(
        plan.validate_source(
            "a",
            PixelSize {
                width: 300,
                height: 200
            }
        ),
        Ok(())
    );
    assert_eq!(
        plan.validate_source(
            "a",
            PixelSize {
                width: 200,
                height: 200
            }
        )
        .unwrap_err(),
        Error::SourceSizeMismatch {
            id: "a".into(),
            expected: PixelSize {
                width: 300,
                height: 200
            },
            actual: PixelSize {
                width: 200,
                height: 200
            }
        }
    );
    assert_eq!(
        plan.validate_source(
            "b",
            PixelSize {
                width: 300,
                height: 200
            }
        )
        .unwrap_err(),
        Error::DisplayNotSelected { id: "b".into() }
    );
    let m = ObservationMapping::new(&plan, plan.size(), None).unwrap();
    assert_eq!(
        m.map_image(image(150.0, 100.0), s.generation()).unwrap(),
        native(50, 50)
    );
}

#[test]
fn budget_dimensions_limit_long_and_short_axes_independently() {
    let s = snapshot(vec![
        display("a", true, 0, 0, 100, 100, 1),
        display("b", false, 100, 0, 100, 100, 1),
    ]);
    let plan = CapturePlan::new(&s, Selection::Desktop, budget(120, 20, 2400)).unwrap();
    assert_eq!(
        plan.size(),
        PixelSize {
            width: 40,
            height: 20
        }
    );
    assert_eq!(
        plan.tiles()[0].composite_rect,
        PixelRect {
            x: 0,
            y: 0,
            width: 20,
            height: 20
        }
    );
    assert_eq!(
        plan.tiles()[1].composite_rect,
        PixelRect {
            x: 20,
            y: 0,
            width: 20,
            height: 20
        }
    );
    let portrait = snapshot(vec![display("p", true, 0, 0, 10, 100, 1)]);
    assert_eq!(
        CapturePlan::new(&portrait, Selection::Primary, budget(2, 100, 200))
            .unwrap()
            .size(),
        PixelSize {
            width: 2,
            height: 20
        }
    );
}
