//! Windows SendInput driver.
//!
//! Pure event planning lives in [`builder`]; this module converts plans
//! into `INPUT` records, injects them with `SendInput`, and handles
//! partial injections by releasing only the down events this driver
//! injected whose up has not been sent yet.

mod builder;

use crate::types::{Axis, Button, Direction, Driver, InputError, Key, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, MAPVK_VK_TO_VSC, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

pub(crate) struct Platform;

impl Platform {
    pub(crate) fn new() -> Result<Self> {
        // Best-effort display presence check: with no interactive desktop
        // GetSystemMetrics reports zero extents.
        unsafe {
            let cx = GetSystemMetrics(SM_CXSCREEN);
            let cvx = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            if cx <= 0 && cvx <= 0 {
                return Err(InputError::new(
                    "no_display",
                    "GetSystemMetrics reports no usable display",
                ));
            }
        }
        Ok(Platform)
    }

    fn screen_metrics(&self) -> Result<builder::ScreenMetrics> {
        let m = unsafe {
            builder::ScreenMetrics {
                vx: GetSystemMetrics(SM_XVIRTUALSCREEN),
                vy: GetSystemMetrics(SM_YVIRTUALSCREEN),
                cx: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                cy: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        };
        if m.cx <= 0 || m.cy <= 0 {
            return Err(InputError::new(
                "no_display",
                format!(
                    "invalid virtual-screen metrics: {}x{} at ({},{})",
                    m.cx, m.cy, m.vx, m.vy
                ),
            ));
        }
        Ok(m)
    }

    fn to_input(&self, event: &builder::EventSpec) -> INPUT {
        match *event {
            builder::EventSpec::Mouse {
                dx,
                dy,
                flags,
                mouse_data,
            } => INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dy,
                        mouseData: mouse_data,
                        dwFlags: MOUSE_EVENT_FLAGS(flags),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            builder::EventSpec::Key { vk, scan, flags } => {
                // KEYBDINPUT contract: for KEYEVENTF_UNICODE events wVk MUST
                // be zero and wScan carries the UTF-16 code unit; VK_PACKET
                // is the message code the system synthesizes afterwards, not
                // an input. Plain VK events carry the VK in wVk and get
                // wScan enriched via MapVirtualKeyW.
                let wscan = if flags & builder::KEYEVENTF_UNICODE != 0 {
                    scan
                } else {
                    let sc = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) };
                    (sc & 0xFFFF) as u16
                };
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(vk),
                            wScan: wscan,
                            dwFlags: KEYBD_EVENT_FLAGS(flags),
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                }
            }
        }
    }

    /// Inject the events and return how many SendInput reported injected.
    fn send(&self, events: &[builder::EventSpec]) -> usize {
        if events.is_empty() {
            return 0;
        }
        let inputs: Vec<INPUT> = events.iter().map(|e| self.to_input(e)).collect();
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize }
    }

    fn single(&self, op: &str, event: builder::EventSpec) -> Result<()> {
        let sent = self.send(&[event]);
        if sent >= 1 {
            Ok(())
        } else {
            Err(InputError::new(
                "input_failed",
                format!("{}: SendInput injected 0 of 1 events", op),
            ))
        }
    }
}

impl Driver for Platform {
    fn move_mouse(&mut self, x: i32, y: i32) -> Result<()> {
        let m = self.screen_metrics()?;
        let event = builder::mouse_move(x, y, m)?;
        self.single("move_mouse", event)
    }

    fn button(&mut self, button: Button, direction: Direction, click_count: u8) -> Result<()> {
        // Windows SendInput has no per-event multi-click field: the OS
        // aggregates consecutive down/up events at the same position into
        // clicks by its own timing. We still emit the REAL atomic event; the
        // count is validated here so an invalid request never reports
        // success or injects anything.
        let event = builder::mouse_button(button, direction, click_count)?;
        self.single("button", event)
    }

    fn key(&mut self, key: Key, direction: Direction) -> Result<()> {
        // invalid_key is rejected here, before any event is injected.
        let event = builder::key_event(&key, direction)?;
        self.single("key", event)
    }

