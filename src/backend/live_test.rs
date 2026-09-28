//! MANUAL DIAGNOSTIC (not part of automated acceptance): exercises the real
//! DesktopBackend capture/geometry path without any input injection.
//! Run explicitly with: cargo test --lib -- --ignored live_backend --nocapture
//!
//! Acceptance integrity: this test FAILS when the environment cannot run the
//! real backend (headless session, missing permission). It only reports
//! `SKIPPED` (as an explicit diagnostic) when the operator sets
//! `RPA_LIVE_TEST_ALLOW_SKIP=1`; a skipped run must never be counted as
//! capture/acceptance proof. Ordinary `cargo test` never runs it.

#[test]
#[ignore = "manual live diagnostic; performs real screen capture (no input)"]
fn live_backend_capture_only() {
    use crate::backend::{Backend, DesktopBackend};

    fn skipped_or_fail(reason: &str) {
        if std::env::var_os("RPA_LIVE_TEST_ALLOW_SKIP").is_some() {
            eprintln!("SKIPPED live_backend_capture_only: {reason}");
            eprintln!("NOTE: this run is NOT capture/acceptance proof.");
        } else {
            panic!(
                "live backend prerequisite unavailable: {reason} \
                 (set RPA_LIVE_TEST_ALLOW_SKIP=1 to record an explicit SKIP instead)"
            );
        }
    }

    let mut backend = match DesktopBackend::new() {
        Ok(b) => b,
        Err(e) => {
            skipped_or_fail(&format!("DesktopBackend::new() -> {e}"));
            return;
        }
    };
    assert_eq!(backend.platform(), std::env::consts::OS);

    let geo = match backend.geometry() {
        Ok(g) => g,
        Err(e) => {
            skipped_or_fail(&format!("geometry() -> {e}"));
            return;
        }
    };
    eprintln!("geometry: {geo:?}");
    assert!(geo.input_size.0 > 0 && geo.input_size.1 > 0);
    assert!(!geo.version.is_empty());

    let cap = match backend.capture() {
        Ok(c) => c,
        Err(e) => {
            skipped_or_fail(&format!("capture() -> {e}"));
            return;
        }
    };
    eprintln!(
        "capture: {}x{} px, {} png bytes, surface={}",
        cap.width,
        cap.height,
        cap.png.len(),
        cap.geometry.surface_id
    );
    let decoded = image::load_from_memory(&cap.png).expect("capture must be a valid PNG");
    assert_eq!(decoded.width(), cap.width, "reported width must match PNG");
    assert_eq!(
        decoded.height(),
        cap.height,
        "reported height must match PNG"
    );
    assert_eq!(
        cap.geometry.version, geo.version,
        "stable geometry within one run"
    );

    // Second capture must be consistent (no geometry change expected).
    let cap2 = backend.capture().expect("second capture");
    assert_eq!(cap2.geometry.version, cap.geometry.version);

    // release_all with nothing held is a no-op success.
    backend
        .release_all()
        .expect("release_all with empty held set");
    eprintln!("live capture diagnostic: OK (real capture performed)");
}
