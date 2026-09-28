//! PNG decoding, honest dimensions, bounded downscaling, and image→native
//! coordinate mapping.
//!
//! The metadata the model receives (`width_px`/`height_px`) always matches the
//! *actual decoded dimensions* of the exact PNG bytes returned in
//! `Reply.image_png`. Mapping uses the capture dimensions of the observation
//! the step is based on, never a re-measured guess.

use crate::runtime::error::{codes, ToolError};

/// Smallest image dimension we will ever produce; guards against degenerate
/// sizes that would divide by zero during coordinate mapping.
pub const MIN_IMAGE_DIM: u32 = 16;
/// Hard ceiling on accepted capture dimensions (also protects memory).
pub const MAX_CAPTURE_DIM: u32 = 16384;

/// Header of a decoded PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngInfo {
    pub width: u32,
    pub height: u32,
}

/// Decode a PNG and report its actual pixel dimensions. Never trusts the
/// caller's claimed size. Decoding is bounded so a hostile or corrupt
/// capture cannot exhaust memory.
pub fn decode_png(png: &[u8]) -> Result<PngInfo, ToolError> {
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(MAX_CAPTURE_DIM);
    limits.max_image_height = Some(MAX_CAPTURE_DIM);
    let mut reader = image::io::Reader::with_format(
        std::io::BufReader::new(std::io::Cursor::new(png)),
        image::ImageFormat::Png,
    );
    reader.limits(limits);
    let img = reader.decode().map_err(|e| {
        ToolError::new(
            codes::CAPTURE_ERROR,
            format!("capture is not a decodable PNG: {e}"),
        )
    })?;
    let width = img.width();
    let height = img.height();
    if width == 0 || height == 0 {
        return Err(ToolError::new(
            codes::CAPTURE_ERROR,
            "capture has zero dimensions",
        ));
    }
    Ok(PngInfo { width, height })
}

/// Compute the proportional target size fitting `src` within `max`, never
/// below `MIN_IMAGE_DIM` per axis, never upscaling.
pub fn fit_within(src: (u32, u32), max: (u32, u32)) -> (u32, u32) {
    let (w, h) = src;
    if w == 0 || h == 0 {
        return (MIN_IMAGE_DIM, MIN_IMAGE_DIM);
    }
    if w <= max.0 && h <= max.1 {
        return (w.max(MIN_IMAGE_DIM), h.max(MIN_IMAGE_DIM));
    }
    let s = f64::min(max.0 as f64 / w as f64, max.1 as f64 / h as f64);
    let tw = ((w as f64 * s).round() as u32).clamp(MIN_IMAGE_DIM, max.0.max(MIN_IMAGE_DIM));
    let th = ((h as f64 * s).round() as u32).clamp(MIN_IMAGE_DIM, max.1.max(MIN_IMAGE_DIM));
    (tw, th)
}

/// Downscale a captured PNG to fit within `max`, preserving aspect ratio.
/// Returns the final bytes plus their *actual* decoded dimensions.
pub fn downscale(png: &[u8], max: (u32, u32)) -> Result<(Vec<u8>, PngInfo), ToolError> {
    let info = decode_png(png)?;
    let (max_w, max_h) = (max.0.max(MIN_IMAGE_DIM), max.1.max(MIN_IMAGE_DIM));
    if info.width <= max_w && info.height <= max_h {
        return Ok((png.to_vec(), info));
    }
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| ToolError::new(codes::CAPTURE_ERROR, format!("capture decode failed: {e}")))?;
    let (tw, th) = fit_within((info.width, info.height), (max_w, max_h));
    let resized = img.resize_exact(tw, th, image::imageops::FilterType::Triangle);
    let mut buf = std::io::Cursor::new(Vec::new());
    resized
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| ToolError::new(codes::CAPTURE_ERROR, format!("resized encode failed: {e}")))?;
    let bytes = buf.into_inner();
    // Verify by decoding what we produced: declared metadata must equal the
    // actual encoded dimensions, not the resize request.
    let actual = decode_png(&bytes)?;
    Ok((bytes, actual))
}

/// A deterministic, pure mapping from observation-image pixels to native
/// backend input coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordMap {
    pub image_width: u32,
    pub image_height: u32,
    pub input_origin: (i32, i32),
    pub input_size: (u32, u32),
}

