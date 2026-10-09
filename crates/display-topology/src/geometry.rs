use crate::Error;

/// Integer position in the caller-declared native input unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativePoint {
    /// Absolute horizontal native position, including negative origins.
    pub x: i32,
    /// Absolute vertical native position, including negative origins.
    pub y: i32,
}

/// Continuous pixel-edge coordinate; valid image points satisfy 0 <= x < width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImagePoint {
    /// Horizontal coordinate. Not implicitly rounded or clamped at validation.
    pub x: f64,
    /// Vertical coordinate. Not implicitly rounded or clamped at validation.
    pub y: f64,
}

/// Half-open native bounds. The last valid integer point must fit in i32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeRect {
    /// Native left origin.
    pub x: i32,
    /// Native top origin.
    pub y: i32,
    /// Native width; NOT source capture pixels.
    pub width: u32,
    /// Native height; NOT source capture pixels.
    pub height: u32,
}
impl NativeRect {
    pub(crate) fn validate(self, id: &str) -> Result<(), Error> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::InvalidNativeSize { id: id.into() });
        }
        // End is exclusive: MAX+1 is valid when the final address is MAX.
        if self.right() > i64::from(i32::MAX) + 1 || self.bottom() > i64::from(i32::MAX) + 1 {
            return Err(Error::NativeBoundsOverflow { id: id.into() });
        }
        Ok(())
    }
    pub(crate) fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }
    pub(crate) fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
    pub(crate) fn contains(self, point: NativePoint) -> bool {
        i64::from(point.x) >= i64::from(self.x)
            && i64::from(point.x) < self.right()
            && i64::from(point.y) >= i64::from(self.y)
            && i64::from(point.y) < self.bottom()
    }
    pub(crate) fn overlaps(self, other: Self) -> bool {
        i64::from(self.x) < other.right()
            && i64::from(other.x) < self.right()
            && i64::from(self.y) < other.bottom()
            && i64::from(other.y) < self.bottom()
    }
}

/// Actual PNG/source/canvas dimensions, not guessed from native scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}
impl PixelSize {
    pub(crate) fn valid(self) -> bool {
        self.width > 0 && self.height > 0
    }
    pub(crate) fn area(self) -> Result<u64, Error> {
        u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .ok_or(Error::ArithmeticOverflow {
                operation: "pixel area",
            })
    }
}

/// Half-open integer pixel region in a composite or actual observation PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    /// Left pixel edge.
    pub x: u32,
    /// Top pixel edge.
    pub y: u32,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
}
impl PixelRect {
    pub(crate) fn full(size: PixelSize) -> Self {
        Self {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        }
    }
    pub(crate) fn right(self) -> u64 {
        u64::from(self.x) + u64::from(self.width)
    }
    pub(crate) fn bottom(self) -> u64 {
        u64::from(self.y) + u64::from(self.height)
    }
    pub(crate) fn within(self, size: PixelSize) -> bool {
        self.width > 0
            && self.height > 0
            && self.right() <= u64::from(size.width)
            && self.bottom() <= u64::from(size.height)
    }
    pub(crate) fn contains(self, point: ImagePoint) -> bool {
        point.x >= f64::from(self.x)
            && point.x < self.right() as f64
            && point.y >= f64::from(self.y)
            && point.y < self.bottom() as f64
    }
    pub(crate) fn overlaps(self, other: Self) -> bool {
        u64::from(self.x) < other.right()
            && u64::from(other.x) < self.right()
            && u64::from(self.y) < other.bottom()
            && u64::from(other.y) < self.bottom()
    }
}

pub(crate) fn floor_ratio(value: u64, numerator: u64, denominator: u64) -> Result<u32, Error> {
    let product =
        u128::from(value)
            .checked_mul(u128::from(numerator))
            .ok_or(Error::ArithmeticOverflow {
                operation: "region edge product",
            })?;
    let result = product
        .checked_div(u128::from(denominator))
        .ok_or(Error::ArithmeticOverflow {
            operation: "region edge division",
        })?;
    u32::try_from(result).map_err(|_| Error::ArithmeticOverflow {
        operation: "region edge conversion",
    })
}
