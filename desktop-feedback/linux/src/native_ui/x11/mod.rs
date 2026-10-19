//! Real XCB windows, never Composite overlay or screen readback. Synchronous
//! checked X operations are covered by process.rs's independent hard deadline.
mod atoms;
mod drawing;
mod facts;
mod windows;
use super::*;
use atoms::Atoms;
use rpa_desktop_feedback_linux::geometry::Rect;
use xcb::{randr, render, shape, x, Xid, XidNew};

#[derive(Clone, Copy)]
struct Format {
    depth: u8,
    visual: u32,
    argb: bool,
    little: bool,
    shifts: [u32; 3],
}
#[derive(Clone)]
struct Raster {
    width: u16,
    height: u16,
    pixels: Vec<u8>,
    shape: Vec<x::Rectangle>,
}

pub struct X11Native {
    connection: Option<xcb::Connection>,
    root: x::Window,
    atoms: Atoms,
    cm: x::Atom,
    compositor: x::Window,
    format: Format,
    colormap: x::Colormap,
    windows: Option<WindowSet>,
    gcs: Vec<x::Gcontext>,
    facts: Option<DisplayFacts>,
    epoch: u64,
    ring_mapped: bool,
    raster: Vec<Raster>,
    rectangles: Vec<Rect>,
    grant: Option<rpa_desktop_feedback::protocol::Session>,
    grant_serial: u32,
    grant_revision: u64,
}
impl X11Native {
    pub fn connect() -> Result<Self, UiFailure> {
        let (conn, screen_num) = xcb::Connection::connect(None).map_err(|_| UiFailure::Native)?;
        let screen = conn
            .get_setup()
            .roots()
            .nth(screen_num as usize)
            .ok_or(UiFailure::Native)?;
        let root = screen.root();
        let atoms = Atoms::intern_all(&conn).map_err(|_| UiFailure::Native)?;
        let name = format!("_NET_WM_CM_S{screen_num}");
        let cm = conn
            .wait_for_reply(conn.send_request(&x::InternAtom {
                only_if_exists: false,
                name: name.as_bytes(),
            }))
            .map_err(|_| UiFailure::Native)?
            .atom();
        let compositor = conn
            .wait_for_reply(conn.send_request(&x::GetSelectionOwner { selection: cm }))
            .map_err(|_| UiFailure::Native)?
            .owner();
        let shape = conn
            .wait_for_reply(conn.send_request(&shape::QueryVersion {}))
            .map_err(|_| UiFailure::InputShapeUnavailable)?;
        if (shape.major_version(), shape.minor_version()) < (1, 1) {
            return Err(UiFailure::InputShapeUnavailable);
        }
        let rr = conn
            .wait_for_reply(conn.send_request(&randr::QueryVersion {
                major_version: 1,
                minor_version: 5,
            }))
            .map_err(|_| UiFailure::Native)?;
        if (rr.major_version(), rr.minor_version()) < (1, 5) {
            return Err(UiFailure::Native);
        }
        let root_visual = screen
            .allowed_depths()
            .flat_map(|d| d.visuals().to_vec())
            .find(|v| v.visual_id() == screen.root_visual())
            .ok_or(UiFailure::Native)?;
        let mut format = Format {
            depth: screen.root_depth(),
            visual: screen.root_visual(),
            argb: false,
            little: conn.get_setup().image_byte_order() == x::ImageOrder::LsbFirst,
            shifts: [
                root_visual.red_mask().trailing_zeros(),
                root_visual.green_mask().trailing_zeros(),
                root_visual.blue_mask().trailing_zeros(),
            ],
        };
        if root_visual.class() != x::VisualClass::TrueColor
            || [
                root_visual.red_mask(),
                root_visual.green_mask(),
                root_visual.blue_mask(),
            ]
            .iter()
            .any(|m| m.count_ones() != 8 || (*m >> m.trailing_zeros()) != 255)
        {
            return Err(UiFailure::Native);
        }
        if !compositor.is_none() {
            let formats = conn
                .wait_for_reply(conn.send_request(&render::QueryPictFormats {}))
                .map_err(|_| UiFailure::Native)?;
            let ids: Vec<_> = formats
                .formats()
                .iter()
                .filter(|f| {
                    let d = f.direct();
                    f.depth() == 32
                        && f.r#type() == render::PictType::Direct
                        && d.alpha_mask == 255
                        && d.alpha_shift == 24
                        && d.red_mask == 255
                        && d.red_shift == 16
                        && d.green_mask == 255
                        && d.green_shift == 8
                        && d.blue_mask == 255
                        && d.blue_shift == 0
                })
                .map(|f| f.id())
                .collect();
            if let Some(v) = formats.screens().nth(screen_num as usize).and_then(|s| {
                s.depths()
                    .flat_map(|d| d.visuals().to_vec())
                    .find(|v| ids.contains(&v.format))
            }) {
                format = Format {
                    depth: 32,
                    visual: v.visual,
                    argb: true,
                    shifts: [16, 8, 0],
                    ..format
                };
            }
        }
        if !conn.get_setup().pixmap_formats().iter().any(|p| {
            p.depth() == format.depth && p.bits_per_pixel() == 32 && p.scanline_pad() == 32
        }) {
            return Err(UiFailure::Native);
        }
        let colormap = conn.generate_id();
        conn.send_and_check_request(&x::CreateColormap {
            alloc: x::ColormapAlloc::None,
            mid: colormap,
            window: root,
            visual: format.visual,
        })
        .map_err(|_| UiFailure::Native)?;
        conn.send_and_check_request(&randr::SelectInput {
            window: root,
            enable: randr::NotifyMask::SCREEN_CHANGE
                | randr::NotifyMask::CRTC_CHANGE
                | randr::NotifyMask::OUTPUT_CHANGE
                | randr::NotifyMask::RESOURCE_CHANGE,
        })
        .map_err(|_| UiFailure::Native)?;
        conn.send_and_check_request(&x::ChangeWindowAttributes {
            window: root,
            value_list: &[x::Cw::EventMask(x::EventMask::PROPERTY_CHANGE)],
        })
        .map_err(|_| UiFailure::Native)?;
        Ok(Self {
            connection: Some(conn),
            root,
            atoms,
            cm,
            compositor,
            format,
            colormap,
            windows: None,
            gcs: Vec::new(),
            facts: None,
            epoch: 1,
            ring_mapped: false,
            raster: Vec::new(),
            rectangles: Vec::new(),
            grant: None,
            grant_serial: 0,
            grant_revision: 0,
        })
    }
    fn conn(&self) -> Result<&xcb::Connection, UiFailure> {
        self.connection.as_ref().ok_or(UiFailure::Native)
    }
    fn window(id: u32) -> x::Window {
        x::Window::new(id)
    }
    fn checked<R: xcb::RequestWithoutReply>(&self, request: &R) -> Result<(), UiFailure> {
        self.conn()?
            .send_and_check_request(request)
            .map_err(|_| UiFailure::Native)
    }
    pub fn next_event(&mut self) -> Result<Option<Event>, UiFailure> {
        let Some(event) = self
            .conn()?
            .poll_for_event()
            .map_err(|_| UiFailure::Native)?
        else {
            return Ok(None);
        };
        let Some(w) = self.windows else {
            return Ok(Some(Event::Poll));
        };
        use xcb::BaseEvent;
        let event = match event {
            xcb::Event::X(x::Event::ButtonPress(e)) if !e.is_from_send_event() => {
                if self.fresh_event(&e) {
                    self.grant
                        .clone()
                        .map_or(Event::Poll, |session| Event::NativePress {
                            session,
                            revision: self.grant_revision,
                            button: Button {
                                window: e.event().resource_id(),
                                epoch: w.epoch,
                                button: e.detail(),
                                x: e.event_x().into(),
                                y: e.event_y().into(),
                            },
                        })
                } else {
                    Event::Poll
                }
            }
            xcb::Event::X(x::Event::ButtonRelease(e)) if !e.is_from_send_event() => {
                if self.fresh_event(&e) {
                    self.grant
                        .clone()
                        .map_or(Event::Poll, |session| Event::NativeRelease {
                            session,
                            revision: self.grant_revision,
                            button: Button {
                                window: e.event().resource_id(),
                                epoch: w.epoch,
                                button: e.detail(),
                                x: e.event_x().into(),
                                y: e.event_y().into(),
                            },
                        })
                } else {
                    Event::Poll
                }
            }
            xcb::Event::X(x::Event::EnterNotify(e)) => Event::Hover {
                window: e.event().resource_id(),
                epoch: w.epoch,
                inside: true,
            },
            xcb::Event::X(x::Event::LeaveNotify(e)) => Event::Hover {
                window: e.event().resource_id(),
                epoch: w.epoch,
                inside: false,
            },
            xcb::Event::X(x::Event::Expose(e)) if e.count() == 0 => Event::Expose,
            xcb::Event::X(x::Event::DestroyNotify(e))
                if [w.status, w.stop, w.ring].contains(&e.window().resource_id()) =>
            {
                Event::NativeLost
            }
            xcb::Event::X(x::Event::UnmapNotify(e))
                if [w.status, w.stop].contains(&e.window().resource_id()) =>
            {
                Event::NativeLost
            }
            xcb::Event::X(x::Event::ClientMessage(e)) if e.r#type() == self.atoms.protocols => {
                if let x::ClientMessageData::Data32([atom, ..]) = e.data() {
                    if atom == self.atoms.delete.resource_id() {
                        if self.fresh_event(&e) {
                            self.grant
                                .clone()
                                .map_or(Event::Poll, |session| Event::NativeClose {
                                    window: e.window().resource_id(),
                                    epoch: w.epoch,
                                    session,
                                    revision: self.grant_revision,
                                })
                        } else {
                            Event::Poll
                        }
                    } else {
                        Event::Poll
                    }
                } else {
                    Event::Poll
                }
            }
            xcb::Event::RandR(_) => Event::TopologyChanged,
            xcb::Event::X(x::Event::PropertyNotify(e))
                if e.window() == self.root
                    && [
                        self.atoms.resources,
                        self.atoms.workarea,
                        self.atoms.desktop,
                    ]
                    .contains(&e.atom()) =>
            {
                Event::TopologyChanged
            }
            _ => Event::Poll,
        };
        Ok(Some(event))
    }
    fn fresh_event<E: xcb::BaseEvent>(&self, event: &E) -> bool {
        // SAFETY: a BaseEvent owns a live xcb_generic_event_t allocation,
        // including XCB's expanded full_sequence field. Never read after drop.
        let serial = unsafe { (*event.as_raw()).full_sequence };
        (serial.wrapping_sub(self.grant_serial) as i32) >= 0
    }
    pub fn refresh(&mut self) -> Result<bool, UiFailure> {
        let next = self.read_facts()?;
        self.maintain_stack()?;
        let changed = self
            .facts
            .as_ref()
            .is_none_or(|f| f.monitors != next.monitors || f.scale != next.scale);
        // read_facts does checked round trips even when nothing changes; an
        // unresponsive server cannot get its deadline refreshed by idle polling.
        Ok(changed)
    }
}
