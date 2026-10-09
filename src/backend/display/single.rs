//! Explicit synthetic single-display authority for test/diagnostic backends.
//! Not a native Windows fallback and never infers unit from target cfg.
use super::{error, BackendError, Geometry};
use rpa_display_topology::*;
pub struct SingleDisplay {
    tracker: TopologyTracker,
    version: Option<String>,
    unit: NativeUnit,
}
impl SingleDisplay {
    pub fn new(unit: NativeUnit) -> Self {
        Self {
            tracker: TopologyTracker::new().expect("topology authority exhausted"),
            version: None,
            unit,
        }
    }
    pub fn snapshot(
        &mut self,
        g: &Geometry,
        size: PixelSize,
    ) -> Result<TopologySnapshot, BackendError> {
        if self.version.as_ref().is_some_and(|v| v != &g.version) {
            self.tracker = TopologyTracker::new().map_err(error)?;
        }
        self.version = Some(g.version.clone());
        self.tracker
            .update(
                self.unit,
                vec![DisplayDescriptor {
                    id: g.surface_id.clone(),
                    is_primary: true,
                    native_bounds: NativeRect {
                        x: g.input_origin.0,
                        y: g.input_origin.1,
                        width: g.input_size.0,
                        height: g.input_size.1,
                    },
                    capture_size: size,
                    scale: 1.0,
                    rotation: Rotation::Deg0,
                }],
            )
            .map_err(error)
    }
}
