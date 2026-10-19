//! Checked ZPixmap layout and TrueColor conversion. Validation and permit
//! admission happen before GetImage; only then may the reply/RGBA be allocated.
use rpa_display_topology::PixelSize;
use rpa_windows_display::{DisplayError, RgbaFrame, SourcePermit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Format {
    pub bits_per_pixel: u8,
    pub scanline_pad: u8,
    pub little_endian: bool,
    pub red: u32,
    pub green: u32,
    pub blue: u32,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct Layout {
    pub stride: usize,
    pub wire_bytes: usize,
    rgba_bytes: usize,
    size: PixelSize,
    format: Format,
}
fn invalid(reason: &'static str) -> DisplayError {
    DisplayError::UnsupportedDisplay { reason }
}
fn overflow() -> DisplayError {
    DisplayError::ArithmeticOverflow {
        operation: "X11 image layout",
    }
}

impl Format {
    pub fn layout(self, size: PixelSize) -> Result<Layout, DisplayError> {
        if size.width == 0 || size.height == 0 {
            return Err(DisplayError::InvalidFrameSize);
        }
        if ![16, 24, 32].contains(&self.bits_per_pixel) || ![8, 16, 32].contains(&self.scanline_pad)
        {
            return Err(invalid("unsupported X11 TrueColor pixel/scanline format"));
        }
        let combined = self.red | self.green | self.blue;
        if [self.red, self.green, self.blue].iter().any(|mask| {
            if *mask == 0 {
                return true;
            }
            let channel = mask >> mask.trailing_zeros();
            channel & channel.wrapping_add(1) != 0
        }) || self.red & self.green != 0
            || self.red & self.blue != 0
            || self.green & self.blue != 0
            || (self.bits_per_pixel < 32 && combined >> self.bits_per_pixel != 0)
        {
            return Err(invalid("invalid or overlapping X11 TrueColor masks"));
        }
        let row_bits = u64::from(size.width)
            .checked_mul(u64::from(self.bits_per_pixel))
            .ok_or_else(overflow)?;
        let stride = row_bits
            .div_ceil(u64::from(self.scanline_pad))
            .checked_mul(u64::from(self.scanline_pad) / 8)
            .ok_or_else(overflow)?;
        let wire = stride
            .checked_mul(u64::from(size.height))
            .ok_or_else(overflow)?;
        let rgba = u64::from(size.width)
            .checked_mul(u64::from(size.height))
            .and_then(|area| area.checked_mul(4))
            .ok_or_else(overflow)?;
        // X replies are padded to 4 bytes. Count padding as native payload too.
        let wire = wire.checked_add(3).ok_or_else(overflow)? & !3;
        if wire > rgba {
            return Err(invalid(
                "X11 reply padding exceeds the engine's native source permit",
            ));
        }
        Ok(Layout {
            stride: usize::try_from(stride).map_err(|_| overflow())?,
            wire_bytes: usize::try_from(wire).map_err(|_| overflow())?,
            rgba_bytes: usize::try_from(rgba).map_err(|_| overflow())?,
            size,
            format: self,
        })
    }
}
impl Layout {
    pub fn require(self, permit: SourcePermit) -> Result<Self, DisplayError> {
        if permit.size() != self.size
            || permit.bytes() != self.rgba_bytes as u64
            || self.wire_bytes as u64 > permit.bytes()
        {
            return Err(invalid(
                "X11 source dimensions do not match the engine's allocation permit",
            ));
        }
        Ok(self)
    }
    pub fn decode(self, bytes: &[u8]) -> Result<RgbaFrame, DisplayError> {
        if bytes.len() != self.wire_bytes {
            return Err(DisplayError::InvalidFrameLength {
                expected: self.wire_bytes as u64,
                actual: bytes.len() as u64,
            });
        }
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(self.rgba_bytes)
            .map_err(|_| DisplayError::AllocationFailed {
                bytes: self.rgba_bytes as u64,
            })?;
        let bytes_per_pixel = usize::from(self.format.bits_per_pixel / 8);
        for y in 0..self.size.height as usize {
            for x in 0..self.size.width as usize {
                let offset = y * self.stride + x * bytes_per_pixel;
                let pixel = &bytes[offset..offset + bytes_per_pixel];
                let mut value = 0u32;
                if self.format.little_endian {
                    for (index, byte) in pixel.iter().enumerate() {
                        value |= u32::from(*byte) << (index * 8);
                    }
                } else {
                    for byte in pixel {
                        value = (value << 8) | u32::from(*byte);
                    }
                }
                for mask in [self.format.red, self.format.green, self.format.blue] {
                    let shift = mask.trailing_zeros();
                    let maximum = u64::from(mask >> shift);
                    let sample = u64::from((value & mask) >> shift);
                    rgba.push(((sample * 255 + maximum / 2) / maximum) as u8);
                }
                rgba.push(255);
            }
        }
        RgbaFrame::new(self.size, rgba)
    }
}
