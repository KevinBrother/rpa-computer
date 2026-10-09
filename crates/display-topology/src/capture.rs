use crate::geometry::floor_ratio;
use crate::{
    DisplayDescriptor, Error, Generation, NativeRect, NativeUnit, PixelRect, PixelSize, Selection,
    TopologySnapshot,
};

/// Explicit maximum canvas dimensions and pixel count. Limits never allocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureBudget {
    max_width: u32,
    max_height: u32,
    max_pixels: u64,
}
impl CaptureBudget {
    /// Validate nonzero limits and their checked area before narrowing dimensions
    /// to the PNG protocol. Oversized requests are errors, not saturated limits.
    pub fn new(max_width: u64, max_height: u64, max_pixels: u64) -> Result<Self, Error> {
        if max_width == 0 || max_height == 0 || max_pixels == 0 {
            return Err(Error::InvalidBudget);
        }
        max_width
            .checked_mul(max_height)
            .ok_or(Error::ArithmeticOverflow {
                operation: "budget dimensions product",
            })?;
        Ok(Self {
            max_width: u32::try_from(max_width).map_err(|_| Error::BudgetDimensionsTooLarge)?,
            max_height: u32::try_from(max_height).map_err(|_| Error::BudgetDimensionsTooLarge)?,
            max_pixels,
        })
    }
    /// Maximum canvas width in pixels.
    pub fn max_width(self) -> u32 {
        self.max_width
    }
    /// Maximum canvas height in pixels.
    pub fn max_height(self) -> u32 {
        self.max_height
    }
    /// Maximum canvas area in pixels (not bytes).
    pub fn max_pixels(self) -> u64 {
        self.max_pixels
    }
    fn fits(self, size: PixelSize) -> Result<bool, Error> {
        Ok(size.valid()
            && size.width <= self.max_width
            && size.height <= self.max_height
            && size.area()? <= self.max_pixels)
    }
}

/// Exact common composite pixels per native unit (no floating-point layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Density {
    /// Numerator of the common density ratio.
    pub numerator: u64,
    /// Nonzero denominator of the common density ratio.
    pub denominator: u64,
}

/// Desktop native bounding box; a full signed-i32 span needs u64, not u32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopLayout {
    /// Native minimum x.
    pub x: i32,
    /// Native minimum y.
    pub y: i32,
    /// Native bounding width including gaps.
    pub width: u64,
    /// Native bounding height including gaps.
    pub height: u64,
    /// Common density for every desktop tile.
    pub density: Density,
}

/// Source-to-composite compositor instruction and explicit per-display mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureTile {
    /// Exact selected runtime display ID.
    pub display_id: String,
    /// Source must be captured at this actual, already oriented size.
    pub source_size: PixelSize,
    /// Native input rectangle; never replaced by screenshot dimensions.
    pub native_bounds: NativeRect,
    /// Half-open destination rectangle. Resize the entire source into this rect.
    pub composite_rect: PixelRect,
}