    fn scroll(&mut self, length: i32, axis: Axis) -> Result<()> {
        if length == 0 {
            // Zero ticks: nothing to inject, consistent with the macOS
            // driver; a zero-delta wheel event is not meaningful input.
            return Ok(());
        }
        let event = builder::wheel(length, axis)?;
        self.single("scroll", event)
    }

    fn text_scalar(&mut self, ch: char) -> Result<()> {
        let plan = builder::text_plan(ch)?;
        let total = plan.events.len();
        let sent = self.send(&plan.events);
        if sent >= total {
            return Ok(());
        }
        // Partial injection: release every down we injected whose up has
        // not been sent, so no key of ours is left stuck. We do not touch
        // anything we did not inject ourselves.
        let cleanup = builder::text_cleanup(&plan, sent);
        let mut cleanup_detail = String::from("no pending up events to release");
        if !cleanup.is_empty() {
            let expected = cleanup.len();
            let released = self.send(&cleanup);
            cleanup_detail = format!(
                "cleanup injected {} of {} pending up events",
                released, expected
            );
            if released < expected {
                return Err(InputError::new(
                    "input_failed",
                    format!(
                        "text_scalar partial send: injected {} of {} events; {} but cleanup itself was partial",
                        sent, total, cleanup_detail
                    ),
                ));
            }
        }
        Err(InputError::new(
            "input_failed",
            format!(
                "text_scalar partial send: injected {} of {} events; {}",
                sent, total, cleanup_detail
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    // Regression tests for the EventSpec -> INPUT conversion itself (the
    // earlier VK_PACKET-in-wVk bug lived exactly here, invisible to
    // EventSpec-level tests). No SendInput is called: to_input only builds
    // records, and the one FFI call it makes (MapVirtualKeyW) is
    // side-effect free. These tests run on a real Windows host; on other
    // hosts they are type-checked via
    // `cargo check --target x86_64-pc-windows-msvc --all-targets`.
    use super::*;
    use crate::types::{Axis, Direction, Driver, Key};

    fn key_input(input: &INPUT) -> &KEYBDINPUT {
        assert_eq!(input.r#type, INPUT_KEYBOARD, "expected keyboard INPUT");
        unsafe { &input.Anonymous.ki }
    }

    fn mouse_input(input: &INPUT) -> &MOUSEINPUT {
        assert_eq!(input.r#type, INPUT_MOUSE, "expected mouse INPUT");
        unsafe { &input.Anonymous.mi }
    }

    #[test]
    fn unicode_input_has_zero_wvk_and_code_unit_in_wscan() {
        let p = Platform;
        let plan = builder::text_plan('é').unwrap(); // U+00E9
        let down = p.to_input(&plan.events[0]);
        let ki = key_input(&down);
        assert_eq!(
            ki.wVk.0, 0,
            "KEYBDINPUT contract: wVk must be ZERO for KEYEVENTF_UNICODE"
        );
        assert_ne!(
            ki.wVk.0, 0xE7,
            "VK_PACKET is a resulting message, never an input wVk"
        );
        assert_eq!(ki.wScan, 0xE9);
        assert_eq!(ki.dwFlags, KEYBD_EVENT_FLAGS(builder::KEYEVENTF_UNICODE));
        let up = p.to_input(&plan.events[1]);
        let ki = key_input(&up);
        assert_eq!(ki.wVk.0, 0);
        assert_eq!(ki.wScan, 0xE9);
        assert_eq!(
            ki.dwFlags,
            KEYBD_EVENT_FLAGS(builder::KEYEVENTF_UNICODE | builder::KEYEVENTF_KEYUP)
        );
    }

    #[test]
    fn supplementary_unicode_input_uses_surrogate_units_with_zero_wvk() {
        let p = Platform;
        let plan = builder::text_plan('\u{1F600}').unwrap(); // 😀 → D83D DE00
        let scans = [0xD83Du16, 0xDE00, 0xD83D, 0xDE00];
        let expected_flags = [
            builder::KEYEVENTF_UNICODE,
            builder::KEYEVENTF_UNICODE,
            builder::KEYEVENTF_UNICODE | builder::KEYEVENTF_KEYUP,
            builder::KEYEVENTF_UNICODE | builder::KEYEVENTF_KEYUP,
        ];
        for (ev, (&scan, &flags)) in plan
            .events
            .iter()
            .zip(scans.iter().zip(expected_flags.iter()))
        {
            let input = p.to_input(ev);
            let ki = key_input(&input);
            assert_eq!(ki.wVk.0, 0, "wVk must be ZERO for KEYEVENTF_UNICODE");
            assert_eq!(ki.wScan, scan);
            assert_eq!(ki.dwFlags, KEYBD_EVENT_FLAGS(flags));
        }
    }

    #[test]
    fn vk_input_carries_vk_and_mapped_scan() {
        let p = Platform;
        let press = builder::key_event(&Key::Return, Direction::Press).unwrap();
        let input = p.to_input(&press);
        let ki = key_input(&input);
        assert_eq!(ki.wVk.0, 0x0D);
        let expected_scan = unsafe { MapVirtualKeyW(0x0D, MAPVK_VK_TO_VSC) } & 0xFFFF;
        assert_eq!(ki.wScan as u32, expected_scan);
        assert_eq!(ki.dwFlags, KEYBD_EVENT_FLAGS(0));
        let release = builder::key_event(&Key::Return, Direction::Release).unwrap();
        let input = p.to_input(&release);
        let ki = key_input(&input);
        assert_eq!(ki.wVk.0, 0x0D);
        assert_eq!(ki.dwFlags, KEYBD_EVENT_FLAGS(builder::KEYEVENTF_KEYUP));
    }

    #[test]
    fn extended_key_input_keeps_extended_flag() {
        let p = Platform;
        let ev = builder::key_event(&Key::Delete, Direction::Press).unwrap();
        let input = p.to_input(&ev);
        let ki = key_input(&input);
        assert_eq!(ki.wVk.0, 0x2E);
        assert_eq!(
            ki.dwFlags,
            KEYBD_EVENT_FLAGS(builder::KEYEVENTF_EXTENDEDKEY)
        );
    }

    #[test]
    fn mouse_move_input_carries_absolute_virtualdesk_fields() {
        let p = Platform;
        let m = builder::ScreenMetrics {
            vx: 0,
            vy: 0,
            cx: 1920,
            cy: 1080,
        };
        let ev = builder::mouse_move(960, 540, m).unwrap();
        let input = p.to_input(&ev);
        let mi = mouse_input(&input);
        let (dx, dy) = builder::absolute_coords(960, 540, m);
        assert_eq!(mi.dx, dx as i32);
        assert_eq!(mi.dy, dy as i32);
        assert_eq!(
            mi.dwFlags,
            MOUSE_EVENT_FLAGS(
                builder::MOUSEEVENTF_MOVE
                    | builder::MOUSEEVENTF_ABSOLUTE
                    | builder::MOUSEEVENTF_VIRTUALDESK
            )
        );
        assert_eq!(mi.mouseData, 0);
    }

    #[test]
    fn wheel_input_carries_signed_delta() {
        let p = Platform;
        let ev = builder::wheel(2, Axis::Vertical).unwrap();
        let input = p.to_input(&ev);
        let mi = mouse_input(&input);
        assert_eq!(mi.dwFlags, MOUSE_EVENT_FLAGS(builder::MOUSEEVENTF_WHEEL));
        assert_eq!(mi.mouseData, (-240i32) as u32);
    }

    #[test]
    fn scroll_zero_is_noop_success() {
        let mut p = Platform;
        // Must return Ok without touching SendInput (early return before
        // any planning or injection), consistent with the macOS driver.
        p.scroll(0, Axis::Vertical).unwrap();
        p.scroll(0, Axis::Horizontal).unwrap();
    }
}
