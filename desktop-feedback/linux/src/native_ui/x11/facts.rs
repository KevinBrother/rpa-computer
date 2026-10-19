use super::*;
use rpa_desktop_feedback_linux::geometry::Monitor;
impl X11Native {
    fn property(&self, atom: x::Atom, kind: x::Atom) -> Result<x::GetPropertyReply, UiFailure> {
        let conn = self.conn()?;
        let reply = conn
            .wait_for_reply(conn.send_request(&x::GetProperty {
                delete: false,
                window: self.root,
                property: atom,
                r#type: kind,
                long_offset: 0,
                long_length: 4096,
            }))
            .map_err(|_| UiFailure::Native)?;
        if reply.bytes_after() != 0 {
            return Err(UiFailure::Geometry);
        }
        Ok(reply)
    }
    pub(super) fn read_facts(&self) -> Result<DisplayFacts, UiFailure> {
        let conn = self.conn()?;
        let compositor = conn
            .wait_for_reply(conn.send_request(&x::GetSelectionOwner { selection: self.cm }))
            .map_err(|_| UiFailure::Native)?
            .owner();
        // Visual choice is fixed for this process. A compositor change is a
        // visible failure, never silently keep alpha windows on a bare screen.
        if compositor != self.compositor {
            return Err(UiFailure::Native);
        }
        let bounds = conn
            .wait_for_reply(conn.send_request(&x::GetGeometry {
                drawable: x::Drawable::Window(self.root),
            }))
            .map_err(|_| UiFailure::Native)?;
        let root = Rect {
            x: 0,
            y: 0,
            width: bounds.width().into(),
            height: bounds.height().into(),
        };
        let desktop = self.property(self.atoms.desktop, x::ATOM_CARDINAL)?;
        let desktop_index = if desktop.r#type().is_none() {
            0
        } else {
            if desktop.format() != 32 || desktop.value::<u32>().len() != 1 {
                return Err(UiFailure::Geometry);
            }
            desktop.value::<u32>()[0] as usize
        };
        let work = self.property(self.atoms.workarea, x::ATOM_CARDINAL)?;
        let workarea = if work.r#type().is_none() {
            root
        } else {
            if work.format() != 32 || desktop_index >= 128 {
                return Err(UiFailure::Geometry);
            }
            let values = work.value::<u32>();
            if values.len() % 4 != 0 {
                return Err(UiFailure::Geometry);
            }
            let values = values
                .get(desktop_index * 4..desktop_index * 4 + 4)
                .ok_or(UiFailure::Geometry)?;
            Rect {
                x: i32::try_from(values[0]).map_err(|_| UiFailure::Geometry)?,
                y: i32::try_from(values[1]).map_err(|_| UiFailure::Geometry)?,
                width: values[2],
                height: values[3],
            }
        };
        let reply = conn
            .wait_for_reply(conn.send_request(&randr::GetMonitors {
                window: self.root,
                get_active: true,
            }))
            .map_err(|_| UiFailure::Geometry)?;
        let mut monitors = Vec::new();
        for m in reply.monitors().take(65) {
            if monitors.len() == 64 {
                return Err(UiFailure::Geometry);
            }
            let bounds = Rect {
                x: m.x().into(),
                y: m.y().into(),
                width: m.width().into(),
                height: m.height().into(),
            };
            if bounds.x < 0
                || bounds.y < 0
                || bounds.x as u64 + u64::from(bounds.width) > u64::from(root.width)
                || bounds.y as u64 + u64::from(bounds.height) > u64::from(root.height)
            {
                return Err(UiFailure::Geometry);
            }
            let left = i64::from(bounds.x).max(i64::from(workarea.x));
            let top = i64::from(bounds.y).max(i64::from(workarea.y));
            let right = (i64::from(bounds.x) + i64::from(bounds.width))
                .min(i64::from(workarea.x) + i64::from(workarea.width));
            let bottom = (i64::from(bounds.y) + i64::from(bounds.height))
                .min(i64::from(workarea.y) + i64::from(workarea.height));
            if right <= left || bottom <= top {
                return Err(UiFailure::Geometry);
            }
            monitors.push(Monitor {
                key: format!("randr:{}", m.name().resource_id()),
                bounds,
                workarea: Rect {
                    x: left as i32,
                    y: top as i32,
                    width: (right - left) as u32,
                    height: (bottom - top) as u32,
                },
            });
        }
        if monitors.is_empty() {
            return Err(UiFailure::Geometry);
        }
        let resource = self.property(self.atoms.resources, x::ATOM_STRING)?;
        let mut scale = None;
        if !resource.r#type().is_none() {
            if resource.format() != 8 {
                return Err(UiFailure::Geometry);
            }
            let text =
                std::str::from_utf8(resource.value::<u8>()).map_err(|_| UiFailure::Geometry)?;
            for line in text.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    if key.trim() == "Xft.dpi" {
                        if scale.is_some() {
                            return Err(UiFailure::Geometry);
                        }
                        let dpi = value
                            .trim()
                            .parse::<f64>()
                            .map_err(|_| UiFailure::Geometry)?;
                        if !dpi.is_finite() || !(48.0..=384.0).contains(&dpi) {
                            return Err(UiFailure::Geometry);
                        }
                        scale = Some(dpi / 96.0);
                    }
                }
            }
        }
        Ok(DisplayFacts {
            monitors,
            scale: scale.unwrap_or(1.0),
            epoch: self.epoch,
            input_shapes: true,
        })
    }
}
