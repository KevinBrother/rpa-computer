use super::{
    fixture::{Fixture, LEFT, RIGHT},
    support::*,
    X11Provider,
};
use crate::backend::display::DisplayProvider;
use rpa_display_topology::{DisplayDescriptor, Error, ImagePoint, NativeRect, Selection};
use rpa_windows_display::{DisplayBackend, DisplayError, FrameProvider, RgbaFrame, SourcePermit};
use std::{cell::RefCell, rc::Rc};

const DESKTOP: NativeRect = NativeRect {
    x: 32,
    y: 24,
    width: 448,
    height: 200,
};

#[test]
#[ignore = "CC-only: isolated Xvfb MUST expose owned RandR 1.5 logical monitors; unsupported is UNAVAILABLE, not pass"]
fn nonzero_origin_primary_id_desktop_pixels_and_gap() {
    let mut fixture = Fixture::connect();
    fixture.split();
    let left = fixture.paint(LEFT, false);
    let right = fixture.paint(RIGHT, true);
    let native = X11Provider::attach().unwrap();
    let probe = native.capture_probe();
    let mut provider = DisplayProvider::new(native).unwrap();
    let snapshot = provider.snapshot().unwrap();
    assert_eq!(snapshot.displays().len(), 2);
    let primary = snapshot.displays().iter().find(|d| d.is_primary).unwrap();
    let secondary = snapshot.displays().iter().find(|d| !d.is_primary).unwrap();
    assert_eq!(primary.native_bounds, LEFT);
    assert_eq!(secondary.native_bounds, RIGHT);
    for (selection, bounds, paint) in [
        (Selection::Primary, LEFT, left),
        (Selection::Display(primary.id.clone()), LEFT, left),
        (Selection::Display(secondary.id.clone()), RIGHT, right),
    ] {
        provider.select(&selection).unwrap();
        let capture = provider.capture(exact(bounds)).unwrap();
        assert_paint(
            &decode(&capture.png, bounds),
            &capture.mapping,
            bounds,
            paint,
        );
        assert_eq!(capture.mapping.regions().len(), 1);
        assert_eq!(capture.mapping.regions()[0].native_bounds, bounds);
        assert_eq!(capture.topology.generation(), snapshot.generation());
        assert_eq!(
            probe.read().last_bounds,
            Some(bounds),
            "native request must read exact nonzero root region, not full root"
        );
        assert_eq!(probe.read().last_permit_bytes, bytes(bounds));
    }
    provider.select(&Selection::Desktop).unwrap();
    let capture = provider.capture(exact(DESKTOP)).unwrap();
    let png = decode(&capture.png, DESKTOP);
    assert_paint(&png, &capture.mapping, DESKTOP, left);
    assert_paint(&png, &capture.mapping, DESKTOP, right);
    assert_eq!(capture.mapping.regions().len(), 2);
    // Actual desktop gap is root x=232..280. Output x=210 lies in it.
    assert_eq!(png.get_pixel(210, 80).0, [0, 0, 0, 255]);
    assert!(matches!(
        capture.mapping.map_image(
            ImagePoint { x: 210.0, y: 80.0 },
            capture.topology.generation()
        ),
        Err(Error::ImageGapOrPadding { .. })
    ));
    let reads = probe.read();
    assert_eq!(
        (reads.requests, reads.replies, reads.decode_entries),
        (5, 5, 5)
    );
    assert_eq!(reads.decoded_rgba_bytes, 5 * bytes(LEFT));
    eprintln!("native nonzero-origin exact regions/desktop: {reads:?}");
}

/// Deterministic real server mutation after engine enumeration but before native
/// revalidation/GetImage. No fake snapshot, frame, native reply, or error.
struct MoveBeforeRead {
    native: X11Provider,
    fixture: Rc<RefCell<Fixture>>,
    move_once: bool,
}
impl FrameProvider for MoveBeforeRead {
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        self.native.enumerate()
    }
    fn capture(
        &mut self,
        display: &DisplayDescriptor,
        permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        if self.move_once {
            self.move_once = false;
            self.fixture.borrow_mut().move_right();
        }
        self.native.capture(display, permit)
    }
}

