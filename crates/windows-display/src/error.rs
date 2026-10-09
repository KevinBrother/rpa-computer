use std::fmt;

/// Structured errors: no fallback, stale pixels, or partial-desktop successes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DisplayError {
    /// Actual OS backend is available only on Windows.
    UnsupportedPlatform,
    /// Both process and current thread must actually be PMv2.
    DpiContextMismatch {
        /// Whether the verified process context is PMv2.
        process_pmv2: bool,
        /// Whether the verified thread context is PMv2.
        thread_pmv2: bool,
    },
    /// Raw Win32 error or HRESULT, preserving the operation name.
    Native {
        /// Failed Windows API operation.
        operation: &'static str,
        /// Win32 error/HRESULT bits, zero if API does not provide extended error.
        code: u32,
    },
    /// Explicit cleanup failed; no successful observation is returned.
    Cleanup {
        /// Resource or selection restoration that failed.
        resource: &'static str,
    },
    /// A Windows display/mode cannot be represented reliably.
    UnsupportedDisplay {
        /// Diagnostic reason, not a fabricated default.
        reason: &'static str,
    },
    /// Monitor facts/identity no longer match the requested source.
    SourceChanged {
        /// Previously enumerated opaque ID.
        id: String,
    },
    /// Opaque ID violates the future wire-token ASCII/length constraint.
    InvalidDisplayId {
        /// Rejected ID.
        id: String,
    },
    /// Bounded native enumeration exceeded its explicit resource cap.
    EnumerationLimit,
    /// Duplicate OS handle/device identity, not a made-up second screen.
    DuplicateNativeIdentity,
    /// Monotonic runtime-ID allocation exhausted; never wraps.
    IdentityExhausted,
    /// Underlying topology diagnostic is retained intact.
    Topology(rpa_display_topology::Error),
    /// Source/total byte limits must both be nonzero.
    InvalidMemoryBudget,
    /// A byte reservation would exceed a caller limit.
    BudgetExceeded {
        /// source_bytes or total_bytes.
        kind: &'static str,
        /// Checked required byte count.
        required: u64,
        /// Explicit configured limit.
        limit: u64,
    },
    /// Checked integer arithmetic/conversion failed before allocation.
    ArithmeticOverflow {
        /// Operation name.
        operation: &'static str,
    },
    /// Zero frame axis is invalid.
    InvalidFrameSize,
    /// Pixel payload did not match the exact packed RGBA length.
    InvalidFrameLength {
        /// Required packed byte count.
        expected: u64,
        /// Supplied byte count.
        actual: u64,
    },
    /// Only tightly packed 32-bit rows are accepted; no guessed stride.
    InvalidStride {
        /// Required row bytes.
        expected: u64,
        /// Supplied row bytes.
        actual: u64,
    },
    /// Fallible allocation failed, rather than allocating an unbounded buffer.
    AllocationFailed {
        /// Requested payload bytes.
        bytes: u64,
    },
}
impl From<rpa_display_topology::Error> for DisplayError {
    fn from(error: rpa_display_topology::Error) -> Self {
        Self::Topology(error)
    }
}
impl fmt::Display for DisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DisplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Topology(error) => Some(error),
            _ => None,
        }
    }
}
pub(crate) fn overflow(operation: &'static str) -> DisplayError {
    DisplayError::ArithmeticOverflow { operation }
}
