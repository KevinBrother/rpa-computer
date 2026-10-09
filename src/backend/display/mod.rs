//! Root display boundary. Native provider and mocks use the SAME pure engine.
use super::{BackendError, Geometry};
use rpa_display_topology::*;
use rpa_windows_display::{DisplayBackend, DisplayError, FrameProvider, MemoryBudget};

pub mod budget;
pub mod metadata;
pub mod single;

pub struct DisplayCapture {
    pub png: Vec<u8>,
    pub geometry: Geometry,
    pub topology: TopologySnapshot,
    pub mapping: ObservationMapping,
}

/// No input driver here. The owner must retain this value on the native thread.
pub struct DisplayProvider<P: FrameProvider> {
    engine: DisplayBackend<P>,
    selection: Selection,
}
impl<P: FrameProvider> DisplayProvider<P> {
    pub fn new(provider: P) -> Result<Self, BackendError> {
        let memory =
            MemoryBudget::new(64 * 1024 * 1024, 192 * 1024 * 1024).map_err(provider_error)?;
        Ok(Self {
            engine: DisplayBackend::new(provider, memory).map_err(provider_error)?,
            selection: Selection::Primary,
        })
    }
    pub fn snapshot(&mut self) -> Result<TopologySnapshot, BackendError> {
        let snapshot = self.engine.snapshot().map_err(provider_error)?;
        metadata::validate(&snapshot)?;
        Ok(snapshot)
    }
    pub fn geometry(&mut self) -> Result<Geometry, BackendError> {
        let snapshot = self.snapshot()?;
        metadata::geometry(&snapshot, &self.selection)
    }
    pub fn select(&mut self, selection: &Selection) -> Result<Geometry, BackendError> {
        let snapshot = self.snapshot()?;
        let geometry = metadata::geometry(&snapshot, selection)?;
        self.selection = selection.clone(); // commit only after successful validation
        Ok(geometry)
    }
    pub fn capture(&mut self, requested: CaptureBudget) -> Result<DisplayCapture, BackendError> {
        // Bound ALL possible plans admitted by the next observe, not just this
        // snapshot's plan. Thus a layout race cannot bypass output preflight.
        let budget = budget::bounded(requested)?;
        let before = self.snapshot()?;
        let plan = CapturePlan::new(&before, self.selection.clone(), budget).map_err(error)?;
        budget::check_estimate(plan.size())?;
        let observed = self
            .engine
            .observe(self.selection.clone(), budget)
            .map_err(provider_error)?;
        if observed.topology.generation() != before.generation() {
            return Err(BackendError::new(
                "geometry_changed",
                "topology changed before capture admission",
            ));
        }
        metadata::validate(&observed.topology)?;
        budget::check_png(observed.png.len())?;
        let geometry = metadata::geometry(&observed.topology, &self.selection)?;
        // Drop provider canvas before handing PNG to root decode/cache/base64.
        Ok(DisplayCapture {
            png: observed.png,
            geometry,
            topology: observed.topology,
            mapping: observed.mapping,
        })
    }
}
pub fn error(e: impl std::fmt::Display) -> BackendError {
    BackendError::new("display_error", e.to_string())
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

/// Keep allocation and stale-capture failures distinct from input dispatch.
pub fn provider_error(e: DisplayError) -> BackendError {
    let code = match &e {
        DisplayError::BudgetExceeded { .. } | DisplayError::AllocationFailed { .. } => {
            "resource_limit"
        }
        DisplayError::SourceChanged { .. }
        | DisplayError::Topology(Error::StaleGeneration { .. }) => "geometry_changed",
        _ => "display_error",
    };
    BackendError::new(code, e.to_string())
}