impl CoordMap {
    pub fn new(
        image: PngInfo,
        input_origin: (i32, i32),
        input_size: (u32, u32),
    ) -> Result<Self, ToolError> {
        if image.width < MIN_IMAGE_DIM
            || image.height < MIN_IMAGE_DIM
            || input_size.0 == 0
            || input_size.1 == 0
        {
            return Err(ToolError::new(
                codes::GEOMETRY_CHANGED,
                format!(
                    "degenerate geometry: image {}x{}, input {:?}",
                    image.width, image.height, input_size
                ),
            ));
        }
        Ok(Self {
            image_width: image.width,
            image_height: image.height,
            input_origin,
            input_size,
        })
    }

    /// Map one image pixel coordinate to absolute native backend units.
    pub fn to_native(&self, pos: [i32; 2]) -> (i32, i32) {
        let sx = self.input_size.0 as f64 / self.image_width as f64;
        let sy = self.input_size.1 as f64 / self.image_height as f64;
        (
            self.input_origin.0 + (pos[0] as f64 * sx).round() as i32,
            self.input_origin.1 + (pos[1] as f64 * sy).round() as i32,
        )
    }

    /// Check an image coordinate lies inside the image it claims to be on.
    pub fn contains(&self, pos: [i32; 2]) -> bool {
        pos[0] >= 0
            && pos[1] >= 0
            && (pos[0] as u32) < self.image_width
            && (pos[1] as u32) < self.image_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([7, 8, 9, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn decode_reports_actual_dimensions() {
        let png = make_png(320, 200);
        let info = decode_png(&png).unwrap();
        assert_eq!(
            info,
            PngInfo {
                width: 320,
                height: 200
            }
        );
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode_png(b"not a png").is_err());
        assert!(decode_png(&[]).is_err());
    }

    #[test]
    fn downscale_preserves_aspect_and_reports_actual_size() {
        // Aspect ratio is preserved and both axes fit inside the bound; the
        // width is rounded to the nearest whole pixel rather than forced to
        // the bound (which would distort the image).
        let png = make_png(1600, 900);
        let (bytes, info) = downscale(&png, (1366, 768)).unwrap();
        assert_eq!(info.height, 768);
        assert!(info.width <= 1366);
        let expected_w = (1600.0f64 * (768.0f64 / 900.0f64)).round() as u32; // 1365
        assert_eq!(info.width, expected_w);
        // Returned metadata must equal the decoded size of the returned bytes.
        assert_eq!(decode_png(&bytes).unwrap(), info);
    }

    #[test]
    fn downscale_keeps_small_images_unchanged() {
        let png = make_png(800, 600);
        let (bytes, info) = downscale(&png, (1366, 768)).unwrap();
        assert_eq!(
            info,
            PngInfo {
                width: 800,
                height: 600
            }
        );
        assert_eq!(bytes, png);
    }

    #[test]
    fn fit_never_produces_zero() {
        assert_eq!(fit_within((16384, 1), (1366, 768)), (1366, MIN_IMAGE_DIM));
        let (w, h) = fit_within((1, 16384), (1366, 768));
        assert_eq!((w, h), (MIN_IMAGE_DIM, 768));
    }

    #[test]
    fn coordinate_mapping_uses_capture_dimensions() {
        // Image is the downscaled 1366x768; native space is 3008x1692 at
        // origin (10, 20).
        let map = CoordMap::new(
            PngInfo {
                width: 1366,
                height: 768,
            },
            (10, 20),
            (3008, 1692),
        )
        .unwrap();
        let (x, y) = map.to_native([0, 0]);
        assert_eq!((x, y), (10, 20));
        let (x, y) = map.to_native([1365, 767]);
        // Near the far edge: within native space, offset from origin.
        assert!(x > 10 && x <= 10 + 3008);
        assert!(y > 20 && y <= 20 + 1692);
        // Center maps proportionally.
        let (cx, cy) = map.to_native([683, 384]);
        assert!((cx - (10 + 3008 / 2)).abs() <= 2);
        assert!((cy - (20 + 1692 / 2)).abs() <= 2);
    }

    #[test]
    fn degenerate_geometry_rejected() {
        assert!(CoordMap::new(
            PngInfo {
                width: 0,
                height: 10
            },
            (0, 0),
            (100, 100)
        )
        .is_err());
        assert!(CoordMap::new(
            PngInfo {
                width: 100,
                height: 100
            },
            (0, 0),
            (0, 100)
        )
        .is_err());
    }
}
