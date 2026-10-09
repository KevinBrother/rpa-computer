//! Exact generation-bound observations; native provider already composed tiles.
use super::*;
use crate::backend::display::{budget, metadata};
use rpa_display_topology::{CapturePlan, ObservationMapping, PixelSize};

pub fn capture_observation(
    session: &mut Session,
    backend: &mut dyn Backend,
    wait_ms: u64,
    cancel: &Arc<AtomicBool>,
) -> Result<Observation, ToolError> {
    sleep_cancellable(
        Duration::from_millis(wait_ms.min(OBSERVE_MAX_WAIT_MS)),
        cancel,
    )?;
    if cancel.load(Ordering::SeqCst) {
        return Err(ToolError::new(codes::CANCELLED, "observe cancelled"));
    }
    let geometry = backend
        .geometry()
        .map_err(|e| fault(session, ToolError::from(e)))?;
    if geometry != session.geometry {
        return Err(fault(
            session,
            ToolError::new(codes::GEOMETRY_CHANGED, "selected geometry changed"),
        ));
    }
    let snapshot = crate::runtime::display::check(backend, session.display_generation)
        .map_err(|e| fault(session, e))?;
    // Direct session tests may construct Session without Runtime::open.
    if session.display_generation.is_none() {
        session.display_generation = snapshot.as_ref().map(|s| s.generation());
    }
    let requested =
        budget::requested(session.max_image.0, session.max_image.1).map_err(ToolError::from)?;
    let (png, actual, mapping) = if let Some(c) =
        backend.capture_display(requested).map_err(|e| {
            let e = ToolError::from(e);
            if e.code == codes::GEOMETRY_CHANGED {
                fault(session, e)
            } else {
                e
            }
        })? {
        if c.geometry != session.geometry
            || Some(c.topology.generation()) != session.display_generation
        {
            return Err(fault(
                session,
                ToolError::new(codes::GEOMETRY_CHANGED, "capture authority changed"),
            ));
        }
        let actual = image::decode_png(&c.png)?;
        if c.mapping.size()
            != (PixelSize {
                width: actual.width,
                height: actual.height,
            })
            || c.mapping.generation() != c.topology.generation()
            || actual.width > session.max_image.0
            || actual.height > session.max_image.1
        {
            return Err(ToolError::new(
                codes::CAPTURE_ERROR,
                "provider PNG/mapping/budget mismatch",
            ));
        }
        (c.png, actual, Some(c.mapping))
    } else {
        let c = backend.capture().map_err(ToolError::from)?;
        if c.geometry != session.geometry {
            return Err(fault(
                session,
                ToolError::new(codes::GEOMETRY_CHANGED, "capture geometry changed"),
            ));
        }
        let (png, actual) = image::downscale(&c.png, session.max_image)?;
        let mapping = if let Some(s) = &snapshot {
            // Only the explicit single-source legacy/mock seam may use whole
            // image resize. A multiscreen backend MUST provide capture_display.
            if s.displays().len() != 1 {
                return Err(ToolError::new(
                    codes::CAPTURE_ERROR,
                    "multi-display provider omitted mapped capture",
                ));
            }
            let plan = CapturePlan::new(s, session.display_selection.clone(), requested)
                .map_err(|e| ToolError::new(codes::CAPTURE_ERROR, e.to_string()))?;
            Some(
                ObservationMapping::new(
                    &plan,
                    PixelSize {
                        width: actual.width,
                        height: actual.height,
                    },
                    None,
                )
                .map_err(|e| ToolError::new(codes::CAPTURE_ERROR, e.to_string()))?,
            )
        } else {
            None
        };
        (png, actual, mapping)
    };
    budget::check_png(png.len()).map_err(ToolError::from)?;
    crate::runtime::display::check(backend, session.display_generation)
        .map_err(|e| fault(session, e))?;
    if cancel.load(Ordering::SeqCst) {
        return Err(ToolError::new(
            codes::CANCELLED,
            "capture cancelled before publication",
        ));
    }
    let display_metadata = mapping
        .as_ref()
        .map(metadata::observation)
        .unwrap_or(Value::Null);
    let map = CoordMap::new(
        actual,
        session.geometry.input_origin,
        session.geometry.input_size,
    )?;
    let obs = Observation {
        meta: ObservationMeta {
            observation_id: session.next_observation_id(),
            session_id: session.id.clone(),
            surface_id: session.geometry.surface_id.clone(),
            geometry_version: session.geometry.version.clone(),
            input_sequence: session.input_sequence,
            width_px: actual.width,
            height_px: actual.height,
            settled: false,
            display_metadata,
        },
        captured_at: Instant::now(),
        png,
        map,
        regions: mapping.map(Arc::new),
    };
    session.store_observation(obs.clone())?;
    Ok(obs)
}
