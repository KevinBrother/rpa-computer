//! Screen capture against the target (primary) display, with truthful PNG
//! dimensions and a geometry version that tracks display/DPI changes.

use super::Geometry;

/// Target display descriptor: `display_info` uses native backend input units
/// (macOS global points / Windows virtual-screen pixels). `capture_width` /
/// `capture_height` are the expected *pixel* dimensions of the captured image
/// (may exceed `display_info.width/height` on macOS Retina).
///
/// Expected-dimension derivation (`width * scale_factor`, `target_screen`):
/// matches what the installed dependencies actually produce on both supported
/// platforms (verified against display-info 0.4.8 and screenshots 0.8.10
/// sources):
/// - **macOS**: display-info reports logical CG points and derives
///   `scale_factor = backing pixels / points`; `screenshots` returns the raw
///   CGImage at backing resolution, so pixels == points * scale. The
///   multiplication is required.
/// - **Windows (PMv2-aware process, enforced by `prepare_thread`)**:
///   display-info reports physical `rcMonitor` pixels and derives
///   `scale_factor = DESKTOPHORZRES / HORZRES`; `screenshots::capture_screen`
///   computes its GDI blit size as `display_info.width * scale_factor` — the
///   SAME expression as ours, so the expected and actual dimensions agree by
///   construction (deviation truncates identically; the aware-process device
///   DC is in physical pixels, so the physical-pixel-sized blit is 1:1, not a
///   downscale). The multiplication is therefore not double-scaling; removing
///   it would DESYNCHRONIZE expected vs actual and trip the hard dimension
///   check in `windows::capture_screen`. Do not "fix" the formula without
///   re-verifying the installed dependency code.
#[derive(Debug, Clone, Copy)]
pub struct TargetScreen {
    pub display_info: screenshots::display_info::DisplayInfo,
    pub capture_width: u32,
    pub capture_height: u32,
    pub scale_factor: f32,
}

#[derive(Debug)]
pub enum ScreenCaptureError {
    /// No displays at all, or no primary display.
    NoDisplay(String),
    /// The OS denied the capture (macOS Screen Recording permission).
    Denied(String),
    /// Any other capture/encode failure.
    Failed(String),
}

impl std::fmt::Display for ScreenCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScreenCaptureError::NoDisplay(m) => write!(f, "no display: {m}"),
            ScreenCaptureError::Denied(m) => write!(f, "screen capture denied: {m}"),
            ScreenCaptureError::Failed(m) => write!(f, "capture failed: {m}"),
        }
    }
}

impl std::error::Error for ScreenCaptureError {}

/// Enumerate displays and pick the primary one, computing expected capture
/// dimensions. Called on every geometry()/capture() so display changes are
/// always observed live.
pub fn target_screen() -> Result<TargetScreen, ScreenCaptureError> {
    let screens = screenshots::Screen::all()
        .map_err(|e| ScreenCaptureError::NoDisplay(format!("failed to enumerate displays: {e}")))?;
    let primary = screens
        .into_iter()
        .find(|s| s.display_info.is_primary)
        .ok_or_else(|| ScreenCaptureError::NoDisplay("no primary display found".to_string()))?;
    let info = primary.display_info;
    let scale = if info.scale_factor.is_finite() && info.scale_factor > 0.0 {
        info.scale_factor
    } else {
        1.0
    };
    let (capture_width, capture_height) = expected_capture_size(info.width, info.height, scale);
    if capture_width == 0 || capture_height == 0 || info.width == 0 || info.height == 0 {
        return Err(ScreenCaptureError::Failed(format!(
            "display {} has degenerate dimensions ({}x{} input, {}x{} capture)",
            info.id, info.width, info.height, capture_width, capture_height
        )));
    }
    Ok(TargetScreen {
        display_info: info,
        capture_width,
        capture_height,
        scale_factor: scale,
    })
}

/// Expected capture pixel dimensions for a display. See the [`TargetScreen`]
/// docs for the per-platform derivation; the formula intentionally matches
/// the one `screenshots` 0.8.10 uses internally on Windows, keeping expected
/// and actual dimensions consistent by construction. Pure: unit-testable.
pub fn expected_capture_size(width: u32, height: u32, scale_factor: f32) -> (u32, u32) {
    (
        ((width as f64) * scale_factor as f64).round() as u32,
        ((height as f64) * scale_factor as f64).round() as u32,
    )
}

impl TargetScreen {
    /// Geometry of this target in native backend input units.
    pub fn geometry(&self) -> Geometry {
        let info = &self.display_info;
        Geometry {
            surface_id: format!("{}:{}", std::env::consts::OS, info.id),
            input_origin: (info.x, info.y),
            input_size: (info.width, info.height),
            version: geometry_version(
                info.id,
                (info.x, info.y),
                (info.width, info.height),
                (self.capture_width, self.capture_height),
                info.rotation,
            ),
        }
    }
}

