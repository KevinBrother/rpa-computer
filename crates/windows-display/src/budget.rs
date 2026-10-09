use crate::{error::overflow, frame::rgba_bytes, png, DisplayError};
use rpa_display_topology::{CapturePlan, PixelSize};

/// Explicit payload-byte budgets in addition to topology's final pixel budget.
/// Peak counts canvas + native DIB + returned source, or canvas + exact PNG.
/// Does not claim to limit allocator bookkeeping, OS/driver-internal overhead,
/// caller-retained prior observations, or a malicious third-party provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBudget {
    source: u64,
    total: u64,
}
impl MemoryBudget {
    /// Maximum bytes in one packed source and maximum concurrent image payload.
    pub fn new(max_source_bytes: u64, max_total_bytes: u64) -> Result<Self, DisplayError> {
        if max_source_bytes == 0 || max_total_bytes == 0 {
            return Err(DisplayError::InvalidMemoryBudget);
        }
        Ok(Self {
            source: max_source_bytes,
            total: max_total_bytes,
        })
    }
    /// Limit for one real source frame, before native allocation.
    pub fn max_source_bytes(self) -> u64 {
        self.source
    }
    /// Limit for concurrent controlled image payload, not process RSS.
    pub fn max_total_bytes(self) -> u64 {
        self.total
    }
    pub(crate) fn preflight(self, plan: &CapturePlan) -> Result<(), DisplayError> {
        let canvas = rgba_bytes(plan.size())?;
        // Check every source before the first capture, not only selected output.
        for tile in plan.tiles() {
            let source = rgba_bytes(tile.source_size)?;
            require("source_bytes", source, self.source)?;
            let peak = source
                .checked_mul(2)
                .and_then(|n| n.checked_add(canvas))
                .ok_or_else(|| overflow("capture peak bytes"))?;
            require("total_bytes", peak, self.total)?;
        }
        let peak = canvas
            .checked_add(png::encoded_len(plan.size())?)
            .ok_or_else(|| overflow("PNG peak bytes"))?;
        require("total_bytes", peak, self.total)
    }
    pub(crate) fn permit(self, size: PixelSize) -> Result<SourcePermit, DisplayError> {
        let bytes = rgba_bytes(size)?;
        require("source_bytes", bytes, self.source)?;
        Ok(SourcePermit { size, bytes })
    }
}
fn require(kind: &'static str, required: u64, limit: u64) -> Result<(), DisplayError> {
    if required > limit {
        Err(DisplayError::BudgetExceeded {
            kind,
            required,
            limit,
        })
    } else {
        Ok(())
    }
}

/// Engine-issued authorization for exactly one bounded source. Fields are
/// private; native provider validates it BEFORE creating DIB/CPU allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePermit {
    size: PixelSize,
    bytes: u64,
}
impl SourcePermit {
    /// Authorized physical source dimensions, never multiplied by DPI.
    pub fn size(self) -> PixelSize {
        self.size
    }
    /// Authorized bytes for each packed DIB/RGBA source buffer.
    pub fn bytes(self) -> u64 {
        self.bytes
    }
    #[cfg(target_os = "windows")]
    pub(crate) fn require(self, size: PixelSize) -> Result<(), DisplayError> {
        if self.size != size || rgba_bytes(size)? != self.bytes {
            return Err(DisplayError::UnsupportedDisplay {
                reason: "source permit does not match current physical mode",
            });
        }
        Ok(())
    }
}
