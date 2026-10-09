use crate::{DisplayError, FrameProvider, RgbaFrame, SourcePermit};
use rpa_display_topology::DisplayDescriptor;

/// Windows-only DPI verification. Never changes non-Windows state.
pub fn verify_pmv2() -> Result<(), DisplayError> {
    Err(DisplayError::UnsupportedPlatform)
}
/// Type-level API availability off Windows; cannot produce synthetic capture.
#[derive(Debug)]
pub struct GdiProvider {
    _private: (),
}
impl GdiProvider {
    /// Always rejects off Windows. Pure tests use their own FrameProvider instead.
    pub fn attach() -> Result<Self, DisplayError> {
        Err(DisplayError::UnsupportedPlatform)
    }
}
impl FrameProvider for GdiProvider {
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        Err(DisplayError::UnsupportedPlatform)
    }
    fn capture(
        &mut self,
        _: &DisplayDescriptor,
        _: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        Err(DisplayError::UnsupportedPlatform)
    }
}
