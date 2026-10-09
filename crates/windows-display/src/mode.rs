use crate::DisplayError;
use rpa_display_topology::{DisplayDescriptor, NativeRect, PixelSize, Rotation};

/// Copied documented OS facts, separated from Win32 pointers for pure tests.
#[derive(Debug, Clone)]
pub(crate) struct ModeFacts {
    pub bounds: NativeRect,
    pub mode_origin: (i32, i32),
    pub mode_size: PixelSize,
    pub orientation: u32,
    pub scale_percent: i32,
    pub primary: bool,
}
impl ModeFacts {
    pub fn descriptor(&self, id: String) -> Result<DisplayDescriptor, DisplayError> {
        if self.mode_size.width == 0
            || self.mode_size.height == 0
            || self.bounds.width != self.mode_size.width
            || self.bounds.height != self.mode_size.height
            || (self.bounds.x, self.bounds.y) != self.mode_origin
        {
            return Err(DisplayError::UnsupportedDisplay {
                reason: "physical mode and monitor bounds disagree",
            });
        }
        if self.scale_percent <= 0 {
            return Err(DisplayError::UnsupportedDisplay {
                reason: "GetScaleFactorForMonitor returned invalid scale",
            });
        }
        // Win32 DEVMODE DMDO values are COUNTER-clockwise; topology Rotation
        // is clockwise. Sources are already oriented, so convert the fact only.
        let rotation = match self.orientation {
            0 => Rotation::Deg0,
            1 => Rotation::Deg270,
            2 => Rotation::Deg180,
            3 => Rotation::Deg90,
            _ => {
                return Err(DisplayError::UnsupportedDisplay {
                    reason: "unrecognized DEVMODE display orientation",
                })
            }
        };
        Ok(DisplayDescriptor {
            id,
            is_primary: self.primary,
            native_bounds: self.bounds,
            capture_size: self.mode_size,
            scale: f64::from(self.scale_percent) / 100.0,
            rotation,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn facts() -> ModeFacts {
        ModeFacts {
            bounds: NativeRect {
                x: -1080,
                y: 0,
                width: 1080,
                height: 1920,
            },
            mode_origin: (-1080, 0),
            mode_size: PixelSize {
                width: 1080,
                height: 1920,
            },
            orientation: 1,
            scale_percent: 150,
            primary: false,
        }
    }
    #[test]
    fn physical_size_is_not_multiplied_by_scale_or_rotated_twice() {
        let d = facts().descriptor("a".into()).unwrap();
        assert_eq!(
            d.capture_size,
            PixelSize {
                width: 1080,
                height: 1920
            }
        );
        assert_eq!(d.native_bounds.x, -1080);
        assert_eq!(d.scale, 1.5);
        assert_eq!(d.rotation, Rotation::Deg270);
    }
    #[test]
    fn windows_counter_clockwise_modes_convert_to_topology_clockwise_facts() {
        for (orientation, expected) in [
            (0, Rotation::Deg0),
            (1, Rotation::Deg270),
            (2, Rotation::Deg180),
            (3, Rotation::Deg90),
        ] {
            let mut f = facts();
            f.orientation = orientation;
            assert_eq!(f.descriptor("a".into()).unwrap().rotation, expected);
        }
    }
    #[test]
    fn missing_or_inconsistent_facts_are_not_defaulted() {
        for f in [
            {
                let mut f = facts();
                f.mode_size.width = 1920;
                f
            },
            {
                let mut f = facts();
                f.mode_origin.0 = 0;
                f
            },
            {
                let mut f = facts();
                f.scale_percent = 0;
                f
            },
            {
                let mut f = facts();
                f.orientation = 4;
                f
            },
        ] {
            assert!(matches!(
                f.descriptor("a".into()),
                Err(DisplayError::UnsupportedDisplay { .. })
            ));
        }
    }
}
