use crate::{frame::rgba_bytes, png, DisplayError, MemoryBudget, RgbaFrame, SourcePermit};
use rpa_display_topology::{
    CaptureBudget, CapturePlan, DisplayDescriptor, NativeUnit, ObservationMapping, PixelRect,
    Selection, TopologySnapshot, TopologyTracker,
};

/// Provider boundary. Default tests supply owned pure frames; real Windows uses
/// GdiProvider. Implementations MUST honor the permit before source allocation,
/// return complete top-down RGBA, and report OS errors instead of stale pixels.
pub trait FrameProvider {
    /// Enumerate complete, current active display facts, never fabricated defaults.
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError>;
    /// Capture this exact descriptor after revalidating its native identity.
    /// The permit accounts for at most one native DIB and one owned source buffer.
    fn capture(
        &mut self,
        display: &DisplayDescriptor,
        permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError>;
}

/// Complete capture result. Fields describe the same pixel image and generation.
/// A failure returns none of these, including on post-capture topology changes.
#[derive(Debug)]
pub struct CapturedObservation {
    /// Exact pre/post-verified topology.
    pub topology: TopologySnapshot,
    /// Sole authority for source selection and tile placement.
    pub plan: CapturePlan,
    /// Coordinates for the actual encoded PNG, no metadata-only resize.
    pub mapping: ObservationMapping,
    /// Real composed, packed RGBA frame (not a source screenshot cache).
    pub rgba: RgbaFrame,
    /// Lossless RGBA8 PNG with exactly mapping.size() dimensions.
    pub png: Vec<u8>,
}

/// Long-lived provider + topology authority. No root Host or input dependencies.
/// GDI calls can block: timeout/isolation/quarantine/cancel remain Host duties.
#[derive(Debug)]
pub struct DisplayBackend<P> {
    provider: P,
    tracker: TopologyTracker,
    memory: MemoryBudget,
}
impl<P: FrameProvider> DisplayBackend<P> {
    /// Construct pure state only. Does NOT call the provider or change DPI.
    pub fn new(provider: P, memory: MemoryBudget) -> Result<Self, DisplayError> {
        Ok(Self {
            provider,
            tracker: TopologyTracker::new()?,
            memory,
        })
    }
    /// Fresh complete enumeration, explicit PhysicalPixels, persistent generation.
    /// A caller must not treat an earlier snapshot as current after failure.
    pub fn snapshot(&mut self) -> Result<TopologySnapshot, DisplayError> {
        let displays = self.provider.enumerate()?;
        for d in &displays {
            if d.id.is_empty() || !d.id.is_ascii() || d.id.len() > 128 {
                return Err(DisplayError::InvalidDisplayId { id: d.id.clone() });
            }
        }
        Ok(self.tracker.update(NativeUnit::PhysicalPixels, displays)?)
    }
    /// Enumerate, validate ALL allocations, capture each selected real source,
    /// independently resample tiles, encode, re-enumerate, then publish atomically.
    /// Source failures/changed generation discard the complete tentative result.
    pub fn observe(
        &mut self,
        selection: Selection,
        budget: CaptureBudget,
    ) -> Result<CapturedObservation, DisplayError> {
        let before = self.snapshot()?;
        let plan = CapturePlan::new(&before, selection, budget)?;
        self.memory.preflight(&plan)?;
        let mapping = ObservationMapping::new(&plan, plan.size(), None)?;
        let mut canvas = RgbaFrame::black(plan.size())?;
        for tile in plan.tiles() {
            let display = before
                .displays()
                .iter()
                .find(|d| d.id == tile.display_id)
                .ok_or_else(|| DisplayError::SourceChanged {
                    id: tile.display_id.clone(),
                })?;
            let permit = self.memory.permit(tile.source_size)?;
            let source = self.provider.capture(display, permit)?;
            plan.validate_source(&tile.display_id, source.size())?;
            // Private frame fields guarantee exact length; explicitly retain this
            // check at the boundary instead of trusting a provider's pixel labels.
            if source.pixels().len() as u64 != rgba_bytes(source.size())? {
                return Err(DisplayError::InvalidFrameLength {
                    expected: permit.bytes(),
                    actual: source.pixels().len() as u64,
                });
            }
            compose_tile(&mut canvas, &source, tile.composite_rect);
            // source released on this iteration; no all-screens staging allocation.
        }
        let png = png::encode(&canvas)?;
        let after = self.snapshot()?;
        plan.require_generation(after.generation())?;
        Ok(CapturedObservation {
            topology: after,
            plan,
            mapping,
            rgba: canvas,
            png,
        })
    }
}

// Nearest-neighbour sampling of EACH entire source; no whole-canvas filtering
// across boundaries, intermediate resized tile allocation or gap/padding bleed.
fn compose_tile(canvas: &mut RgbaFrame, source: &RgbaFrame, rect: PixelRect) {
    let cw = canvas.size().width as usize;
    let sw = source.size().width as usize;
    for y in 0..rect.height {
        let sy = u64::from(y) * u64::from(source.size().height) / u64::from(rect.height);
        for x in 0..rect.width {
            let sx = u64::from(x) * u64::from(source.size().width) / u64::from(rect.width);
            let from = (sy as usize * sw + sx as usize) * 4;
            let to = ((rect.y as usize + y as usize) * cw + rect.x as usize + x as usize) * 4;
            canvas.pixels_mut()[to..to + 4].copy_from_slice(&source.pixels()[from..from + 4]);
        }
    }
}
