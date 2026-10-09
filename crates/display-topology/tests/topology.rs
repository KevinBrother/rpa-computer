mod support;

use rpa_display_topology::*;
use support::*;

#[test]
fn explicit_selection_never_falls_back() {
    let s = snapshot(vec![display("main", true, 0, 0, 100, 100, 1)]);
    assert_eq!(
        CapturePlan::new(&s, Selection::Display("gone".into()), wide_budget()).unwrap_err(),
        Error::DisplayNotFound { id: "gone".into() }
    );
    assert_eq!(
        CapturePlan::new(&s, Selection::Primary, wide_budget())
            .unwrap()
            .tiles()[0]
            .display_id,
        "main"
    );
}

#[test]
fn descriptor_errors_are_specific() {
    let mut t = TopologyTracker::new().unwrap();
    assert_eq!(
        t.update(NativeUnit::Points, vec![]).unwrap_err(),
        Error::EmptyTopology
    );
    let d = display("a", true, 0, 0, 100, 100, 1);
    assert_eq!(
        t.update(NativeUnit::Points, vec![d.clone(), d.clone()])
            .unwrap_err(),
        Error::DuplicateDisplayId { id: "a".into() }
    );
    let mut invalid = d.clone();
    invalid.is_primary = false;
    assert_eq!(
        t.update(NativeUnit::Points, vec![invalid]).unwrap_err(),
        Error::NoPrimaryDisplay
    );
    let mut second = d.clone();
    second.id = "b".into();
    second.native_bounds.x = 100;
    assert!(matches!(
        t.update(NativeUnit::Points, vec![d.clone(), second]),
        Err(Error::MultiplePrimaryDisplays { .. })
    ));
    for scale in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let mut invalid = d.clone();
        invalid.scale = scale;
        assert!(matches!(
            t.update(NativeUnit::Points, vec![invalid]),
            Err(Error::InvalidScale { .. })
        ));
    }
    let mut invalid = d.clone();
    invalid.native_bounds.width = 0;
    assert!(matches!(
        t.update(NativeUnit::Points, vec![invalid]),
        Err(Error::InvalidNativeSize { .. })
    ));
    let mut invalid = d.clone();
    invalid.capture_size.height = 0;
    assert!(matches!(
        t.update(NativeUnit::Points, vec![invalid]),
        Err(Error::InvalidCaptureSize { .. })
    ));
    let mut invalid = d.clone();
    invalid.native_bounds.x = i32::MAX;
    assert!(matches!(
        t.update(NativeUnit::Points, vec![invalid]),
        Err(Error::NativeBoundsOverflow { .. })
    ));
    let mut invalid = d;
    invalid.id.clear();
    assert_eq!(
        t.update(NativeUnit::Points, vec![invalid]).unwrap_err(),
        Error::EmptyDisplayId
    );
}

#[test]
fn generation_is_exact_order_independent_and_transactional() {
    let a = display("a", true, 0, 0, 100, 100, 1);
    let b = display("b", false, -100, 0, 100, 100, 2);
    let mut t = TopologyTracker::new().unwrap();
    let first = t
        .update(NativeUnit::Points, vec![b.clone(), a.clone()])
        .unwrap();
    assert_eq!(first.displays()[0].id, "a");
    assert_eq!(first.generation().revision(), 1);
    assert_eq!(
        t.update(NativeUnit::Points, vec![a.clone(), b.clone()])
            .unwrap()
            .generation(),
        first.generation()
    );
    assert_eq!(
        t.update(NativeUnit::Points, vec![]).unwrap_err(),
        Error::EmptyTopology
    );
    assert_eq!(
        t.update(NativeUnit::Points, vec![a.clone(), b.clone()])
            .unwrap()
            .generation(),
        first.generation()
    );
    let variants = [
        {
            let mut d = b.clone();
            d.native_bounds.x -= 1;
            d
        },
        {
            let mut d = b.clone();
            d.native_bounds.width -= 1;
            d
        },
        {
            let mut d = b.clone();
            d.native_bounds.height -= 1;
            d
        },
        {
            let mut d = b.clone();
            d.native_bounds.y += 1;
            d
        },
        {
            let mut d = b.clone();
            d.capture_size.width += 1;
            d
        },
        {
            let mut d = b.clone();
            d.capture_size.height += 1;
            d
        },
        {
            let mut d = b.clone();
            d.rotation = Rotation::Deg90;
            d
        },
        {
            let mut d = b.clone();
            d.scale = f64::from_bits(2.0f64.to_bits() + 1);
            d
        },
        {
            let mut d = b.clone();
            d.id = "replugged".into();
            d
        },
    ];
    for (i, changed) in variants.into_iter().enumerate() {
        let s = t
            .update(NativeUnit::Points, vec![a.clone(), changed])
            .unwrap();
        assert_eq!(s.generation().revision(), i as u64 + 2);
    }
    let mut a2 = a.clone();
    a2.is_primary = false;
    let mut b2 = b.clone();
    b2.is_primary = true;
    assert_eq!(
        t.update(NativeUnit::Points, vec![a2, b2])
            .unwrap()
            .generation()
            .revision(),
        11
    );
    assert_eq!(
        t.update(NativeUnit::Points, vec![a.clone()])
            .unwrap()
            .generation()
            .revision(),
        12
    );
    assert_eq!(
        t.update(NativeUnit::PhysicalPixels, vec![a.clone(), b.clone()])
            .unwrap()
            .generation()
            .revision(),
        13
    );
    assert_eq!(
        t.update(NativeUnit::Points, vec![a, b])
            .unwrap()
            .generation()
            .revision(),
        14
    );
}

#[test]
fn tracker_lifetimes_do_not_alias_generation_one() {
    let a = snapshot(vec![display("a", true, 0, 0, 10, 10, 1)]);
    let b = snapshot(vec![display("a", true, 0, 0, 10, 10, 1)]);
    assert_ne!(a.generation(), b.generation());
    assert!(matches!(
        mapping(&a, Selection::Primary).map_image(image(0.0, 0.0), b.generation()),
        Err(Error::StaleGeneration { .. })
    ));
}
