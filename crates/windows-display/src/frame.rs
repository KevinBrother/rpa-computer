use crate::{error::overflow, DisplayError};
use rpa_display_topology::PixelSize;

/// Input DIB row orientation. Windows capture always requests TopDown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowOrder {
    /// First row is the top of the display.
    TopDown,
    /// First row is the bottom; conversion reverses rows in place.
    BottomUp,
}

/// Packed, top-down RGBA8 frame. No hidden row stride or premultiplication.
/// Windows GDI capture normalizes undefined DIB alpha to opaque 255.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaFrame {
    size: PixelSize,
    pixels: Vec<u8>,
}
impl RgbaFrame {
    /// Validate an existing owned, tightly packed RGBA payload.
    pub fn new(size: PixelSize, pixels: Vec<u8>) -> Result<Self, DisplayError> {
        let expected = rgba_bytes(size)?;
        if pixels.len() as u64 != expected {
            return Err(DisplayError::InvalidFrameLength {
                expected,
                actual: pixels.len() as u64,
            });
        }
        Ok(Self { size, pixels })
    }
    /// Convert packed BGRA/BGRX to top-down opaque RGBA without a second buffer.
    /// A stride mismatch is rejected, not silently reinterpreted or cropped.
    pub fn from_bgra32(
        size: PixelSize,
        stride: u64,
        mut bytes: Vec<u8>,
        order: RowOrder,
    ) -> Result<Self, DisplayError> {
        let expected_stride = u64::from(size.width)
            .checked_mul(4)
            .ok_or_else(|| overflow("BGRA stride"))?;
        if stride != expected_stride {
            return Err(DisplayError::InvalidStride {
                expected: expected_stride,
                actual: stride,
            });
        }
        let expected = rgba_bytes(size)?;
        if bytes.len() as u64 != expected {
            return Err(DisplayError::InvalidFrameLength {
                expected,
                actual: bytes.len() as u64,
            });
        }
        if order == RowOrder::BottomUp {
            let stride = usize::try_from(stride).map_err(|_| overflow("BGRA stride usize"))?;
            for row in 0..size.height as usize / 2 {
                let other = size.height as usize - 1 - row;
                for x in 0..stride {
                    bytes.swap(row * stride + x, other * stride + x);
                }
            }
        }
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        Ok(Self {
            size,
            pixels: bytes,
        })
    }
    /// True packed frame size.
    pub fn size(&self) -> PixelSize {
        self.size
    }
    /// Top-down RGBA8 bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// Transfer ownership without another image copy.
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
    pub(crate) fn black(size: PixelSize) -> Result<Self, DisplayError> {
        let mut pixels = allocate_zeroed(rgba_bytes(size)?)?;
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        Ok(Self { size, pixels })
    }
    pub(crate) fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }
}

pub(crate) fn rgba_bytes(size: PixelSize) -> Result<u64, DisplayError> {
    if size.width == 0 || size.height == 0 {
        return Err(DisplayError::InvalidFrameSize);
    }
    let bytes = u64::from(size.width)
        .checked_mul(u64::from(size.height))
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| overflow("RGBA byte count"))?;
    let _ = isize::try_from(bytes).map_err(|_| overflow("RGBA addressable length"))?;
    Ok(bytes)
}
pub(crate) fn allocate_zeroed(bytes: u64) -> Result<Vec<u8>, DisplayError> {
    let length = usize::try_from(bytes).map_err(|_| overflow("allocation usize"))?;
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(length)
        .map_err(|_| DisplayError::AllocationFailed { bytes })?;
    buffer.resize(length, 0);
    Ok(buffer)
}
