use crate::capture::validate_regions;
use crate::geometry::floor_ratio;
use crate::{
    CapturePlan, Error, Generation, ImagePoint, NativePoint, NativeRect, NativeUnit, PixelRect,
    PixelSize,
};

/// A selected display's region in the ACTUAL decoded observation PNG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationRegion {
    /// Selected runtime display ID.
    pub display_id: String,
    /// Corresponding native bounds; not inferred from overall image dimensions.
    pub native_bounds: NativeRect,
    /// Half-open click-valid rectangle in the actual observation PNG.
    pub image_rect: PixelRect,
}

/// Immutable per-region map bound to a capture generation and actual PNG size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationMapping {
    generation: Generation,
    unit: NativeUnit,
    size: PixelSize,
    content_rect: PixelRect,
    regions: Vec<ObservationRegion>,
}
impl ObservationMapping {
    /// Build from the actual decoded PNG dimensions. None means the whole PNG
    /// contains the complete rescaled layout; Some declares explicit letterboxing.
    /// Cropping, arbitrary tile rearrangement or stale pixels are NOT supported.
    /// The compositor must independently resize whole source tiles into these
    /// exact rescaled rects and keep gaps/padding blank. Arbitrary whole-canvas
    /// filtering across tile edges is not this protocol; metadata cannot repair it.
    pub fn new(
        plan: &CapturePlan,
        actual_size: PixelSize,
        content: Option<PixelRect>,
    ) -> Result<Self, Error> {
        if !actual_size.valid() {
            return Err(Error::InvalidObservationSize);
        }
        actual_size.area()?;
        let content_rect = content.unwrap_or_else(|| PixelRect::full(actual_size));
        if !content_rect.within(actual_size) {
            return Err(Error::InvalidContentRect);
        }
        let mut regions = Vec::with_capacity(plan.tiles().len());
        for tile in plan.tiles() {
            let r = tile.composite_rect;
            let left = rescale_edge(
                u64::from(r.x),
                content_rect.width,
                plan.size().width,
                content_rect.x,
            )?;
            let right = rescale_edge(
                r.right(),
                content_rect.width,
                plan.size().width,
                content_rect.x,
            )?;
            let top = rescale_edge(
                u64::from(r.y),
                content_rect.height,
                plan.size().height,
                content_rect.y,
            )?;
            let bottom = rescale_edge(
                r.bottom(),
                content_rect.height,
                plan.size().height,
                content_rect.y,
            )?;
            let image_rect = PixelRect {
                x: left,
                y: top,
                width: right.checked_sub(left).ok_or(Error::ArithmeticOverflow {
                    operation: "observation region width",
                })?,
                height: bottom.checked_sub(top).ok_or(Error::ArithmeticOverflow {
                    operation: "observation region height",
                })?,
            };
            if image_rect.width == 0 || image_rect.height == 0 {
                return Err(Error::CollapsedRegion {
                    id: tile.display_id.clone(),
                });
            }
            if !image_rect.within(actual_size) {
                return Err(Error::InvalidContentRect);
            }
            regions.push(ObservationRegion {
                display_id: tile.display_id.clone(),
                native_bounds: tile.native_bounds,
                image_rect,
            });
        }
        validate_regions(
            regions
                .iter()
                .map(|r| (&r.display_id, r.native_bounds, r.image_rect)),
        )?;
        Ok(Self {
            generation: plan.generation(),
            unit: plan.unit(),
            size: actual_size,
            content_rect,
            regions,
        })
    }
    /// Exact topology generation bound to the observation.
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// Explicit native unit returned by coordinate mapping.
    pub fn unit(&self) -> NativeUnit {
        self.unit
    }
    /// Actual decoded PNG dimensions.
    pub fn size(&self) -> PixelSize {
        self.size
    }
    /// Content placement inside the actual PNG, excluding external padding.
    pub fn content_rect(&self) -> PixelRect {
        self.content_rect
    }
    /// Regions for composing resized source tiles and exposing trusted metadata.
    pub fn regions(&self) -> &[ObservationRegion] {
        &self.regions
    }
    /// Validate image point BEFORE quantization, reject gaps/padding, then map
    /// only through its selected display. Runtime must pass its latest generation.
    pub fn map_image(&self, point: ImagePoint, current: Generation) -> Result<NativePoint, Error> {
        self.generation.require(current)?;
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x < 0.0
            || point.y < 0.0
            || point.x >= f64::from(self.size.width)
            || point.y >= f64::from(self.size.height)
        {
            return Err(Error::InvalidImagePoint { point });
        }
        let region = self
            .regions
            .iter()
            .find(|r| r.image_rect.contains(point))
            .ok_or(Error::ImageGapOrPadding { point })?;
        Ok(NativePoint {
            x: native_axis(
                point.x - f64::from(region.image_rect.x),
                region.image_rect.width,
                region.native_bounds.x,
                region.native_bounds.width,
            )?,
            y: native_axis(
                point.y - f64::from(region.image_rect.y),
                region.image_rect.height,
                region.native_bounds.y,
                region.native_bounds.height,
            )?,
        })
    }
    /// Native user-specified points remain restricted to the selected displays.
    /// This method deliberately does NOT allow click/waypoint gaps.
    pub fn map_native(
        &self,
        point: NativePoint,
        current: Generation,
    ) -> Result<NativePoint, Error> {
        self.generation.require(current)?;
        if self.regions.iter().any(|r| r.native_bounds.contains(point)) {
            Ok(point)
        } else {
            Err(Error::NativePointOutsideSelection { point })
        }
    }
}

fn rescale_edge(value: u64, destination: u32, source: u32, offset: u32) -> Result<u32, Error> {
    floor_ratio(value, u64::from(destination), u64::from(source))?
        .checked_add(offset)
        .ok_or(Error::ArithmeticOverflow {
            operation: "observation edge offset",
        })
}
fn native_axis(local: f64, pixels: u32, origin: i32, units: u32) -> Result<i32, Error> {
    // Caller has already proven finite local in [0,pixels). Floor prevents the
    // old round() right/bottom escape. Only this final LEGAL-point quantization
    // may limit a floating-point last-bit result to the display's final unit.
    let offset = (local / f64::from(pixels) * f64::from(units))
        .floor()
        .min(f64::from(units - 1)) as i64;
    let absolute = i64::from(origin)
        .checked_add(offset)
        .ok_or(Error::ArithmeticOverflow {
            operation: "mapped native coordinate",
        })?;
    i32::try_from(absolute).map_err(|_| Error::ArithmeticOverflow {
        operation: "mapped native i32",
    })
}
