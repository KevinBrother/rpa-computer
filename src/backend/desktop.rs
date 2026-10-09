//! [`DesktopBackend`]: the real native desktop backend. All held-state /
//! release / scroll-translation logic lives in the generic [`BackendCore`]
//! state machine, which is driven through the [`NativeInput`] dispatch seam
//! (production: [`DriverInput`]; tests: a recording mock), so the full input
//! lifecycle is regression-tested without touching a real desktop.
//!
//! Stale-geometry rejection is a per-SESSION concern owned by the Runtime
//! (it validates `geometry()` and `capture.geometry` against the session's
//! bound geometry and faults on mismatch); the backend retains enumeration authority, NOT a stale-session latch, so a new Runtime session can rebind after
//! a display change without restarting the host process. The pure
//! [`CaptureGeometryBinding`] policy below encodes exactly that session-side
//! decision and is unit-tested here so the lifecycle split is pinned by
//! backend-owned regression evidence.

#[cfg(not(target_os = "windows"))]
use super::capture::{self, TargetScreen};
use super::dispatch::{BackendCore, DriverInput};
use super::{Backend, BackendError, Capture, Geometry, InputEvent};
/// Native desktop backend. Intentionally NOT `Send`: construct it on the
/// worker thread that will perform all capture/inject calls.
///
/// `DesktopBackend::new()` performs permission/environment checks only; it
/// never injects input. A successful `new()` (and successful
/// `geometry()`/`capture()`) does NOT by itself prove input works — on macOS
/// the Accessibility check gates construction, on Windows there is no
/// equivalent grant so input success is only proven by `inject()` calls.
///
/// Geometry-change rejection is deliberately NOT persisted across captures
/// here: every capture is self-describing (`Capture.geometry`) and the
/// Runtime owns per-session geometry binding — it checks `geometry()` before
/// capturing and `capture.geometry` afterward, and it faults/rejects on
/// mismatch (fail-fast, no silent retarget). Latching the rejection inside
/// the backend would make even a normal safe close + fresh open of a NEW
/// session fail forever without restarting the host process, while adding
/// no race protection the Runtime does not already provide.
pub struct DesktopBackend {
    core: BackendCore<DriverInput>,
    #[cfg(target_os = "windows")]
    display: super::display::DisplayProvider<rpa_windows_display::GdiProvider>,
}

impl DesktopBackend {
    /// Create the backend on the calling thread. Checks:
    /// - a display server / primary display exists,
    /// - macOS: Accessibility (input) permission — WITHOUT opening the
    ///   system prompt,
    /// - Windows: the process is Per-Monitor-V2 DPI aware (attempted and
    ///   verified; failure is an error, not silently ignored).
    pub fn new() -> Result<Self, BackendError> {
        super::platform::prepare_thread()?;
        // Fails with a descriptive error when there is no usable primary
        // display (headless session, RDP with no console, etc.).
        #[cfg(not(target_os = "windows"))]
        capture::target_screen()?;
        #[cfg(target_os = "windows")]
        let mut display = super::display::DisplayProvider::new(
            rpa_windows_display::GdiProvider::attach().map_err(super::display::provider_error)?,
        )?;
        #[cfg(target_os = "windows")]
        display.snapshot()?;
        let driver = super::platform::create_driver()?;
        Ok(Self {
            core: BackendCore::new(DriverInput::new(driver)),
            #[cfg(target_os = "windows")]
            display,
        })
    }
}

impl Backend for DesktopBackend {
    #[cfg(target_os = "windows")]
    fn display_selections(&self) -> &'static [&'static str] {
        &["primary", "id", "desktop"]
    }
    fn platform(&self) -> &'static str {
        std::env::consts::OS
    }

    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        // Pure query: reflects current display layout/DPI. Input is neither
        // performed nor implied by success.
        #[cfg(target_os = "windows")]
        {
            self.display.geometry()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(capture::target_screen()?.geometry())
        }
    }

    fn capture(&mut self) -> Result<Capture, BackendError> {
        #[cfg(target_os = "windows")]
        {
            let c = self
                .display
                .capture(super::display::budget::requested(1366, 768)?)?;
            Ok(Capture {
                width: c.mapping.size().width,
                height: c.mapping.size().height,
                png: c.png,
                geometry: c.geometry,
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let target: TargetScreen = capture::target_screen()?;
            let geometry = target.geometry();
            let (png, width, height) = super::platform::capture_screen(&target)?;
            Ok(Capture {
                png,
                width,
                height,
                geometry,
            })
        }
    }
    #[cfg(target_os = "windows")]
    fn display_snapshot(
        &mut self,
    ) -> Result<Option<rpa_display_topology::TopologySnapshot>, BackendError> {
        self.display.snapshot().map(Some)
    }
    #[cfg(target_os = "windows")]
    fn select_display(
        &mut self,
        s: &rpa_display_topology::Selection,
    ) -> Result<Geometry, BackendError> {
        self.display.select(s)
    }
    #[cfg(target_os = "windows")]
    fn capture_display(
        &mut self,
        b: rpa_display_topology::CaptureBudget,
    ) -> Result<Option<super::display::DisplayCapture>, BackendError> {
        self.display.capture(b).map(Some)
    }

    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        self.core.inject_event(event)
    }

    fn release_all(&mut self) -> Result<(), BackendError> {
        self.core.release_all()
    }
}