/// Deterministic version string: changes with monitor identity, input
/// origin/size, capture pixel size (DPI/backing scale) and rotation.
pub fn geometry_version(
    display_id: u32,
    origin: (i32, i32),
    input_size: (u32, u32),
    capture_size: (u32, u32),
    rotation: f32,
) -> String {
    format!(
        "d{display_id}:o{},{}:i{}x{}:c{}x{}:r{}",
        origin.0,
        origin.1,
        input_size.0,
        input_size.1,
        capture_size.0,
        capture_size.1,
        rotation as i32
    )
}

/// Encode an RGBA buffer as PNG; the reported dimensions come from the
/// encoded image itself, never from a cached declaration.
pub fn encode_png(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<(Vec<u8>, u32, u32), ScreenCaptureError> {
    let img = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| {
        ScreenCaptureError::Failed(format!(
            "RGBA buffer size mismatch for {width}x{height} capture"
        ))
    })?;
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| ScreenCaptureError::Failed(format!("PNG encode failed: {e}")))?;
    Ok((buf.into_inner(), img.width(), img.height()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_changes_with_identity_geometry_and_dpi() {
        let base = geometry_version(1, (0, 0), (1512, 982), (3024, 1964), 0.0);
        assert_ne!(
            base,
            geometry_version(2, (0, 0), (1512, 982), (3024, 1964), 0.0)
        );
        assert_ne!(
            base,
            geometry_version(1, (100, 0), (1512, 982), (3024, 1964), 0.0)
        );
        assert_ne!(
            base,
            geometry_version(1, (0, 0), (1440, 900), (3024, 1964), 0.0)
        );
        assert_ne!(
            base,
            geometry_version(1, (0, 0), (1512, 982), (1512, 982), 0.0)
        );
        assert_ne!(
            base,
            geometry_version(1, (0, 0), (1512, 982), (3024, 1964), 90.0)
        );
        assert_eq!(
            base,
            geometry_version(1, (0, 0), (1512, 982), (3024, 1964), 0.0)
        );
    }

    #[test]
    fn encode_png_reports_actual_dimensions() {
        let rgba = vec![255u8; 4 * 4 * 6];
        let (png, w, h) = encode_png(4, 6, rgba).unwrap();
        assert_eq!((w, h), (4, 6));
        // The encoded PNG is genuinely decodable at those dimensions.
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 6));
    }

    #[test]
    fn encode_png_rejects_size_mismatch() {
        let err = encode_png(4, 6, vec![0u8; 10]).unwrap_err();
        assert!(matches!(err, ScreenCaptureError::Failed(_)));
    }

    #[test]
    fn error_display_is_descriptive() {
        assert!(ScreenCaptureError::Denied("tcc".into())
            .to_string()
            .contains("denied"));
    }

    /// Windows PMv2 derivation guard: display-info 0.4.8 reports physical
    /// rcMonitor pixels with scale = DESKTOPHORZRES/HORZRES, and screenshots
    /// 0.8.10's `capture_screen` blits `width * scale` physical pixels
    /// (1:1 on a DPI-aware process). Our expected size MUST equal that exact
    /// expression — this is consistency, not double-scaling (see the
    /// TargetScreen docs). If this test fails after a dependency change,
    /// re-inspect the installed sources before editing the formula.
    #[test]
    fn expected_capture_size_matches_screenshots_windows_formula() {
        // 1920x1080 rcMonitor at 125% scaling: scale = 2400/1920 = 1.25,
        // physical blit = 2400x1350 (1920 * 1.25). `capture_width` must be
        // 2400 so the hard check against the actual bitmap passes.
        assert_eq!(expected_capture_size(1920, 1080, 1.25), (2400, 1350));
        // 150%: 1920 * 1.5 = 2880.
        assert_eq!(expected_capture_size(1920, 1080, 1.5), (2880, 1620));
        // 100%: identity.
        assert_eq!(expected_capture_size(1920, 1080, 1.0), (1920, 1080));
    }

    /// macOS derivation guard: display-info reports logical points and
    /// screenshots returns the CGImage at backing resolution, so expected
    /// pixels == points * backing scale.
    #[test]
    fn expected_capture_size_matches_macos_backing_resolution() {
        assert_eq!(expected_capture_size(1512, 982, 2.0), (3024, 1964));
        assert_eq!(expected_capture_size(1440, 900, 1.0), (1440, 900));
    }
}
