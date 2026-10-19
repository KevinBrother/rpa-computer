//! Query/capture-only XCB connection. No server grab, ownership, input or GUI.
mod enumerate;
mod frame;
#[cfg(test)]
mod probe;

use super::{identity::Identities, pixels::Format, topology};
use crate::backend::{BackendError, TargetScreen};
use rpa_display_topology::{DisplayDescriptor, PixelSize};
use rpa_windows_display::{DisplayError, FrameProvider, RgbaFrame, SourcePermit};
use std::{marker::PhantomData, rc::Rc};
use xcb::{randr, x, XidNew};

#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    root_size: PixelSize,
    descriptors: Vec<DisplayDescriptor>,
    monitors: Vec<topology::Monitor>,
    // Complete CRTC/output/transform/panning/config stamps, not a lossy hash.
    // Used to catch concurrent native changes even when bounds stay identical.
    native_state: Vec<String>,
}

pub(crate) struct X11Provider {
    connection: xcb::Connection,
    root: x::Window,
    depth: u8,
    visual: u32,
    format: Format,
    last: Option<Snapshot>,
    #[cfg(test)]
    capture_probe: probe::CaptureProbe,
    identities: Identities,
    _thread: PhantomData<Rc<()>>,
}
impl X11Provider {
    #[cfg(test)]
    pub(crate) fn capture_probe(&self) -> probe::CaptureProbe {
        self.capture_probe.clone()
    }

    pub(crate) fn attach() -> Result<Self, BackendError> {
        crate::backend::linux::prepare_thread()?;
        let display = std::env::var_os("DISPLAY")
            .filter(|name| !name.is_empty())
            .ok_or_else(|| BackendError::new("no_display", "DISPLAY is missing or empty"))?;
        let display = display
            .to_str()
            .filter(|name| !name.contains('\0'))
            .ok_or_else(|| {
                BackendError::new("no_display", "DISPLAY is not a valid X11 display name")
            })?;
        // Optional extension caching avoids xcb's mandatory-extension panic.
        let (connection, screen_index) =
            xcb::Connection::connect_with_extensions(Some(display), &[], &[xcb::Extension::RandR])
                .map_err(|error| {
                    BackendError::new("no_display", format!("X11 connection failed: {error}"))
                })?;
        if !connection
            .active_extensions()
            .any(|extension| extension == xcb::Extension::RandR)
        {
            return Err(BackendError::new(
                "unsupported_session",
                "X11 server lacks RandR",
            ));
        }
        let xwayland = connection
            .wait_for_reply(connection.send_request(&x::QueryExtension { name: b"XWAYLAND" }))
            .map_err(|error| {
                BackendError::new(
                    "display_error",
                    format!("XWayland extension query failed: {error:?}"),
                )
            })?;
        if xwayland.present() {
            return Err(BackendError::new(
                "unsupported_session",
                "XWayland capture/input is not supported",
            ));
        }
        let version = connection
            .wait_for_reply(connection.send_request(&randr::QueryVersion {
                major_version: 1,
                minor_version: 5,
            }))
            .map_err(|error| {
                BackendError::new(
                    "display_error",
                    format!("RandR version query failed: {error:?}"),
                )
            })?;
        if (version.major_version(), version.minor_version()) < (1, 5) {
            return Err(BackendError::new(
                "unsupported_session",
                "RandR 1.5 is required for root-pixel monitor topology",
            ));
        }
        let setup = connection.get_setup();
        let screen = setup
            .roots()
            .nth(screen_index as usize)
            .ok_or_else(|| BackendError::new("no_display", "requested X11 screen is absent"))?;
        let root = screen.root();
        let depth = screen.root_depth();
        let visual = screen.root_visual();
        let visual_type = screen
            .allowed_depths()
            .flat_map(|depth| depth.visuals().to_vec())
            .find(|candidate| candidate.visual_id() == visual)
            .ok_or_else(|| BackendError::new("display_error", "root visual is absent"))?;
        if visual_type.class() != x::VisualClass::TrueColor {
            return Err(BackendError::new(
                "unsupported_session",
                "X11 capture requires a TrueColor root visual",
            ));
        }
        let pixmap = setup
            .pixmap_formats()
            .iter()
            .find(|format| format.depth() == depth)
            .ok_or_else(|| BackendError::new("display_error", "root pixmap format is absent"))?;
        let format = Format {
            bits_per_pixel: pixmap.bits_per_pixel(),
            scanline_pad: pixmap.scanline_pad(),
            little_endian: setup.image_byte_order() == x::ImageOrder::LsbFirst,
            red: visual_type.red_mask(),
            green: visual_type.green_mask(),
            blue: visual_type.blue_mask(),
        };
        format
            .layout(PixelSize {
                width: 1,
                height: 1,
            })
            .map_err(crate::backend::display::provider_error)?;
        Ok(Self {
            connection,
            root,
            depth,
            visual,
            format,
            last: None,
            #[cfg(test)]
            capture_probe: probe::CaptureProbe::default(),
            identities: Identities::default(),
            _thread: PhantomData,
        })
    }

