//! Ignored, opt-in REAL-X11 checks. See README.md; CC executes these only on
//! its exclusively owned disposable Xvfb. Ordinary test runs never connect.
mod fixture;
mod support;
mod topology;

use super::super::X11Provider;
use crate::backend::display::DisplayProvider;
use fixture::Fixture;
use rpa_display_topology::{NativeRect, NativeUnit, Selection};
use rpa_windows_display::{DisplayBackend, DisplayError, MemoryBudget};
use std::{cell::RefCell, rc::Rc};
use support::*;

#[test]
#[ignore = "CC-only: explicit isolated disposable Xvfb required; see native_x11/README.md"]
fn root_pixels_png_and_single_monitor_selections() {
    let mut fixture = Fixture::connect();
    fixture.require_no_primary();
    let bounds = NativeRect {
        x: 0,
        y: 0,
        width: fixture.root_size.0,
        height: fixture.root_size.1,
    };
    let painted = fixture.paint(bounds, false);
    let native = X11Provider::attach().expect("real provider attach");
    let probe = native.capture_probe();
    let mut provider = DisplayProvider::new(native).unwrap();
    let snapshot = provider.snapshot().unwrap();
    assert_eq!(snapshot.displays().len(), 1);
    let display = &snapshot.displays()[0];
    assert!(
        display.is_primary,
        "single-monitor no-primary fallback must be selectable"
    );
    assert_eq!(
        display.native_bounds, bounds,
        "Xft.dpi must not scale root geometry"
    );
    assert_eq!(
        (display.capture_size.width, display.capture_size.height),
        fixture.root_size
    );
    let mut first = None;
    for selection in [
        Selection::Primary,
        Selection::Display(display.id.clone()),
        Selection::Desktop,
    ] {
        provider.select(&selection).unwrap();
        let captured = provider.capture(exact(bounds)).unwrap();
        assert_eq!(captured.mapping.unit(), NativeUnit::PhysicalPixels);
        let png = decode(&captured.png, bounds);
        assert_paint(&png, &captured.mapping, bounds, painted);
        assert_eq!(captured.topology.generation(), snapshot.generation());
        if let Some(ref previous) = first {
            assert_eq!(
                &png, previous,
                "primary/id/desktop must give identical root pixels"
            );
        } else {
            first = Some(png);
        }
    }
    let reads = probe.read();
    assert_eq!(
        (reads.requests, reads.replies, reads.decode_entries),
        (3, 3, 3)
    );
    assert!(reads.reply_bytes > 0);
    assert_eq!(reads.decoded_rgba_bytes, 3 * bytes(bounds));
    assert_eq!(reads.last_bounds, Some(bounds));
    assert_eq!(reads.last_permit_bytes, bytes(bounds));
    eprintln!("native root GetImage/decode observation: {reads:?}");
}

#[test]
#[ignore = "CC-only: explicit isolated disposable Xvfb required; see native_x11/README.md"]
fn real_source_admission_precedes_getimage_and_decode() {
    let mut fixture = Fixture::connect();
    let bounds = NativeRect {
        x: 0,
        y: 0,
        width: fixture.root_size.0,
        height: fixture.root_size.1,
    };
    let painted = fixture.paint(bounds, false);
    let native = Rc::new(RefCell::new(X11Provider::attach().unwrap()));
    let probe = native.borrow().capture_probe();
    let mut positive = DisplayBackend::new(Shared(native.clone()), generous()).unwrap();
    let observed = positive.observe(Selection::Primary, exact(bounds)).unwrap();
    let png = decode(&observed.png, bounds);
    assert_eq!(
        png.as_raw().as_slice(),
        observed.rgba.pixels(),
        "encoded PNG must reproduce real composed RGBA"
    );
    assert_paint(&png, &observed.mapping, bounds, painted);
    let before = probe.read();
    assert_eq!(
        (before.requests, before.replies, before.decode_entries),
        (1, 1, 1),
        "positive control must exercise the REAL native seam"
    );
    assert!(before.reply_bytes > 0);
    assert_eq!(before.decoded_rgba_bytes, bytes(bounds));
    drop(observed);
    drop(positive);

    // Same live provider/connection/probe, real enumeration and engine. No fake
    // frame, invented SourcePermit, substituted reply, or synthetic counter.
    let limit = bytes(bounds) - 1;
    let mut refused = DisplayBackend::new(
        Shared(native.clone()),
        MemoryBudget::new(limit, 64 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let error = refused
        .observe(Selection::Primary, exact(bounds))
        .unwrap_err();
    assert_eq!(
        error,
        DisplayError::BudgetExceeded {
            kind: "source_bytes",
            required: bytes(bounds),
            limit
        }
    );
    assert_eq!(
        probe.read(),
        before,
        "rejected source must issue no GetImage and never enter native decode"
    );
    assert_eq!(
        crate::backend::display::provider_error(error).code,
        "resource_limit"
    );

    // Separately force total-payload admission, not topology quantization/PNG.
    let peak = 3 * bytes(bounds); // full-size canvas + reply + source RGBA
    let mut total = DisplayBackend::new(
        Shared(native),
        MemoryBudget::new(bytes(bounds), peak - 1).unwrap(),
    )
    .unwrap();
    assert_eq!(
        total
            .observe(Selection::Primary, exact(bounds))
            .unwrap_err(),
        DisplayError::BudgetExceeded {
            kind: "total_bytes",
            required: peak,
            limit: peak - 1,
        }
    );
    assert_eq!(probe.read(), before);
    eprintln!(
        "native admission positive={before:?}; after source/total rejection={:?}",
        probe.read()
    );
}
