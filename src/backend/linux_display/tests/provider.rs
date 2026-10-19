use super::support::*;
use crate::backend::display::DisplayProvider;
use rpa_display_topology::{CaptureBudget, Selection};
use rpa_windows_display::{DisplayBackend, DisplayError, MemoryBudget};

fn budget() -> CaptureBudget {
    CaptureBudget::new(8, 5, 40).unwrap()
}

#[test]
fn all_sources_are_budgeted_before_any_read_or_source_allocation() {
    let mut monitors = pair();
    // Only the SECOND source exceeds the limit. Checking lazily per capture is
    // insufficient: even the first small source must not be read/allocated.
    monitors[0].bounds.width = 1024;
    monitors[0].bounds.height = 1024;
    // Fixture correction after independent RED: at 64px, the old 76px
    // native gap (x=1100) collapsed to zero, so planning failed BEFORE the
    // source budget. A 512px gap is representable. The resource-limit and
    // zero-read/allocation assertions stay strict; this is not a product fix.
    monitors[1].bounds.x = 1536;
    monitors[1].bounds.y = 0;
    monitors[1].bounds.width = 8192;
    monitors[1].bounds.height = 8192;
    let shared = facts(monitors);
    let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
    provider.select(&Selection::Desktop).unwrap();
    // Keep both tiles nonzero so topology quantization cannot mask admission.
    match provider.capture(CaptureBudget::new(64, 64, 4096).unwrap()) {
        Err(e) => assert_eq!(e.code, "resource_limit"),
        Ok(_) => panic!("oversized native source must be rejected before capture"),
    }
    assert_eq!(shared.borrow().reads, 0);
    assert_eq!(shared.borrow().allocations, 0);
}

#[test]
fn combined_native_reply_and_rgba_payload_is_preflighted() {
    let shared = facts(vec![Monitor::new("one", true, 0, 0, 4, 3)]);
    // Source=48; two native/source payloads plus a 1px canvas=100 > 99.
    let mut engine =
        DisplayBackend::new(Frames(shared.clone()), MemoryBudget::new(48, 99).unwrap()).unwrap();
    let result = engine.observe(Selection::Primary, CaptureBudget::new(1, 1, 1).unwrap());
    assert!(matches!(
        result,
        Err(DisplayError::BudgetExceeded {
            kind: "total_bytes",
            ..
        })
    ));
    assert_eq!(shared.borrow().reads, 0);
    assert_eq!(shared.borrow().allocations, 0);
}

#[test]
fn stale_move_identity_primary_or_rotation_discards_entire_capture() {
    for mutation in [
        Change::Move,
        Change::ReplaceIdentity,
        Change::Primary,
        Change::Rotation,
    ] {
        let shared = facts(pair());
        shared.borrow_mut().change_on_read = Some(mutation);
        let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
        match provider.capture(budget()) {
            Err(e) => assert_eq!(e.code, "geometry_changed"),
            Ok(_) => panic!("no pixels may escape a stale topology"),
        }
        assert_eq!(shared.borrow().reads, 1);
        // Stale capture is not a backend lifetime latch: a new session may bind.
        assert!(provider.geometry().is_ok());
        assert!(provider.capture(budget()).is_ok());
    }
}

#[test]
fn topology_change_between_root_preflight_and_engine_admission_is_rejected() {
    let shared = facts(pair());
    shared.borrow_mut().change_on_query = Some((2, Change::ReplaceIdentity));
    let mut provider = DisplayProvider::new(Frames(shared)).unwrap();
    match provider.capture(budget()) {
        Err(e) => assert_eq!(e.code, "geometry_changed"),
        Ok(_) => panic!("capture must match the root's admission generation"),
    }
}

#[test]
fn font_dpi_change_during_capture_is_not_a_topology_race() {
    let shared = facts(pair());
    shared.borrow_mut().change_on_read = Some(Change::XftDpiOnly);
    let mut provider = DisplayProvider::new(Frames(shared)).unwrap();
    assert!(
        provider.capture(budget()).is_ok(),
        "Xft.dpi does not change root pixels"
    );
}

#[test]
fn failed_fresh_query_never_reuses_cached_geometry_or_pixels() {
    let shared = facts(pair());
    let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
    provider.snapshot().unwrap();
    shared.borrow_mut().fail_query = true;
    assert!(provider.geometry().is_err());
    assert!(provider.capture(budget()).is_err());
    assert_eq!(shared.borrow().reads, 0);
}

#[test]
fn failed_native_read_does_not_publish_partial_desktop() {
    let shared = facts(pair());
    shared.borrow_mut().fail_read = true;
    let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
    provider.select(&Selection::Desktop).unwrap();
    assert!(provider.capture(budget()).is_err());
    assert_eq!(shared.borrow().reads, 1);
    assert_eq!(shared.borrow().allocations, 0);
}

#[test]
fn collapsed_output_gap_is_a_planning_failure_not_source_budget_evidence() {
    let mut monitors = pair();
    monitors[0].bounds.width = 1024;
    monitors[0].bounds.height = 1024;
    monitors[1].bounds.x = 1100;
    monitors[1].bounds.y = 0;
    monitors[1].bounds.width = 8192;
    monitors[1].bounds.height = 8192;
    let shared = facts(monitors);
    let mut provider = DisplayProvider::new(Frames(shared.clone())).unwrap();
    provider.select(&Selection::Desktop).unwrap();
    match provider.capture(CaptureBudget::new(64, 64, 4096).unwrap()) {
        Err(error) => {
            assert_eq!(error.code, "display_error");
            assert!(error.message.contains("CollapsedGap"), "{error}");
        }
        Ok(_) => panic!("planning must not erase the native desktop gap"),
    }
    assert_eq!(shared.borrow().reads, 0);
    assert_eq!(shared.borrow().allocations, 0);
}
