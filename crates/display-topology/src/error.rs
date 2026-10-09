use crate::{Generation, ImagePoint, NativePoint, PixelSize};
use std::fmt;

/// Structured diagnostics. Invalid requests are never repaired into input targets.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Enumeration returned no displays.
    EmptyTopology,
    /// An opaque display ID was empty.
    EmptyDisplayId,
    /// Two enumerated descriptors used the same opaque ID.
    DuplicateDisplayId {
        /// Duplicated ID.
        id: String,
    },
    /// No descriptor was primary.
    NoPrimaryDisplay,
    /// More than one descriptor was primary.
    MultiplePrimaryDisplays {
        /// Primary display IDs, sorted.
        ids: Vec<String>,
    },
    /// Native width or height was zero.
    InvalidNativeSize {
        /// Invalid display ID.
        id: String,
    },
    /// A native rectangle includes positions outside i32 input coordinates.
    NativeBoundsOverflow {
        /// Invalid display ID.
        id: String,
    },
    /// Source capture width or height was zero.
    InvalidCaptureSize {
        /// Invalid display ID.
        id: String,
    },
    /// Scale was not finite and strictly positive.
    InvalidScale {
        /// Invalid display ID.
        id: String,
    },
    /// Explicit selection is absent; no primary fallback is performed.
    DisplayNotFound {
        /// Requested display ID.
        id: String,
    },
    /// Selected native regions overlap; desktop/mirror mapping is ambiguous.
    OverlappingDisplays {
        /// First sorted ID.
        first: String,
        /// Second sorted ID.
        second: String,
    },
    /// A capture limit was zero.
    InvalidBudget,
    /// Capture dimensions exceed the u32 PNG/rectangle protocol.
    BudgetDimensionsTooLarge,
    /// Checked arithmetic rejected an operation.
    ArithmeticOverflow {
        /// Operation name.
        operation: &'static str,
    },
    /// Even the minimum canvas cannot fit in the budget.
    BudgetTooSmall,
    /// Quantization removed a selected display on one or both axes.
    CollapsedRegion {
        /// Collapsed display ID.
        id: String,
    },
    /// A positive native separation vanished at the requested pixel resolution.
    CollapsedGap {
        /// First sorted display ID.
        first: String,
        /// Second sorted display ID.
        second: String,
    },
    /// Quantized regions overlap unexpectedly.
    OverlappingRegions {
        /// First ID.
        first: String,
        /// Second ID.
        second: String,
    },
    /// A requested source is not among the selected tiles.
    DisplayNotSelected {
        /// Requested display ID.
        id: String,
    },
    /// Decoded source dimensions do not match the topology facts.
    SourceSizeMismatch {
        /// Display ID.
        id: String,
        /// Enumerated size.
        expected: PixelSize,
        /// Decoded size.
        actual: PixelSize,
    },
    /// A tracker has consumed every generation revision; update did not commit.
    GenerationExhausted,
    /// Process-local tracker identity space was exhausted.
    TrackerIdentityExhausted,
    /// Observation/drag belongs to an older generation or a different tracker.
    StaleGeneration {
        /// Generation bound to the operation.
        expected: Generation,
        /// Latest caller-supplied generation.
        actual: Generation,
    },
    /// Actual decoded observation dimensions include a zero axis.
    InvalidObservationSize,
    /// Content rectangle is empty or extends outside the actual observation.
    InvalidContentRect,
    /// Image coordinate is nonfinite or outside the half-open observation.
    InvalidImagePoint {
        /// Rejected point.
        point: ImagePoint,
    },
    /// Image point hits no selected display (gap or padding).
    ImageGapOrPadding {
        /// Rejected point.
        point: ImagePoint,
    },
    /// Native coordinate does not hit any selected display.
    NativePointOutsideSelection {
        /// Rejected point.
        point: NativePoint,
    },
    /// At least two explicit waypoints are needed to prepare a drag.
    TooFewWaypoints,
    /// Segment index exceeds the prepared drag.
    SegmentNotFound {
        /// Rejected index.
        index: usize,
    },
    /// Internal interpolation requires steps > 0 and step <= steps.
    InvalidInterpolation {
        /// Requested step.
        step: u32,
        /// Requested total steps.
        steps: u32,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Variant and named fields remain visible to diagnostics consumers.
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
