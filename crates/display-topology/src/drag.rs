use crate::{Error, Generation, ImagePoint, NativePoint, ObservationMapping};

/// Validated drag endpoints in native units. Internal straight-line interpolation
/// may cross a physical gap; this is NOT a click/explicit-waypoint authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSegment {
    /// Validated user-specified start point on a selected display.
    pub start: NativePoint,
    /// Validated user-specified end point on a selected display.
    pub end: NativePoint,
}

/// Pure drag geometry, never a driver sleep loop or input dispatcher. Runtime
/// owns duration, pacing, polling, cancellation and held-button release/cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragPlan {
    generation: Generation,
    segments: Vec<NativeSegment>,
}
impl DragPlan {
    /// Validate EVERY native user waypoint as a real selected-display hit before
    /// creating any segments. Explicit gap points are always rejected.
    pub fn from_native(
        mapping: &ObservationMapping,
        waypoints: &[NativePoint],
        current: Generation,
    ) -> Result<Self, Error> {
        mapping.generation().require(current)?;
        if waypoints.len() < 2 {
            return Err(Error::TooFewWaypoints);
        }
        let points = waypoints
            .iter()
            .map(|p| mapping.map_native(*p, current))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::from_validated(mapping.generation(), &points))
    }
    /// Validate EVERY user image waypoint via its actual observation region.
    /// Gap/padding/invalid points reject the entire drag before input is pressed.
    pub fn from_image(
        mapping: &ObservationMapping,
        waypoints: &[ImagePoint],
        current: Generation,
    ) -> Result<Self, Error> {
        mapping.generation().require(current)?;
        if waypoints.len() < 2 {
            return Err(Error::TooFewWaypoints);
        }
        let points = waypoints
            .iter()
            .map(|p| mapping.map_image(*p, current))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::from_validated(mapping.generation(), &points))
    }
    fn from_validated(generation: Generation, points: &[NativePoint]) -> Self {
        Self {
            generation,
            segments: points
                .windows(2)
                .map(|p| NativeSegment {
                    start: p[0],
                    end: p[1],
                })
                .collect(),
        }
    }
    /// Generation whose native endpoint facts were validated.
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// Native continuous segment endpoints, with no injected timing policy.
    pub fn segments(&self) -> &[NativeSegment] {
        &self.segments
    }
    /// Sample an INTERNAL drag move at exact step/steps in [0,1]. Unlike map_native
    /// this may return a gap position. Never use it for clicks or user waypoints.
    /// Latest generation is required for EACH sample; stale => stop and clean up.
    /// Signed rational interpolation rounds toward the start on sub-unit steps.
    pub fn sample(
        &self,
        segment: usize,
        step: u32,
        steps: u32,
        current: Generation,
    ) -> Result<NativePoint, Error> {
        self.generation.require(current)?;
        if steps == 0 || step > steps {
            return Err(Error::InvalidInterpolation { step, steps });
        }
        let segment = self
            .segments
            .get(segment)
            .ok_or(Error::SegmentNotFound { index: segment })?;
        Ok(NativePoint {
            x: interpolate(segment.start.x, segment.end.x, step, steps)?,
            y: interpolate(segment.start.y, segment.end.y, step, steps)?,
        })
    }
}
fn interpolate(start: i32, end: i32, step: u32, steps: u32) -> Result<i32, Error> {
    let delta = i128::from(end) - i128::from(start);
    let product = delta
        .checked_mul(i128::from(step))
        .ok_or(Error::ArithmeticOverflow {
            operation: "drag delta product",
        })?;
    let value = i128::from(start) + product / i128::from(steps);
    i32::try_from(value).map_err(|_| Error::ArithmeticOverflow {
        operation: "drag sample i32",
    })
}
