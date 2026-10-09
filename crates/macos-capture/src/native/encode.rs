use crate::flight::Flight;
use crate::request::{validate_dimensions, MAX_RAW_BYTES};
use crate::{
    validate_png_header, CaptureError, CapturedImage, ErrorKind, Result, Stage, MAX_PNG_BYTES,
};
use objc2_core_foundation::{CFMutableData, CFString};
use objc2_core_graphics::CGImage;
use objc2_image_io::CGImageDestination;
use std::time::Instant;

/// Native ImageIO consumes the original CGImage with its actual stride/color
/// space. No manual pixel copy, row inversion, alpha fill or overlay redaction.
pub(super) fn png(image: &CGImage, flight: &Flight<CapturedImage>) -> Result<CapturedImage> {
    let invalid = CaptureError::new(ErrorKind::InvalidImage, Stage::Encode);
    let width = u32::try_from(CGImage::width(Some(image))).map_err(|_| invalid)?;
    let height = u32::try_from(CGImage::height(Some(image))).map_err(|_| invalid)?;
    validate_dimensions(width, height, ErrorKind::InvalidImage)?;
    let raw_size = CGImage::bytes_per_row(Some(image))
        .checked_mul(height as usize)
        .ok_or(invalid)?;
    if raw_size == 0 || raw_size > MAX_RAW_BYTES {
        return Err(invalid);
    }
    if !flight.may_continue_at(Instant::now()) {
        return Err(CaptureError::new(ErrorKind::Timeout, Stage::Encode));
    }
    let data = CFMutableData::new(None, 0)
        .ok_or(CaptureError::new(ErrorKind::EncodingFailed, Stage::Encode))?;
    let png_type = CFString::from_str("public.png");
    // SAFETY: new exclusive mutable data, real PNG UTI, exactly one CGImage, no
    // untyped properties. ImageIO preserves the source image's native color data.
    let destination = unsafe { CGImageDestination::with_data(&data, &png_type, 1, None) }
        .ok_or(CaptureError::new(ErrorKind::EncodingFailed, Stage::Encode))?;
    unsafe {
        destination.add_image(image, None);
    }
    if !unsafe { destination.finalize() } {
        return Err(CaptureError::new(ErrorKind::EncodingFailed, Stage::Encode));
    }
    drop(destination); // No writer can mutate CFData while copying below.
    if !flight.may_continue_at(Instant::now()) {
        return Err(CaptureError::new(ErrorKind::Timeout, Stage::Encode));
    }
    let length = usize::try_from(data.length()).map_err(|_| invalid)?;
    if length == 0 || length > MAX_PNG_BYTES {
        return Err(invalid);
    }
    let png = data.to_vec();
    let (width, height) = validate_png_header(&png, width, height)?;
    Ok(CapturedImage { png, width, height })
}
