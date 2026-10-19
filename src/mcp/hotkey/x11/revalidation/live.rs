//! Read-only production adapter on the SAME dedicated, subscribed connection.
//! No server grab, mapping writes, input, prewarm, reconnect or lease renewal.
use super::*;
use crate::mcp::hotkey::x11::{read_map, xkb_aux_wire, xkb_map};
use crate::mcp::hotkey::x11_lifecycle::Control;
use std::sync::atomic::Ordering;
use xcb::{x, xkb};

pub(crate) struct LiveSource<'a> {
    connection: &'a xcb::Connection,
    control: &'a Control,
}
impl<'a> LiveSource<'a> {
    pub fn new(connection: &'a xcb::Connection, control: &'a Control) -> Self {
        Self {
            connection,
            control,
        }
    }
}
impl Source for LiveSource<'_> {
    type Event = xcb::Event;
    fn generation(&self) -> usize {
        // A borrowed, live registration-owned Control is never replaced/reused
        // during a proof. No process-global desktop authorization or new ID.
        self.control as *const Control as usize
    }
    fn checkpoint(&mut self, deadline: Instant) -> Result<(), String> {
        if Instant::now() >= deadline
            || self.control.stop.load(Ordering::SeqCst)
            || self.control.failure.get().is_some()
        {
            return Err("XKB semantic capture expired/stopped/failed".into());
        }
        self.connection
            .has_error()
            .map_err(|e| format!("XKB capture connection failed: {e}"))
    }
    fn read_image(&mut self, deadline: Instant) -> Result<SemanticImage, String> {
        self.checkpoint(deadline)?;
        let core = read_map(self.connection)?;
        self.checkpoint(deadline)?;
        let controls = self
            .connection
            .wait_for_reply(self.connection.send_request(&xkb::GetControls {
                device_spec: xkb_map::DEVICE,
            }))
            .map_err(|e| format!("cannot read full XKB controls: {e}"))?;
        self.checkpoint(deadline)?;
        let controls = xkb_aux_wire::controls_from_reply(&controls)?;
        let map = xkb_map::read_owned(self.connection)?;
        self.checkpoint(deadline)?;
        let compat = self
            .connection
            .wait_for_reply(self.connection.send_request(&xkb::GetCompatMap {
                device_spec: xkb_map::DEVICE,
                groups: xkb::SetOfGroup::all(),
                get_all_si: true,
                first_si: 0,
                n_si: 0,
            }))
            .map_err(|e| format!("cannot read full XKB compatibility: {e}"))?;
        self.checkpoint(deadline)?;
        let compatibility = xkb_aux_wire::compatibility_from_reply(&compat)?;
        Ok(SemanticImage {
            core,
            map,
            controls,
            compatibility,
        })
    }
    fn fence(&mut self, deadline: Instant) -> Result<(), String> {
        self.checkpoint(deadline)?;
        self.connection
            .wait_for_reply(self.connection.send_request(&x::GetInputFocus {}))
            .map_err(|e| format!("XKB capture roundtrip fence failed: {e}"))?;
        self.checkpoint(deadline)
    }
    fn queued(&mut self, deadline: Instant) -> Result<Option<xcb::Event>, String> {
        self.checkpoint(deadline)?;
        let Some(event) = self
            .connection
            .poll_for_queued_event()
            .map_err(|e| format!("XKB coherent capture queue failed: {e}"))?
        else {
            return Ok(None);
        };
        Ok(Some(event))
    }
    fn invalidation(&self, event: &xcb::Event) -> Option<Invalidation> {
        invalidation(event)
    }
}

pub(super) fn invalidation(event: &xcb::Event) -> Option<Invalidation> {
    // Conservative during capture: even a second nominally-equivalent NKN
    // or scratch edit prevents proof. Never infer companion suppression.
    match event {
        xcb::Event::X(x::Event::MappingNotify(e)) if e.response_type() & 0x80 == 0 => {
            Some(Invalidation::CoreMapping)
        }
        xcb::Event::Xkb(xkb::Event::NewKeyboardNotify(e)) if e.response_type() & 0x80 == 0 => {
            Some(Invalidation::NewKeyboard)
        }
        xcb::Event::Xkb(xkb::Event::MapNotify(e)) if e.response_type() & 0x80 == 0 => {
            Some(Invalidation::Map)
        }
        xcb::Event::Xkb(xkb::Event::ControlsNotify(e)) if e.response_type() & 0x80 == 0 => {
            Some(Invalidation::Controls)
        }
        xcb::Event::Xkb(xkb::Event::CompatMapNotify(e)) if e.response_type() & 0x80 == 0 => {
            Some(Invalidation::Compatibility)
        }
        _ => None,
    }
}