/// Validated capture protocol. Immutable getters prevent metadata tampering.
/// Builds only instructions; pixel allocation/capture remains a backend job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturePlan {
    generation: Generation,
    unit: NativeUnit,
    selection: Selection,
    budget: CaptureBudget,
    size: PixelSize,
    layout: Option<DesktopLayout>,
    tiles: Vec<CaptureTile>,
}
impl CapturePlan {
    /// Plan primary/explicit single surface or spatial Desktop within the budget.
    /// Desktop rejects mirrors/overlaps. Explicit single surfaces may overlap
    /// unselected displays because their source and mapping are unambiguous.
    pub fn new(
        snapshot: &TopologySnapshot,
        selection: Selection,
        budget: CaptureBudget,
    ) -> Result<Self, Error> {
        let displays = snapshot.select(&selection)?;
        let (size, layout, tiles) = if selection == Selection::Desktop {
            desktop(&displays, budget)?
        } else {
            let d = displays[0];
            let size = fit_canvas(
                u64::from(d.capture_size.width),
                u64::from(d.capture_size.height),
                1,
                1,
                budget,
            )?;
            (
                size,
                None,
                vec![CaptureTile {
                    display_id: d.id.clone(),
                    source_size: d.capture_size,
                    native_bounds: d.native_bounds,
                    composite_rect: PixelRect::full(size),
                }],
            )
        };
        validate_regions(
            tiles
                .iter()
                .map(|tile| (&tile.display_id, tile.native_bounds, tile.composite_rect)),
        )?;
        Ok(Self {
            generation: snapshot.generation(),
            unit: snapshot.unit(),
            selection,
            budget,
            size,
            layout,
            tiles,
        })
    }
    /// Exact topology token bound to these source/compositor instructions.
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// Explicit native input unit.
    pub fn unit(&self) -> NativeUnit {
        self.unit
    }
    /// Selection used for this plan.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    /// Budget that constrains the planned canvas (not a PNG memory allocation).
    pub fn budget(&self) -> CaptureBudget {
        self.budget
    }
    /// Composite PNG dimensions. Backend must encode exactly these dimensions.
    pub fn size(&self) -> PixelSize {
        self.size
    }
    /// Native layout/common density in Desktop mode; None for single sources.
    pub fn desktop_layout(&self) -> Option<DesktopLayout> {
        self.layout
    }
    /// Sources and destinations, deterministically sorted by opaque ID.
    pub fn tiles(&self) -> &[CaptureTile] {
        &self.tiles
    }
    /// Backend/Runtime verifies a freshly enumerated generation before capture
    /// and again before publishing the observation. A stale capture is discarded.
    pub fn require_generation(&self, current: Generation) -> Result<(), Error> {
        self.generation.require(current)
    }
    /// Backend validates DECODED source dimensions before resizing/compositing.
    /// This cannot verify pixels, orientation or true display identity for it.
    pub fn validate_source(&self, id: &str, actual: PixelSize) -> Result<(), Error> {
        let tile = self
            .tiles
            .iter()
            .find(|t| t.display_id == id)
            .ok_or_else(|| Error::DisplayNotSelected { id: id.into() })?;
        if tile.source_size != actual {
            return Err(Error::SourceSizeMismatch {
                id: id.into(),
                expected: tile.source_size,
                actual,
            });
        }
        Ok(())
    }
}

fn desktop(
    displays: &[&DisplayDescriptor],
    budget: CaptureBudget,
) -> Result<(PixelSize, Option<DesktopLayout>, Vec<CaptureTile>), Error> {
    for (i, a) in displays.iter().enumerate() {
        for b in &displays[i + 1..] {
            if a.native_bounds.overlaps(b.native_bounds) {
                return Err(Error::OverlappingDisplays {
                    first: a.id.clone(),
                    second: b.id.clone(),
                });
            }
        }
    }
    let x = displays
        .iter()
        .map(|d| d.native_bounds.x)
        .min()
        .ok_or(Error::EmptyTopology)?;
    let y = displays
        .iter()
        .map(|d| d.native_bounds.y)
        .min()
        .ok_or(Error::EmptyTopology)?;
    let right = displays
        .iter()
        .map(|d| d.native_bounds.right())
        .max()
        .ok_or(Error::EmptyTopology)?;
    let bottom = displays
        .iter()
        .map(|d| d.native_bounds.bottom())
        .max()
        .ok_or(Error::EmptyTopology)?;
    let width = u64::try_from(right - i64::from(x)).map_err(|_| Error::ArithmeticOverflow {
        operation: "desktop width",
    })?;
    let height = u64::try_from(bottom - i64::from(y)).map_err(|_| Error::ArithmeticOverflow {
        operation: "desktop height",
    })?;
    // Prefer the largest ACTUAL source/native ratio. Scale is a topology fact,
    // not a substitute for source pixel dimensions (especially under rotation).
    let mut preferred = (0u64, 1u64);
    for d in displays {
        for ratio in [
            (
                u64::from(d.capture_size.width),
                u64::from(d.native_bounds.width),
            ),
            (
                u64::from(d.capture_size.height),
                u64::from(d.native_bounds.height),
            ),
        ] {
            if u128::from(ratio.0) * u128::from(preferred.1)
                > u128::from(preferred.0) * u128::from(ratio.1)
            {
                preferred = ratio;
            }
        }
    }
    let size = fit_canvas(width, height, preferred.0, preferred.1, budget)?;
    let density = Density {
        numerator: u64::from(size.width.max(size.height)),
        denominator: width.max(height),
    };
    let layout = DesktopLayout {
        x,
        y,
        width,
        height,
        density,
    };
    let mut tiles = Vec::with_capacity(displays.len());
    for d in displays {
        let bounds = d.native_bounds;
        let left = quantize(i64::from(bounds.x) - i64::from(x), density)?;
        let top = quantize(i64::from(bounds.y) - i64::from(y), density)?;
        let right = quantize(bounds.right() - i64::from(x), density)?;
        let bottom = quantize(bounds.bottom() - i64::from(y), density)?;
        let rect = PixelRect {
            x: left,
            y: top,
            width: right.checked_sub(left).ok_or(Error::ArithmeticOverflow {
                operation: "tile width",
            })?,
            height: bottom.checked_sub(top).ok_or(Error::ArithmeticOverflow {
                operation: "tile height",
            })?,
        };
        if rect.width == 0 || rect.height == 0 {
            return Err(Error::CollapsedRegion { id: d.id.clone() });
        }
        if !rect.within(size) {
            return Err(Error::ArithmeticOverflow {
                operation: "tile outside planned canvas",
            });
        }
        tiles.push(CaptureTile {
            display_id: d.id.clone(),
            source_size: d.capture_size,
            native_bounds: bounds,
            composite_rect: rect,
        });
    }
    Ok((size, Some(layout), tiles))
}