/// Pure session-side geometry binding policy (no OS calls, no native state).
/// Exported for the Runtime (core) to reuse for its per-session geometry
/// binding instead of duplicating the comparison rule; until then it is
/// exercised by the tests below, so keep it crate-visible.
///
/// The Runtime owns one binding per session: created at open (bound to the
/// then-current geometry), consulted before trusting any capture, and
/// dropped when the session closes. A display change therefore rejects
/// further captures of the EXISTING session (fail-fast: the session faults
/// and must be closed — it is never silently retargeted), while a session
/// opened after the change binds to the NEW geometry and captures normally.
/// That is what lets native captures recover after a normal close + fresh
/// open without restarting the host process; the backend keeps no
/// cross-session geometry memory (the persistent Windows tracker records current facts, not a stale-session latch).
///
/// This mirrors the checks the Runtime performs in
/// `runtime::session::capture_observation`: `geometry()` is checked BEFORE
/// capturing and `capture.geometry` is checked AFTER, so a geometry change
/// racing the capture itself is still rejected.
#[allow(dead_code)] // exported seam for the Runtime (core) to adopt; exercised by tests below
#[derive(Debug, Clone)]
pub struct CaptureGeometryBinding {
    bound: Geometry,
}

/// Outcome of comparing live geometry against a session binding.
#[allow(dead_code)] // see CaptureGeometryBinding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryCheck {
    /// Matches the session's bound geometry; the capture may be trusted.
    Current,
    /// Geometry/surface changed since the session bound; the session must
    /// reject (fail-fast) and must NOT silently rebind to the new geometry.
    Stale,
}

#[allow(dead_code)] // see CaptureGeometryBinding
impl CaptureGeometryBinding {
    /// Bind a session to the geometry observed when it opened.
    pub fn new(bound: Geometry) -> Self {
        Self { bound }
    }

    /// Geometry version this session is bound to.
    pub fn bound_version(&self) -> &str {
        &self.bound.version
    }

    /// Compare live geometry (from a pre-capture `geometry()` query or from
    /// `capture.geometry`) against the session binding.
    pub fn check(&self, live: &Geometry) -> GeometryCheck {
        if live.version == self.bound.version && live.surface_id == self.bound.surface_id {
            GeometryCheck::Current
        } else {
            GeometryCheck::Stale
        }
    }
}

#[cfg(test)]
mod tests {
    //! Lifecycle regression evidence for the geometry-binding split, driven
    //! through a scripted mock capture backend — no real desktop, no native
    //! capture. The mock models the FIXED `DesktopBackend` capture contract:
    //! `capture()` always reports the CURRENT geometry (never latched to a
    //! previous capture), so a new session can rebind after a display change.

    use super::*;
    use std::collections::VecDeque;

    /// Scripted capture backend: `geometry()`/`capture()` answer from queues
    /// so tests control the exact sequence of geometry values, including a
    /// change racing the capture call itself.
    struct ScriptedCaptureBackend {
        geometry_answers: VecDeque<Geometry>,
        capture_answers: VecDeque<Geometry>,
        pub captures: usize,
    }

    impl ScriptedCaptureBackend {
        fn new() -> Self {
            Self {
                geometry_answers: VecDeque::new(),
                capture_answers: VecDeque::new(),
                captures: 0,
            }
        }

        fn push_geometry(&mut self, version: &str) {
            self.geometry_answers.push_back(geom(version));
        }

        fn push_capture(&mut self, version: &str) {
            self.capture_answers.push_back(geom(version));
        }
    }

    fn geom(version: &str) -> Geometry {
        Geometry {
            surface_id: "macos:1".into(),
            input_origin: (0, 0),
            input_size: (1512, 982),
            version: version.into(),
        }
    }

    impl Backend for ScriptedCaptureBackend {
        fn platform(&self) -> &'static str {
            "scripted"
        }

        fn geometry(&mut self) -> Result<Geometry, BackendError> {
            self.geometry_answers
                .pop_front()
                .ok_or_else(|| BackendError::new("internal", "no scripted geometry answer"))
        }

