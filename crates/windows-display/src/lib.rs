//! Standalone Windows PMv2 enumeration and bounded GDI capture.
//! Pure provider/composition APIs are portable; real capture is Windows-only.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]

mod budget;
#[cfg(any(target_os = "windows", test))]
mod dpi;
mod engine;
mod error;
mod frame;
#[cfg(any(target_os = "windows", test))]
mod identity;
#[cfg(any(target_os = "windows", test))]
mod mode;
mod output;
#[cfg(any(target_os = "windows", test))]
mod owner;
mod png;

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod native;
#[cfg(not(target_os = "windows"))]
mod unsupported;

pub use budget::{MemoryBudget, SourcePermit};
pub use engine::{CapturedObservation, DisplayBackend, FrameProvider};
pub use error::DisplayError;
pub use frame::{RgbaFrame, RowOrder};
#[cfg(target_os = "windows")]
pub use native::{verify_pmv2, GdiProvider};
pub use output::{estimate_encoded_size, EncodedSize};
#[cfg(not(target_os = "windows"))]
pub use unsupported::{verify_pmv2, GdiProvider};

/// Real Windows capture backend, or explicit UnsupportedPlatform off Windows.
/// Construct only using [`attach`]; no process/thread DPI settings are changed.
pub type WindowsDisplay = DisplayBackend<GdiProvider>;

/// Attach to an already PMv2-prepared process AND thread. Verifies both, makes
/// no awareness changes, does not enumerate/capture. Keep on the owning thread.
pub fn attach(memory: MemoryBudget) -> Result<WindowsDisplay, DisplayError> {
    DisplayBackend::new(GdiProvider::attach()?, memory)
}
