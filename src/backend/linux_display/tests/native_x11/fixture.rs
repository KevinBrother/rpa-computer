//! Only this connection's windows, GCs and uniquely named RandR monitors are
//! created/deleted. Never changes screen mode, output primary, root properties,
//! focus, input state, existing monitors, or other clients' resources.
use rpa_display_topology::NativeRect;
use std::{
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

mod monitors;
static SERIAL: Mutex<()> = Mutex::new(());
use xcb::{randr, x, Xid};

pub(super) const LEFT: NativeRect = NativeRect {
    x: 32,
    y: 24,
    width: 200,
    height: 160,
};
pub(super) const RIGHT: NativeRect = NativeRect {
    x: 280,
    y: 64,
    width: 200,
    height: 160,
};
pub(super) const WINDOW_SIZE: (u16, u16) = (120, 96);
const WAIT: Duration = Duration::from_secs(2);

pub(super) fn unavailable(reason: impl std::fmt::Display) -> ! {
    panic!("UNAVAILABLE (not a passing/skipped acceptance case): {reason}")
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Paint {
    pub origin: (i32, i32),
    pub colors: [[u8; 4]; 3],
}
impl Paint {
    pub fn samples(self) -> Vec<(i32, i32, [u8; 4])> {
        // Asymmetric regions catch channel swaps, wrong stride, vertical flips,
        // incorrect monitor offsets and stale/empty captures, not just PNG magic.
        [
            (1, 1, 0),
            (58, 94, 0),
            (61, 1, 1),
            (118, 94, 1),
            (15, 25, 2),
            (40, 45, 2),
        ]
        .into_iter()
        .map(|(x, y, color)| (self.origin.0 + x, self.origin.1 + y, self.colors[color]))
        .collect()
    }
}
// Successful checked writes have known outcomes; failed or pending writes
// require reconciliation. Retain the owned atom even while absent/uncertain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Presence {
    Present,
    Absent,
    Unknown,
}
#[derive(Clone)]
struct OwnedMonitor {
    atom: x::Atom,
    name: String,
    presence: Presence,
    primary: bool,
    bounds: NativeRect,
    outputs: Vec<randr::Output>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct BaselineMonitor {
    atom: u32,
    primary: bool,
    automatic: bool,
    bounds: NativeRect,
    outputs: Vec<u32>,
}

pub(super) struct Fixture {
    pub connection: xcb::Connection,
    pub root: x::Window,
    pub root_size: (u32, u32),
    depth: u8,
    visual: u32,
    masks: [u32; 3],
    windows: Vec<x::Window>,
    gcs: Vec<x::Gcontext>,
    monitors: Vec<OwnedMonitor>,
    baseline: Vec<BaselineMonitor>,
    output_primary: randr::Output,
    _serial: MutexGuard<'static, ()>,
}
impl Fixture {
    pub fn connect() -> Self {
        let serial = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let display = std::env::var("DISPLAY").expect("UNAVAILABLE: DISPLAY is required");
        assert_eq!(
            std::env::var("RPA_X11_TEST_ISOLATED").as_deref(),
            Ok("owned-disposable-xvfb"),
            "UNAVAILABLE: explicit disposable Xvfb opt-in is required"
        );
        assert_eq!(
            std::env::var("RPA_X11_TEST_DISPLAY").as_deref(),
            Ok(display.as_str()),
            "UNAVAILABLE: DISPLAY must exactly match the explicitly authorized display"
        );
        let number = display
            .strip_prefix(':')
            .and_then(|s| s.parse::<u16>().ok());
        assert!(number.is_some_and(|n| n >= 90), "UNAVAILABLE: use an explicitly owned local :N display, N >= 90; no remote/default screen");
        let (connection, screen_number) =
            xcb::Connection::connect_with_extensions(Some(&display), &[], &[xcb::Extension::RandR])
                .unwrap_or_else(|error| unavailable(format!("connect: {error}")));
        assert_eq!(screen_number, 0, "UNAVAILABLE: fixture expects screen zero");
        if !connection
            .active_extensions()
            .any(|extension| extension == xcb::Extension::RandR)
        {
            unavailable("RandR unavailable");
        }
        let setup = connection.get_setup();
        assert_eq!(
            setup.roots().count(),
            1,
            "UNAVAILABLE: use a single-screen disposable Xvfb"
        );
        let screen = setup.roots().next().unwrap();
        let root = screen.root();
        let depth = screen.root_depth();
        let visual = screen.root_visual();
        let visual_info = screen
            .allowed_depths()
            .flat_map(|depth| depth.visuals().to_vec())
            .find(|candidate| candidate.visual_id() == visual)
            .unwrap();
        assert_eq!(
            visual_info.class(),
            x::VisualClass::TrueColor,
            "UNAVAILABLE: TrueColor required"
        );
        let masks = [
            visual_info.red_mask(),
            visual_info.green_mask(),
            visual_info.blue_mask(),
        ];
        let root_size = (
            u32::from(screen.width_in_pixels()),
            u32::from(screen.height_in_pixels()),
        );
        assert!(
            root_size.0 >= 640
                && root_size.1 >= 480
                && u64::from(root_size.0) * u64::from(root_size.1) <= 2_000_000,
            "UNAVAILABLE: use 640x480..2MP root (recommended 800x600x24)"
        );
        let version = connection
            .wait_for_reply(connection.send_request(&randr::QueryVersion {
                major_version: 1,
                minor_version: 5,
            }))
            .unwrap_or_else(|error| unavailable(format!("RandR version: {error:?}")));
        assert!(
            (version.major_version(), version.minor_version()) >= (1, 5),
            "UNAVAILABLE: RandR 1.5 required"
        );
        let output_primary = connection
            .wait_for_reply(connection.send_request(&randr::GetOutputPrimary { window: root }))
            .unwrap()
            .output();
        let mut fixture = Self {
            connection,
            root,
            root_size,
            depth,
            visual,
            masks,
            windows: vec![],
            gcs: vec![],
            monitors: vec![],
            baseline: vec![],
            output_primary,
            _serial: serial,
        };
        fixture.baseline = fixture.monitor_facts();
        if fixture.baseline.len() != 1
            || !fixture.baseline[0].automatic
            || fixture.baseline[0].bounds
                != (NativeRect {
                    x: 0,
                    y: 0,
                    width: root_size.0,
                    height: root_size.1,
                })
        {
            unavailable("expected one untouched automatic root-sized monitor; refuse to alter existing logical monitor layouts");
        }
        let children = fixture
            .connection
            .wait_for_reply(
                fixture
                    .connection
                    .send_request(&x::QueryTree { window: root }),
            )
            .unwrap();
        assert!(children.children().is_empty(), "UNAVAILABLE: root has other clients' windows; use an empty, dedicated Xvfb without WM/compositor");
        fixture.verify_optional_dpi();
        fixture
    }
    fn monitor_facts(&self) -> Vec<BaselineMonitor> {
        let reply = self
            .connection
            .wait_for_reply(self.connection.send_request(&randr::GetMonitors {
                window: self.root,
                get_active: true,
            }))
            .unwrap_or_else(|error| unavailable(format!("GetMonitors: {error:?}")));
        assert!(
            reply.monitors().take(65).count() <= 64,
            "UNAVAILABLE: unexpected monitor count"
        );
        let mut facts: Vec<_> = reply
            .monitors()
            .map(|monitor| {
                let mut outputs: Vec<_> = monitor.outputs().iter().map(Xid::resource_id).collect();
                outputs.sort_unstable();
                BaselineMonitor {
                    atom: monitor.name().resource_id(),
                    primary: monitor.primary(),
                    automatic: monitor.automatic(),
                    bounds: NativeRect {
                        x: i32::from(monitor.x()),
                        y: i32::from(monitor.y()),
                        width: u32::from(monitor.width()),
                        height: u32::from(monitor.height()),
                    },
                    outputs,
                }
            })
            .collect();
        facts.sort_by_key(|monitor| monitor.atom);
        facts
    }
    fn verify_optional_dpi(&self) {
        let Ok(expected) = std::env::var("RPA_X11_TEST_EXPECT_XFT_DPI") else {
            return;
        };
        let expected: u32 = expected
            .parse()
            .expect("UNAVAILABLE: expected DPI must be an integer");
        assert!(
            [96, 144, 192].contains(&expected),
            "UNAVAILABLE: supported fixture DPI matrix is 96/144/192"
        );
        let reply = self
            .connection
            .wait_for_reply(self.connection.send_request(&x::GetProperty {
                delete: false,
                window: self.root,
                property: x::ATOM_RESOURCE_MANAGER,
                r#type: x::ATOM_STRING,
                long_offset: 0,
                long_length: 4096,
            }))
            .unwrap();
        assert_eq!(
            reply.bytes_after(),
            0,
            "UNAVAILABLE: oversized RESOURCE_MANAGER"
        );
        assert_eq!(
            reply.format(),
            8,
            "UNAVAILABLE: configure Xft.dpi before running the matrix case"
        );
        let resources = std::str::from_utf8(reply.value::<u8>())
            .expect("UNAVAILABLE: non-UTF8 RESOURCE_MANAGER");
        let actual = resources
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.trim() == "Xft.dpi")
            .and_then(|(_, value)| value.trim().parse::<u32>().ok());
        assert_eq!(
            actual,
            Some(expected),
            "UNAVAILABLE: required Xft.dpi scenario was not configured by the runner"
        );
    }
    pub fn require_no_primary(&self) {
        if self.baseline.iter().any(|monitor| monitor.primary) || !self.output_primary.is_none() {
            unavailable("single-monitor fallback case requires the disposable server's original no-primary topology");
        }
    }

    pub fn paint(&mut self, bounds: NativeRect, secondary: bool) -> Paint {
        let origin = (bounds.x + 8, bounds.y + 8);
        assert!(bounds.width >= 136 && bounds.height >= 112);
        let window = self.connection.generate_id();
        let palette = if secondary {
            [[0, 255, 255, 255], [255, 255, 0, 255], [255, 0, 255, 255]]
        } else {
            [[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]]
        };
        let masks = self.masks;
        let pixel = |color: [u8; 4]| {
            masks
                .iter()
                .zip(color)
                .fold(0u32, |value, (mask, channel)| {
                    value | if channel == 255 { *mask } else { 0 }
                })
        };
        self.connection
            .check_request(self.connection.send_request_checked(&x::CreateWindow {
                depth: self.depth,
                wid: window,
                parent: self.root,
                x: origin.0 as i16,
                y: origin.1 as i16,
                width: WINDOW_SIZE.0,
                height: WINDOW_SIZE.1,
                border_width: 0,
                class: x::WindowClass::InputOutput,
                visual: self.visual,
                value_list: &[
                    x::Cw::BackPixel(pixel(palette[0])),
                    x::Cw::OverrideRedirect(true),
                ],
            }))
            .expect("fixture CreateWindow");
        self.windows.push(window);
        self.connection
            .check_request(
                self.connection
                    .send_request_checked(&x::MapWindow { window }),
            )
            .unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            let attrs = self
                .connection
                .wait_for_reply(
                    self.connection
                        .send_request(&x::GetWindowAttributes { window }),
                )
                .unwrap();
            if attrs.map_state() == x::MapState::Viewable {
                break;
            }
            if Instant::now() >= deadline {
                unavailable("owned window did not become viewable within 2s");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let gc = self.connection.generate_id();
        self.connection
            .check_request(self.connection.send_request_checked(&x::CreateGc {
                cid: gc,
                drawable: x::Drawable::Window(window),
                value_list: &[x::Gc::Foreground(pixel(palette[1]))],
            }))
            .unwrap();
        self.gcs.push(gc);
        self.connection
            .check_request(self.connection.send_request_checked(&x::PolyFillRectangle {
                drawable: x::Drawable::Window(window),
                gc,
                rectangles: &[x::Rectangle {
                    x: 60,
                    y: 0,
                    width: 60,
                    height: 96,
                }],
            }))
            .unwrap();
        self.connection
            .check_request(self.connection.send_request_checked(&x::ChangeGc {
                gc,
                value_list: &[x::Gc::Foreground(pixel(palette[2]))],
            }))
            .unwrap();
        self.connection
            .check_request(self.connection.send_request_checked(&x::PolyFillRectangle {
                drawable: x::Drawable::Window(window),
                gc,
                rectangles: &[x::Rectangle {
                    x: 12,
                    y: 20,
                    width: 32,
                    height: 32,
                }],
            }))
            .unwrap();
        // Reply is a server-side ordering barrier after all fixture drawing.
        let translated = self
            .connection
            .wait_for_reply(self.connection.send_request(&x::TranslateCoordinates {
                src_window: window,
                dst_window: self.root,
                src_x: 0,
                src_y: 0,
            }))
            .unwrap();
        assert_eq!(
            (i32::from(translated.dst_x()), i32::from(translated.dst_y())),
            origin
        );
        Paint {
            origin,
            colors: palette,
        }
    }
}