        fn capture(&mut self) -> Result<Capture, BackendError> {
            self.captures += 1;
            let geometry = self
                .capture_answers
                .pop_front()
                .ok_or_else(|| BackendError::new("internal", "no scripted capture answer"))?;
            Ok(Capture {
                png: vec![0x89, b'P', b'N', b'G'],
                width: 2,
                height: 2,
                geometry,
            })
        }

        fn inject(&mut self, _event: &InputEvent) -> Result<(), BackendError> {
            Ok(())
        }

        fn release_all(&mut self) -> Result<(), BackendError> {
            Ok(())
        }
    }

    /// The two-phase protocol the Runtime's `capture_observation` performs:
    /// check `geometry()` against the session binding BEFORE capture and
    /// `capture.geometry` AFTER capture. Fail-fast (`geometry_changed`) on
    /// any mismatch; never retarget the binding.
    fn runtime_style_capture(
        binding: &CaptureGeometryBinding,
        backend: &mut ScriptedCaptureBackend,
    ) -> Result<Capture, BackendError> {
        let queried = backend.geometry()?;
        if binding.check(&queried) == GeometryCheck::Stale {
            return Err(BackendError::new(
                "geometry_changed",
                "geometry query does not match session binding",
            ));
        }
        let capture = backend.capture()?;
        if binding.check(&capture.geometry) == GeometryCheck::Stale {
            return Err(BackendError::new(
                "geometry_changed",
                "capture geometry does not match session binding",
            ));
        }
        Ok(capture)
    }

    /// Test-local unwrap of a capture-style result: panics with the backend
    /// error code without requiring `Capture: Debug` (the contract type has
    /// no Debug impl and we must not add one).
    fn err_code(result: Result<Capture, BackendError>) -> String {
        match result {
            Err(e) => e.code,
            Ok(_) => panic!("expected an error result, got Ok(capture)"),
        }
    }

    #[test]
    fn geometry_change_faults_existing_session_but_new_session_recovers() {
        let mut backend = ScriptedCaptureBackend::new();
        // Session A opens while the display is at v1.
        backend.push_geometry("v1");
        let session_a = CaptureGeometryBinding::new(backend.geometry().unwrap());
        assert_eq!(session_a.bound_version(), "v1");
        backend.push_geometry("v1");
        backend.push_capture("v1");
        assert!(runtime_style_capture(&session_a, &mut backend).is_ok());

        // Display changes to v2 (DPI change, monitor swap, ...). The
        // EXISTING session must fail fast — no silent retarget.
        backend.push_geometry("v2");
        assert_eq!(
            err_code(runtime_style_capture(&session_a, &mut backend)),
            "geometry_changed"
        );
        assert_eq!(
            session_a.bound_version(),
            "v1",
            "rejection must not silently rebind the existing session"
        );

        // A normal safe close drops the binding; a FRESH session rebinds to
        // the new geometry and captures succeed again WITHOUT restarting the
        // host process (the backend holds no cross-session geometry state).
        drop(session_a);
        backend.push_geometry("v2");
        let session_b = CaptureGeometryBinding::new(backend.geometry().unwrap());
        backend.push_geometry("v2");
        backend.push_capture("v2");
        let cap = runtime_style_capture(&session_b, &mut backend).unwrap();
        assert_eq!(cap.geometry.version, "v2");
    }

    #[test]
    fn within_capture_geometry_race_is_rejected() {
        let mut backend = ScriptedCaptureBackend::new();
        let binding = CaptureGeometryBinding::new(geom("v1"));
        // geometry() still reports v1, but the geometry changes before the
        // capture itself completes: capture.geometry reports v2.
        backend.push_geometry("v1");
        backend.push_capture("v2");
        assert_eq!(
            err_code(runtime_style_capture(&binding, &mut backend)),
            "geometry_changed"
        );
        assert_eq!(
            binding.bound_version(),
            "v1",
            "the race must fault the session, never retarget it"
        );
    }

    #[test]
    fn unchanged_geometry_captures_repeatedly_without_false_rejection() {
        let mut backend = ScriptedCaptureBackend::new();
        let binding = CaptureGeometryBinding::new(geom("v1"));
        for _ in 0..3 {
            backend.push_geometry("v1");
            backend.push_capture("v1");
            assert!(runtime_style_capture(&binding, &mut backend).is_ok());
        }
        assert_eq!(backend.captures, 3);
    }

    #[test]
    fn surface_change_with_same_version_is_still_stale() {
        let binding = CaptureGeometryBinding::new(geom("v1"));
        let mut other_surface = geom("v1");
        other_surface.surface_id = "macos:2".into();
        assert_eq!(binding.check(&other_surface), GeometryCheck::Stale);
        assert_eq!(binding.check(&geom("v2")), GeometryCheck::Stale);
        assert_eq!(binding.check(&geom("v1")), GeometryCheck::Current);
    }
}
