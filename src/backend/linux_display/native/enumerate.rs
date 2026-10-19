//! Native monitor enumeration. RandR 1.5 accounts for framebuffer transforms
//! and panning; CRTC/output facts provide rotation and concurrent-change checks.
use super::{changed, native_error, unsupported, Snapshot, X11Provider};
use crate::backend::linux_display::topology::{self, MAX_DISPLAYS, MAX_OUTPUTS};
use rpa_display_topology::{NativeRect, PixelSize, Rotation};
use rpa_windows_display::DisplayError;
use std::collections::BTreeMap;
use xcb::{randr, x, Xid};

impl X11Provider {
    pub(super) fn read_snapshot(&self) -> Result<Snapshot, DisplayError> {
        // Resource timestamps alone need not cover logical monitor/primary
        // changes. Two complete reads must agree before publishing any facts.
        let before = self.read_once()?;
        let after = self.read_once()?;
        if before != after {
            return Err(changed("X11 topology changed during enumeration"));
        }
        Ok(after)
    }

    fn read_once(&self) -> Result<Snapshot, DisplayError> {
        let connection = &self.connection;
        let resources = connection
            .wait_for_reply(
                connection.send_request(&randr::GetScreenResourcesCurrent { window: self.root }),
            )
            .map_err(|error| native_error("RandR.GetScreenResourcesCurrent", error))?;
        if resources.outputs().len() > MAX_OUTPUTS || resources.crtcs().len() > MAX_OUTPUTS {
            return Err(DisplayError::EnumerationLimit);
        }
        let geometry = connection
            .wait_for_reply(connection.send_request(&x::GetGeometry {
                drawable: x::Drawable::Window(self.root),
            }))
            .map_err(|error| native_error("X11.GetGeometry", error))?;
        if geometry.depth() != self.depth {
            return Err(changed("X11 root depth"));
        }
        let root_size = PixelSize {
            width: u32::from(geometry.width()),
            height: u32::from(geometry.height()),
        };
        let reply = connection
            .wait_for_reply(connection.send_request(&randr::GetMonitors {
                window: self.root,
                get_active: true,
            }))
            .map_err(|error| native_error("RandR.GetMonitors", error))?;
        if reply.monitors().take(MAX_DISPLAYS + 1).count() > MAX_DISPLAYS {
            return Err(DisplayError::EnumerationLimit);
        }
        let primary = connection
            .wait_for_reply(connection.send_request(&randr::GetOutputPrimary { window: self.root }))
            .map_err(|error| native_error("RandR.GetOutputPrimary", error))?
            .output();
        let mut native_state = vec![format!(
            "root:{}:{}:timestamps:{}:{}:primary:{}",
            root_size.width,
            root_size.height,
            resources.timestamp(),
            resources.config_timestamp(),
            primary.resource_id()
        )];
        let mut rotations = BTreeMap::new();
        for crtc in resources.crtcs() {
            let info = connection
                .wait_for_reply(connection.send_request(&randr::GetCrtcInfo {
                    crtc: *crtc,
                    config_timestamp: resources.config_timestamp(),
                }))
                .map_err(|error| native_error("RandR.GetCrtcInfo", error))?;
            if info.status() != randr::SetConfig::Success {
                return Err(changed("RandR CRTC configuration"));
            }
            if info.mode().is_none() {
                continue;
            }
            let rotation = match info.rotation().bits() & 15 {
                1 => Rotation::Deg0,
                2 => Rotation::Deg90,
                4 => Rotation::Deg180,
                8 => Rotation::Deg270,
                _ => return Err(unsupported("invalid RandR CRTC rotation")),
            };
            let transform = connection
                .wait_for_reply(connection.send_request(&randr::GetCrtcTransform { crtc: *crtc }))
                .map_err(|error| native_error("RandR.GetCrtcTransform", error))?;
            if transform.current_params().len() > 256
                || transform.current_filter_name().len() > 1024
            {
                return Err(DisplayError::EnumerationLimit);
            }
            let panning = connection
                .wait_for_reply(connection.send_request(&randr::GetPanning { crtc: *crtc }))
                .map_err(|error| native_error("RandR.GetPanning", error))?;
            if panning.status() != randr::SetConfig::Success {
                return Err(changed("RandR panning configuration"));
            }
            native_state.push(format!(
                "crtc:{}:{}:{}:{}:{}:{}:{:?}:{:?}:{:?}:{:?}:{:?}",
                crtc.resource_id(),
                info.mode().resource_id(),
                info.x(),
                info.y(),
                info.width(),
                info.height(),
                info.rotation(),
                transform.current_transform(),
                transform.current_filter_name(),
                transform.current_params(),
                (
                    panning.left(),
                    panning.top(),
                    panning.width(),
                    panning.height(),
                    panning.track_left(),
                    panning.track_top(),
                    panning.track_width(),
                    panning.track_height(),
                    panning.border_left(),
                    panning.border_top(),
                    panning.border_right(),
                    panning.border_bottom()
                )
            ));
            for output in info.outputs() {
                rotations.insert(output.resource_id(), rotation);
            }
        }
        for output in resources.outputs() {
            let info = connection
                .wait_for_reply(connection.send_request(&randr::GetOutputInfo {
                    output: *output,
                    config_timestamp: resources.config_timestamp(),
                }))
                .map_err(|error| native_error("RandR.GetOutputInfo", error))?;
            if info.status() != randr::SetConfig::Success {
                return Err(changed("RandR output configuration"));
            }
            if info.name().len() > 1024 {
                return Err(DisplayError::EnumerationLimit);
            }
            if info.name().starts_with(b"XWAYLAND") {
                return Err(unsupported("XWayland outputs are not supported"));
            }
            native_state.push(format!(
                "output:{}:{}:{:?}:{:?}",
                output.resource_id(),
                info.crtc().resource_id(),
                info.connection(),
                info.name()
            ));
        }
        let mut monitors = Vec::new();
        for monitor in reply.monitors() {
            if monitor.outputs().len() > MAX_OUTPUTS {
                return Err(DisplayError::EnumerationLimit);
            }
            let mut outputs: Vec<_> = monitor.outputs().iter().map(Xid::resource_id).collect();
            outputs.sort_unstable();
            let mut rotation = None;
            for output in &outputs {
                let current = rotations
                    .get(output)
                    .ok_or_else(|| changed("RandR monitor references inactive output"))?;
                if rotation.is_some_and(|previous| previous != *current) {
                    return Err(unsupported(
                        "one logical RandR monitor contains differently rotated CRTCs",
                    ));
                }
                rotation = Some(*current);
            }
            monitors.push(topology::Monitor {
                name: monitor.name().resource_id(),
                primary: monitor.primary(),
                bounds: NativeRect {
                    x: i32::from(monitor.x()),
                    y: i32::from(monitor.y()),
                    width: u32::from(monitor.width()),
                    height: u32::from(monitor.height()),
                },
                outputs,
                rotation: rotation.unwrap_or(Rotation::Deg0),
            });
        }
        monitors.sort_by_key(|monitor| monitor.name);
        native_state.sort();
        let descriptors = topology::descriptors(self.root.resource_id(), root_size, &monitors)?;
        // Catch changes inside this multi-request query instead of publishing a
        // mixture. No server grab is held while waiting for replies/capturing.
        let after = connection
            .wait_for_reply(
                connection.send_request(&randr::GetScreenResourcesCurrent { window: self.root }),
            )
            .map_err(|error| native_error("RandR.GetScreenResourcesCurrent/recheck", error))?;
        if (resources.timestamp(), resources.config_timestamp())
            != (after.timestamp(), after.config_timestamp())
        {
            return Err(changed("RandR topology changed during enumeration"));
        }
        Ok(Snapshot {
            root_size,
            descriptors,
            monitors,
            native_state,
        })
    }
}