fn quantize(offset: i64, density: Density) -> Result<u32, Error> {
    let offset = u64::try_from(offset).map_err(|_| Error::ArithmeticOverflow {
        operation: "negative composite offset",
    })?;
    floor_ratio(offset, density.numerator, density.denominator)
}

// Search a bounded number of canvas dimensions, never a native sparse buffer.
// Common density is k / longest_native_edge. Canvas axes are ceil(span*d).
// Longest axis is k exactly. Integer search <= 32 iterations even for u32::MAX.
fn fit_canvas(
    width: u64,
    height: u64,
    preferred_num: u64,
    preferred_den: u64,
    budget: CaptureBudget,
) -> Result<PixelSize, Error> {
    let longest = width.max(height);
    let preferred_edge = u128::from(longest)
        .checked_mul(u128::from(preferred_num))
        .and_then(|p| p.checked_div(u128::from(preferred_den)))
        .ok_or(Error::ArithmeticOverflow {
            operation: "preferred canvas density",
        })?;
    let dimension_limit = if width >= height {
        budget.max_width
    } else {
        budget.max_height
    };
    // min is a planning fit, never coordinate/request clamping.
    let mut hi = preferred_edge.min(u128::from(dimension_limit)) as u64;
    if hi == 0 {
        return Err(Error::BudgetTooSmall);
    }
    let mut lo = 0u64;
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        let size = size_at(width, height, mid, longest)?;
        if budget.fits(size)? {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 {
        return Err(Error::BudgetTooSmall);
    }
    size_at(width, height, lo, longest)
}
fn size_at(width: u64, height: u64, k: u64, longest: u64) -> Result<PixelSize, Error> {
    let axis = |span: u64| -> Result<u32, Error> {
        let product =
            u128::from(span)
                .checked_mul(u128::from(k))
                .ok_or(Error::ArithmeticOverflow {
                    operation: "canvas axis product",
                })?;
        let value = product.div_ceil(u128::from(longest));
        u32::try_from(value).map_err(|_| Error::ArithmeticOverflow {
            operation: "canvas axis conversion",
        })
    };
    Ok(PixelSize {
        width: axis(width)?,
        height: axis(height)?,
    })
}

pub(crate) fn validate_regions<'a>(
    regions: impl Iterator<Item = (&'a String, NativeRect, PixelRect)>,
) -> Result<(), Error> {
    let regions: Vec<_> = regions.collect();
    for (i, (id, native, region)) in regions.iter().enumerate() {
        if region.width == 0 || region.height == 0 {
            return Err(Error::CollapsedRegion { id: (*id).clone() });
        }
        for (other_id, other_native, other) in &regions[i + 1..] {
            if region.overlaps(*other) {
                return Err(Error::OverlappingRegions {
                    first: (*id).clone(),
                    second: (*other_id).clone(),
                });
            }
            // Do not erase a positive native gap and then claim it is preserved.
            // Every positive separating gap needs at least one blank pixel on
            // its axis, in both the plan and the actual observation.
            let lost_x_gap = (native.right() < i64::from(other_native.x)
                && region.right() >= u64::from(other.x))
                || (other_native.right() < i64::from(native.x)
                    && other.right() >= u64::from(region.x));
            let lost_y_gap = (native.bottom() < i64::from(other_native.y)
                && region.bottom() >= u64::from(other.y))
                || (other_native.bottom() < i64::from(native.y)
                    && other.bottom() >= u64::from(region.y));
            if lost_x_gap || lost_y_gap {
                return Err(Error::CollapsedGap {
                    first: (*id).clone(),
                    second: (*other_id).clone(),
                });
            }
        }
    }
    Ok(())
}
