use crate::requests::Command;
use serde_json::{json, Value};
use std::path::PathBuf;
use xcb::{randr, shape, x, xtest, Xid};

pub struct Observer {
    pub conn: xcb::Connection,
    pub root: x::Window,
    target: x::Window,
    gc: x::Gcontext,
    pub dir: PathBuf,
    pid_atom: x::Atom,
    role_atom: x::Atom,
    pub masks: [u32; 3],
    pub little: bool,
    pub depth: u8,
    pressed: bool,
    pub captures: usize,
}
impl Observer {
    pub fn new(dir: PathBuf) -> Result<Self, &'static str> {
        if !dir.is_dir() || dir.is_symlink() {
            return Err("artifact directory");
        }
        let (conn, number) = xcb::Connection::connect(None).map_err(|_| "X connect")?;
        let setup = conn.get_setup();
        let screen = setup.roots().nth(number as usize).ok_or("screen")?;
        if (screen.width_in_pixels(), screen.height_in_pixels()) != (1920, 1080) {
            return Err("root geometry");
        }
        let visual = screen
            .allowed_depths()
            .flat_map(|d| d.visuals().to_vec())
            .find(|v| v.visual_id() == screen.root_visual())
            .ok_or("visual")?;
        let masks = [visual.red_mask(), visual.green_mask(), visual.blue_mask()];
        if masks != [0xff0000, 0x00ff00, 0x0000ff]
            || visual.class() != x::VisualClass::TrueColor
            || masks
                .iter()
                .any(|m| m.count_ones() != 8 || (*m >> m.trailing_zeros()) != 255)
            || !setup.pixmap_formats().iter().any(|f| {
                f.depth() == screen.root_depth()
                    && f.bits_per_pixel() == 32
                    && f.scanline_pad() == 32
            })
        {
            return Err("unsupported pixel format");
        }
        let root = screen.root();
        let depth = screen.root_depth();
        let little = setup.image_byte_order() == x::ImageOrder::LsbFirst;
        let sh = conn
            .wait_for_reply(conn.send_request(&shape::QueryVersion {}))
            .map_err(|_| "Shape")?;
        let rr = conn
            .wait_for_reply(conn.send_request(&randr::QueryVersion {
                major_version: 1,
                minor_version: 5,
            }))
            .map_err(|_| "RandR")?;
        let test = conn
            .wait_for_reply(conn.send_request(&xtest::GetVersion {
                major_version: 2,
                minor_version: 2,
            }))
            .map_err(|_| "XTEST")?;
        if (sh.major_version(), sh.minor_version()) < (1, 1)
            || (rr.major_version(), rr.minor_version()) < (1, 5)
            || (test.major_version(), test.minor_version()) < (2, 2)
        {
            return Err("extension version");
        }
        let mons = conn
            .wait_for_reply(conn.send_request(&randr::GetMonitors {
                window: root,
                get_active: true,
            }))
            .map_err(|_| "monitors")?;
        let monitors: Vec<_> = mons.monitors().collect();
        if monitors.len() != 1
            || monitors[0].x() != 0
            || monitors[0].y() != 0
            || monitors[0].width() != 1920
            || monitors[0].height() != 1080
        {
            return Err("monitor geometry");
        }
        let pid_atom = atom(&conn, "_NET_WM_PID")?;
        let role_atom = atom(&conn, "_RPA_DESKTOP_FEEDBACK_ROLE")?;
        let cm = atom(&conn, &format!("_NET_WM_CM_S{number}"))?;
        if !conn
            .wait_for_reply(conn.send_request(&x::GetSelectionOwner { selection: cm }))
            .map_err(|_| "compositor query")?
            .owner()
            .is_none()
        {
            return Err("unexpected compositor");
        }
        let target = conn.generate_id();
        let gc = conn.generate_id();
        conn.check_request(conn.send_request_checked(&x::ChangeWindowAttributes {
            window: root,
            value_list: &[x::Cw::EventMask(x::EventMask::SUBSTRUCTURE_NOTIFY)],
        }))
        .map_err(|_| "root events")?;
        conn.check_request(conn.send_request_checked(&x::CreateWindow {
            depth,
            wid: target,
            parent: root,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            border_width: 0,
            class: x::WindowClass::InputOutput,
            visual: screen.root_visual(),
            value_list: &[
                x::Cw::BackPixel(0x203040),
                x::Cw::EventMask(
                    x::EventMask::EXPOSURE
                        | x::EventMask::BUTTON_PRESS
                        | x::EventMask::BUTTON_RELEASE
                        | x::EventMask::FOCUS_CHANGE,
                ),
            ],
        }))
        .map_err(|_| "target create")?;
        conn.check_request(conn.send_request_checked(&x::CreateGc {
            cid: gc,
            drawable: x::Drawable::Window(target),
            value_list: &[x::Gc::GraphicsExposures(false)],
        }))
        .map_err(|_| "GC")?;
        conn.check_request(conn.send_request_checked(&x::MapWindow { window: target }))
            .map_err(|_| "target map")?;
        conn.check_request(conn.send_request_checked(&x::SetInputFocus {
            revert_to: x::InputFocus::PointerRoot,
            focus: target,
            time: x::CURRENT_TIME,
        }))
        .map_err(|_| "target focus")?;
        let this = Self {
            conn,
            root,
            target,
            gc,
            dir,
            pid_atom,
            role_atom,
            masks,
            little,
            depth,
            pressed: false,
            captures: 0,
        };
        this.paint_target()?;
        Ok(this)
    }
    fn paint_target(&self) -> Result<(), &'static str> {
        for parity in 0..2 {
            let color = if parity == 0 { 0x203040 } else { 0x607080 };
            self.conn
                .check_request(self.conn.send_request_checked(&x::ChangeGc {
                    gc: self.gc,
                    value_list: &[x::Gc::Foreground(color)],
                }))
                .map_err(|_| "target color")?;
            let rects: Vec<_> = (0..60)
                .filter(|n| n % 2 == parity)
                .map(|n| x::Rectangle {
                    x: n * 32,
                    y: 0,
                    width: 32,
                    height: 1080,
                })
                .collect();
            self.conn
                .check_request(self.conn.send_request_checked(&x::PolyFillRectangle {
                    drawable: x::Drawable::Window(self.target),
                    gc: self.gc,
                    rectangles: &rects,
                }))
                .map_err(|_| "target paint")?;
        }
        Ok(())
    }
    pub fn info(&self) -> Value {
        json!({"target":self.target.resource_id(),"root":[1920,1080],"shape":"1.1+","randr":"1.5+","xtest":"2.2+","compositor":false,"mapping_trace":"map-unmap-destroy-v1"})
    }
    pub fn focus(&self) -> Result<u32, &'static str> {
        Ok(self
            .conn
            .wait_for_reply(self.conn.send_request(&x::GetInputFocus {}))
            .map_err(|_| "focus")?
            .focus()
            .resource_id())
    }
    pub fn release(&mut self) -> Result<(), &'static str> {
        if self.pressed {
            self.fake(5, 1, 0, 0)?;
            self.pressed = false;
        }
        Ok(())
    }
    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) -> Result<(), &'static str> {
        self.conn
            .check_request(self.conn.send_request_checked(&xtest::FakeInput {
                r#type: kind,
                detail,
                time: 0,
                root: self.root,
                root_x: x,
                root_y: y,
                deviceid: 0,
            }))
            .map_err(|_| "XTEST input")?;
        self.focus()?; // server processing fence; not renderer/scan-out acknowledgment
        Ok(())
    }
    pub fn command(&mut self, c: Command) -> Result<Value, &'static str> {
        match c.op.as_str() {
            "move" => self.fake(6, 0, c.x, c.y)?,
            "press" => {
                if self.pressed {
                    return Err("already pressed");
                }
                self.fake(4, 1, 0, 0)?;
                self.pressed = true;
            }
            "release" => self.release()?,
            "inspect" => {
                return Ok(
                    json!({"windows":self.windows(c.pid)?,"focus":self.focus()?,"events":self.events()?}),
                )
            }
            "capture" => return self.capture(c.pid, &c.role, &c.tag),
            "events" => {}
            _ => return Err("command"),
        }
        // Explicit ordering: the server reply covers earlier XTEST/mapping work;
        // only then drain its queued root notifications. Do not rely on JSON order.
        let focus = self.focus()?;
        let events = self.events()?;
        Ok(json!({"events":events,"focus":focus}))
    }
    fn property(
        &self,
        window: x::Window,
        property: x::Atom,
    ) -> Result<x::GetPropertyReply, &'static str> {
        self.conn
            .wait_for_reply(self.conn.send_request(&x::GetProperty {
                delete: false,
                window,
                property,
                r#type: x::ATOM_ANY,
                long_offset: 0,
                long_length: 32,
            }))
            .map_err(|_| "property")
    }
    pub fn windows(&self, pid: u32) -> Result<Vec<Value>, &'static str> {
        let tree = self
            .conn
            .wait_for_reply(self.conn.send_request(&x::QueryTree { window: self.root }))
            .map_err(|_| "tree")?;
        if tree.children().len() > 256 {
            return Err("child budget");
        }
        let mut found = Vec::new();
        for &w in tree.children() {
            let Ok(p) = self.property(w, self.pid_atom) else {
                continue;
            };
            if p.r#type() != x::ATOM_CARDINAL || p.format() != 32 || p.value::<u32>() != [pid] {
                continue;
            }
            let role = self.property(w, self.role_atom)?;
            if role.format() != 8 || role.bytes_after() != 0 {
                return Err("role");
            }
            let role = std::str::from_utf8(role.value::<u8>()).map_err(|_| "role")?;
            if !["status", "stop", "ring"].contains(&role) {
                return Err("role");
            }
            let Ok(a) = self.conn.wait_for_reply(
                self.conn
                    .send_request(&x::GetWindowAttributes { window: w }),
            ) else {
                continue;
            };
            let Ok(g) = self
                .conn
                .wait_for_reply(self.conn.send_request(&x::GetGeometry {
                    drawable: x::Drawable::Window(w),
                }))
            else {
                continue;
            };
            let input = self
                .conn
                .wait_for_reply(self.conn.send_request(&shape::GetRectangles {
                    window: w,
                    source_kind: shape::Sk::Input,
                }))
                .map_err(|_| "input shape")?;
            let hints = self.property(w, x::ATOM_WM_HINTS)?;
            let no_focus = hints.format() == 32
                && hints.value::<u32>().len() >= 2
                && hints.value::<u32>()[0] & 1 == 1
                && hints.value::<u32>()[1] == 0;
            found.push(json!({"id":w.resource_id(),"role":role,"x":g.x(),"y":g.y(),"width":g.width(),"height":g.height(),"map_state":a.map_state() as u32,"override_redirect":a.override_redirect(),"no_focus_hint":no_focus,
                "input":input.rectangles().iter().map(|r| vec![i32::from(r.x),i32::from(r.y),i32::from(r.width),i32::from(r.height)]).collect::<Vec<_>>() }));
        }
        Ok(found)
    }
    pub fn events(&self) -> Result<Vec<Value>, &'static str> {
        let mut events = Vec::new();
        for _ in 0..512 {
            let Some(event) = self.conn.poll_for_event().map_err(|_| "events")? else {
                return Ok(events);
            };
            match event {
                xcb::Event::X(x::Event::CreateNotify(e))=>events.push(json!({"type":"create","window":e.window().resource_id()})),
                xcb::Event::X(x::Event::UnmapNotify(e))=>events.push(json!({"type":"unmap","window":e.window().resource_id()})),
                xcb::Event::X(x::Event::MapNotify(e))=>events.push(json!({"type":"map","window":e.window().resource_id()})),
                xcb::Event::X(x::Event::DestroyNotify(e))=>events.push(json!({"type":"destroy","window":e.window().resource_id()})),
                xcb::Event::X(x::Event::ButtonPress(e))=>events.push(json!({"type":"press","window":e.event().resource_id(),"x":e.root_x(),"y":e.root_y()})),
                xcb::Event::X(x::Event::ButtonRelease(e))=>events.push(json!({"type":"release","window":e.event().resource_id()})),
                xcb::Event::X(x::Event::FocusIn(e))=>events.push(json!({"type":"focus_in","window":e.event().resource_id()})),
                xcb::Event::X(x::Event::FocusOut(e))=>events.push(json!({"type":"focus_out","window":e.event().resource_id()})),
                xcb::Event::X(x::Event::Expose(e)) if e.window()==self.target && e.count()==0=>self.paint_target()?,
                _=>{},
            }
        }
        Err("event flood")
    }
}
fn atom(conn: &xcb::Connection, name: &str) -> Result<x::Atom, &'static str> {
    Ok(conn
        .wait_for_reply(conn.send_request(&x::InternAtom {
            only_if_exists: false,
            name: name.as_bytes(),
        }))
        .map_err(|_| "atom")?
        .atom())
}