#[test]
#[ignore = "CC-only: isolated Xvfb MUST support owned RandR topology mutation; unsupported is UNAVAILABLE, not pass"]
fn topology_requery_invalidates_mapping_and_rejects_stale_native_read() {
    let fixture = Rc::new(RefCell::new(Fixture::connect()));
    fixture.borrow_mut().split();
    let painted = fixture.borrow_mut().paint(LEFT, false);
    let native = X11Provider::attach().unwrap();
    let probe = native.capture_probe();
    let mut baseline = DisplayBackend::new(native, generous()).unwrap();
    let before = baseline.observe(Selection::Primary, exact(LEFT)).unwrap();
    assert_paint(&decode(&before.png, LEFT), &before.mapping, LEFT, painted);

    // Separate provider is still real. Establish its native identity first,
    // then exercise the exact race between engine plan and native capture.
    let racing = X11Provider::attach().unwrap();
    let racing_probe = racing.capture_probe();
    let mut engine = DisplayBackend::new(
        MoveBeforeRead {
            native: racing,
            fixture: fixture.clone(),
            move_once: true,
        },
        generous(),
    )
    .unwrap();
    let original = engine.snapshot().unwrap();
    let error = engine.observe(Selection::Primary, exact(LEFT)).unwrap_err();
    assert!(
        matches!(error, DisplayError::SourceChanged { .. }),
        "must reject even though selected primary is unchanged: {error:?}"
    );
    assert_eq!(
        crate::backend::display::provider_error(error).code,
        "geometry_changed"
    );
    assert_eq!(
        (
            racing_probe.read().requests,
            racing_probe.read().replies,
            racing_probe.read().decode_entries
        ),
        (0, 0, 0)
    );
    let changed = baseline.snapshot().unwrap();
    // This provider never observed the delete/recreate gap or a failed capture.
    // Same atom/output membership must retain BOTH attachment IDs; generation
    // invalidation must come from changed geometry, not a newly named display.
    assert_eq!(before.topology.displays().len(), 2);
    assert_eq!(changed.displays().len(), 2);
    let moved_right = NativeRect {
        x: RIGHT.x + 8,
        ..RIGHT
    };
    for (old_bounds, new_bounds) in [(LEFT, LEFT), (RIGHT, moved_right)] {
        let old = before
            .topology
            .displays()
            .iter()
            .find(|display| display.native_bounds == old_bounds)
            .expect("original owned monitor");
        let current = changed
            .displays()
            .iter()
            .find(|display| display.id == old.id)
            .expect("uninterrupted provider must preserve original display ID");
        let mut expected = old.clone();
        expected.native_bounds = new_bounds;
        assert_eq!(current, &expected, "only secondary origin may change");
        eprintln!(
            "uninterrupted provider retained id={} bounds={old_bounds:?}->{new_bounds:?}",
            old.id
        );
    }
    assert_ne!(changed.generation(), before.topology.generation());
    assert!(matches!(
        before
            .mapping
            .map_image(ImagePoint { x: 10.0, y: 10.0 }, changed.generation()),
        Err(Error::StaleGeneration { .. })
    ));
    assert_eq!(
        probe.read().requests,
        1,
        "topology requery itself must not read image pixels"
    );
    let after = engine.observe(Selection::Primary, exact(LEFT)).unwrap();
    assert_ne!(original.generation(), after.topology.generation());
    assert!(after
        .topology
        .displays()
        .iter()
        .any(|d| d.native_bounds.x == RIGHT.x + 8));
    assert_paint(&decode(&after.png, LEFT), &after.mapping, LEFT, painted);
    assert_eq!(
        (
            racing_probe.read().requests,
            racing_probe.read().replies,
            racing_probe.read().decode_entries
        ),
        (1, 1, 1),
        "fresh observation must recover after invalidation"
    );
    eprintln!(
        "native stale request rejected before GetImage; recovery={:?}",
        racing_probe.read()
    );
}
