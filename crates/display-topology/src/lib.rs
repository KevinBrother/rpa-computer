//! Pure, backend-independent display topology and observation coordinates.
//!
//! No enumeration, screenshot, pixel allocation, injection, sleep or cancellation
//! occurs here. See the crate README for the compositor and Runtime contracts.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod capture;
mod drag;
mod error;
mod geometry;
mod mapping;
mod topology;

pub use capture::{CaptureBudget, CapturePlan, CaptureTile, Density, DesktopLayout};
pub use drag::{DragPlan, NativeSegment};
pub use error::Error;
pub use geometry::{ImagePoint, NativePoint, NativeRect, PixelRect, PixelSize};
pub use mapping::{ObservationMapping, ObservationRegion};
pub use topology::{
    DisplayDescriptor, Generation, NativeUnit, Rotation, Selection, TopologySnapshot,
    TopologyTracker,
};
