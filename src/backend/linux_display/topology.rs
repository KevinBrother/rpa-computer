//! RandR 1.5 monitor rectangles are in root framebuffer pixels, including
//! transformed scanout and panning. Never reconstruct these from Xft DPI or
//! divide CRTC mode sizes by a font scale.
use crate::backend::capture::{expected_capture_size_for_platform, primary_display_index};
use rpa_display_topology::{DisplayDescriptor, NativeRect, PixelSize, Rotation};
use rpa_windows_display::DisplayError;

pub(super) const MAX_DISPLAYS: usize = 64;
pub(super) const MAX_OUTPUTS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Monitor {
    pub name: u32,
    pub primary: bool,
    pub bounds: NativeRect,
    pub outputs: Vec<u32>,
    pub rotation: Rotation,
}

pub(super) fn descriptors(
    root: u32,
    root_size: PixelSize,
    monitors: &[Monitor],
) -> Result<Vec<DisplayDescriptor>, DisplayError> {
    if monitors.len() > MAX_DISPLAYS {
        return Err(DisplayError::EnumerationLimit);
    }
    let flags: Vec<_> = monitors.iter().map(|monitor| monitor.primary).collect();
    let primary = primary_display_index(&flags).map_err(|_| {
        if flags.iter().filter(|primary| **primary).count() > 1 {
            DisplayError::UnsupportedDisplay {
                reason: "ambiguous X11 primary: multiple active primary monitors",
            }
        } else {
            DisplayError::UnsupportedDisplay {
                reason: "ambiguous X11 primary: no unique primary among active monitors",
            }
        }
    })?;
    let mut result = Vec::with_capacity(monitors.len());
    for (index, monitor) in monitors.iter().enumerate() {
        validate_bounds(monitor.bounds, root_size)?;
        if monitor.name == 0
            || monitors[..index]
                .iter()
                .any(|other| other.name == monitor.name)
        {
            return Err(DisplayError::UnsupportedDisplay {
                reason: "invalid or duplicate RandR monitor identity",
            });
        }
        let mut outputs = monitor.outputs.clone();
        outputs.sort_unstable();
        if outputs.len() > MAX_OUTPUTS
            || outputs.contains(&0)
            || outputs.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(DisplayError::UnsupportedDisplay {
                reason: "invalid RandR monitor output identities",
            });
        }
        // Lossless IDs: no enumeration index, connection-local counter, lossy
        // hash or DPI-dependent identity. Output membership changes are visible.
        let mut id = format!("x11:r{root:08x}:m{:08x}", monitor.name);
        for output in outputs {
            use std::fmt::Write;
            write!(id, ":{output:08x}").expect("writing into String cannot fail");
        }
        if id.len() > 128 {
            return Err(DisplayError::InvalidDisplayId { id });
        }
        let (width, height) = expected_capture_size_for_platform(
            "linux",
            monitor.bounds.width,
            monitor.bounds.height,
            1.0,
        );
        result.push(DisplayDescriptor {
            id,
            is_primary: index == primary,
            native_bounds: monitor.bounds,
            capture_size: PixelSize { width, height },
            scale: 1.0,
            rotation: monitor.rotation,
        });
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

pub(super) fn validate_bounds(bounds: NativeRect, root: PixelSize) -> Result<(), DisplayError> {
    // X11 root drawable coordinates cannot be negative. Also verify protocol
    // field ranges BEFORE narrowing them for GetImage, never clamp or wrap.
    if bounds.width == 0
        || bounds.height == 0
        || bounds.x < 0
        || bounds.y < 0
        || i16::try_from(bounds.x).is_err()
        || i16::try_from(bounds.y).is_err()
        || u16::try_from(bounds.width).is_err()
        || u16::try_from(bounds.height).is_err()
        || bounds.x as u64 + u64::from(bounds.width) > u64::from(root.width)
        || bounds.y as u64 + u64::from(bounds.height) > u64::from(root.height)
    {
        return Err(DisplayError::UnsupportedDisplay {
            reason: "RandR monitor lies outside the addressable X11 root drawable",
        });
    }
    Ok(())
}