    /// Kept only for callers of the legacy public target_screen descriptor API.
    /// No capture goes through screenshots or Xft-scaled display-info.
    pub(crate) fn primary_target(&mut self) -> Result<TargetScreen, BackendError> {
        let snapshot = self
            .read_snapshot()
            .map_err(crate::backend::display::provider_error)?;
        let descriptor = snapshot
            .descriptors
            .iter()
            .find(|descriptor| descriptor.is_primary)
            .ok_or_else(|| BackendError::new("no_display", "no primary monitor"))?;
        // Monitor atoms identify logical monitors, including output-less virtual
        // monitors. raw_handle is unused by the native capture provider.
        let monitor = snapshot
            .monitors
            .iter()
            .find(|monitor| monitor.bounds == descriptor.native_bounds && monitor.primary)
            .or_else(|| snapshot.monitors.first())
            .ok_or_else(|| BackendError::new("no_display", "empty topology"))?;
        let output = monitor.outputs.first().copied().unwrap_or(0);
        let info = screenshots::display_info::DisplayInfo {
            id: monitor.name,
            raw_handle: randr::Output::new(output),
            x: descriptor.native_bounds.x,
            y: descriptor.native_bounds.y,
            width: descriptor.native_bounds.width,
            height: descriptor.native_bounds.height,
            rotation: match descriptor.rotation {
                rpa_display_topology::Rotation::Deg0 => 0.0,
                rpa_display_topology::Rotation::Deg90 => 90.0,
                rpa_display_topology::Rotation::Deg180 => 180.0,
                rpa_display_topology::Rotation::Deg270 => 270.0,
            },
            scale_factor: 1.0,
            frequency: 0.0,
            is_primary: true,
        };
        Ok(TargetScreen {
            display_info: info,
            capture_width: info.width,
            capture_height: info.height,
            scale_factor: 1.0,
        })
    }
}
impl FrameProvider for X11Provider {
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        // Errors retire attachment identities as well as cached facts; a
        // recovered query cannot revive old server resource IDs.
        let previous = self.last.take();
        let result = (|| {
            let snapshot = self.read_snapshot()?;
            // The shared descriptor has no field for a CRTC transform or
            // source mode/connection metadata. If native state changed while
            // all exposed geometry stayed identical, retire attachment IDs so
            // the shared generation cannot falsely certify the old mapping.
            if previous
                .as_ref()
                .is_some_and(|old| old != &snapshot && old.descriptors == snapshot.descriptors)
            {
                self.identities.invalidate();
            }
            let descriptors = self.identities.reconcile(&snapshot.descriptors)?;
            self.last = Some(snapshot);
            Ok(descriptors)
        })();
        if result.is_err() {
            self.identities.invalidate();
        }
        result
    }

    fn capture(
        &mut self,
        display: &DisplayDescriptor,
        permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        let result = self.capture_frame(display, permit);
        if result.is_err() {
            self.last = None;
            self.identities.invalidate();
        }
        result
    }
}

fn native_error(operation: &'static str, error: xcb::Error) -> DisplayError {
    // Preserve protocol error code; connection errors use the documented XCB
    // connection status domain with the high bit marking transport failure.
    let code = match error {
        xcb::Error::Protocol(error) => {
            // SAFETY: ProtocolError owns the live XCB error through this read.
            unsafe { (*error.as_raw()).error_code as u32 }
        }
        xcb::Error::Connection(error) => {
            0x8000_0000
                | match error {
                    xcb::ConnError::Connection => 1,
                    xcb::ConnError::ClosedExtNotSupported => 2,
                    xcb::ConnError::ClosedMemInsufficient => 3,
                    xcb::ConnError::ClosedReqLenExceed => 4,
                    xcb::ConnError::ClosedParseErr => 5,
                    xcb::ConnError::ClosedInvalidScreen => 6,
                    xcb::ConnError::ClosedFdPassingFailed => 7,
                }
        }
    };
    DisplayError::Native { operation, code }
}
fn changed(id: impl Into<String>) -> DisplayError {
    DisplayError::SourceChanged { id: id.into() }
}
fn unsupported(reason: &'static str) -> DisplayError {
    DisplayError::UnsupportedDisplay { reason }
}
