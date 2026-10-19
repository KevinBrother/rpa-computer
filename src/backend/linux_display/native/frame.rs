use super::{changed, native_error, unsupported, X11Provider};
use crate::backend::linux_display::topology;
use rpa_display_topology::DisplayDescriptor;
use rpa_windows_display::{DisplayError, RgbaFrame, SourcePermit};
use xcb::x;

impl X11Provider {
    pub(super) fn capture_frame(
        &mut self,
        display: &DisplayDescriptor,
        permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        // No image request, reply allocation or RGBA allocation before this
        // permit/format validation and exact fresh topology revalidation.
        let layout = self.format.layout(display.capture_size)?.require(permit)?;
        let before = self.read_snapshot()?;
        if self.last.as_ref() != Some(&before)
            || !before
                .descriptors
                .iter()
                .any(|raw| self.identities.matches(raw, display))
        {
            self.last = None;
            return Err(changed(&display.id));
        }
        topology::validate_bounds(display.native_bounds, before.root_size)?;
        if display.capture_size.width != display.native_bounds.width
            || display.capture_size.height != display.native_bounds.height
        {
            return Err(unsupported(
                "X11 root capture must have identity pixel density",
            ));
        }
        let bounds = display.native_bounds;
        #[cfg(test)]
        self.capture_probe.request(bounds, permit.bytes());
        let reply = self
            .connection
            .wait_for_reply(self.connection.send_request(&x::GetImage {
                format: x::ImageFormat::ZPixmap,
                drawable: x::Drawable::Window(self.root),
                x: bounds.x as i16,
                y: bounds.y as i16,
                width: bounds.width as u16,
                height: bounds.height as u16,
                plane_mask: u32::MAX,
            }))
            .map_err(|error| native_error("X11.GetImage", error))?;
        #[cfg(test)]
        self.capture_probe.reply(reply.data().len());
        if reply.depth() != self.depth || reply.visual() != self.visual {
            return Err(changed(&display.id));
        }
        #[cfg(test)]
        self.capture_probe.decode_entry();
        let frame = layout.decode(reply.data())?;
        #[cfg(test)]
        self.capture_probe.decoded(frame.pixels().len());
        // Release native payload before querying and returning the owned RGBA.
        drop(reply);
        let after = self.read_snapshot()?;
        if before != after {
            self.last = None;
            return Err(changed(&display.id));
        }
        Ok(frame)
    }
}
