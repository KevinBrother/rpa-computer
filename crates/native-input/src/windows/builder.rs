//! Pure event planning for the Windows SendInput driver.
//!
//! Platform-independent by design: Win32 flag/virtual-key values are
//! duplicated as raw constants (verified against the windows-0.58
//! bindings) so this module and its tests compile and run on any host.
//! `super` converts [`EventSpec`] values into real `INPUT` records and
//! calls `SendInput`.

use crate::types::{Axis, Button, Direction, InputError, Key, Result};

// --- Win32 constants (values verified against windows-0.58.0 bindings) ---

pub(crate) const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
pub(crate) const KEYEVENTF_KEYUP: u32 = 0x0002;
pub(crate) const KEYEVENTF_UNICODE: u32 = 0x0004;

pub(crate) const MOUSEEVENTF_MOVE: u32 = 0x0001;
pub(crate) const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
pub(crate) const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
pub(crate) const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
pub(crate) const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
pub(crate) const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
pub(crate) const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
pub(crate) const MOUSEEVENTF_WHEEL: u32 = 0x0800;
pub(crate) const MOUSEEVENTF_HWHEEL: u32 = 0x1000;
pub(crate) const MOUSEEVENTF_VIRTUALDESK: u32 = 0x4000;
pub(crate) const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;

pub(crate) const WHEEL_DELTA: i32 = 120;

// --- Event plan types ---

/// One SendInput event described in raw Win32 terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EventSpec {
    Mouse {
        dx: i32,
        dy: i32,
        flags: u32,
        mouse_data: u32,
    },
    Key {
        vk: u16,
        scan: u16,
        flags: u32,
    },
}

/// Virtual-screen geometry from GetSystemMetrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenMetrics {
    pub vx: i32,
    pub vy: i32,
    pub cx: i32,
    pub cy: i32,
}

/// A scalar text injection plan: the events to send plus which events are
/// downs that the driver itself must release if the send is partial.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextPlan {
    pub events: Vec<EventSpec>,
    /// Parallel to `events`: `Some(up_index)` if this event is a down whose
    /// matching up must be sent by the driver on partial-send cleanup.
    pub up_of: Vec<Option<usize>>,
}

// --- Key mapping ---

/// Map a contract `Key` to (virtual-key code, extended-key flag).
///
/// `Key::Character` only accepts ASCII letters/digits, which map to stable
/// physical VK codes; everything else is rejected before any effect.
pub(crate) fn key_vk(key: &Key) -> Result<(u16, bool)> {
    let (vk, ext) = match key {
        Key::Character(c) => {
            let vk = match c {
                'a'..='z' | 'A'..='Z' => (c.to_ascii_uppercase() as u8 - b'A' + 0x41) as u16,
                '0'..='9' => (*c as u8 - b'0' + 0x30) as u16,
                _ => {
                    return Err(InputError::new(
                        "invalid_key",
                        format!(
                            "Key::Character({:?}) is not an ASCII letter or digit; \
                             Character maps to physical virtual-key codes only, \
                             use text_scalar for arbitrary text",
                            c
                        ),
                    ))
                }
            };
            (vk, false)
        }
        Key::Control => (0x11, false),
        Key::Shift => (0x10, false),
        Key::Alt => (0x12, false),
        Key::Meta => (0x5B, false), // VK_LWIN
        Key::Return => (0x0D, false),
        Key::Tab => (0x09, false),
        Key::Space => (0x20, false),
        Key::Backspace => (0x08, false),
        Key::Delete => (0x2E, true),
        Key::Escape => (0x1B, false),
        Key::UpArrow => (0x26, true),
        Key::DownArrow => (0x28, true),
        Key::LeftArrow => (0x25, true),
        Key::RightArrow => (0x27, true),
        Key::Home => (0x24, true),
        Key::End => (0x23, true),
        Key::PageUp => (0x21, true),
        Key::PageDown => (0x22, true),
        Key::F1 => (0x70, false),
        Key::F2 => (0x71, false),
        Key::F3 => (0x72, false),
        Key::F4 => (0x73, false),
        Key::F5 => (0x74, false),
        Key::F6 => (0x75, false),
        Key::F7 => (0x76, false),
        Key::F8 => (0x77, false),
        Key::F9 => (0x78, false),
        Key::F10 => (0x79, false),
        Key::F11 => (0x7A, false),
        Key::F12 => (0x7B, false),
    };
    Ok((vk, ext))
}

pub(crate) fn key_event(key: &Key, direction: Direction) -> Result<EventSpec> {
    let (vk, ext) = key_vk(key)?;
    let mut flags = if ext { KEYEVENTF_EXTENDEDKEY } else { 0 };
    if direction == Direction::Release {
        flags |= KEYEVENTF_KEYUP;
    }
    Ok(EventSpec::Key { vk, scan: 0, flags })
}

// --- Mouse planning ---

/// One real atomic down/up specification. SendInput has no click-count
/// field; Windows/apps aggregate these events by timing and position.
/// Validate metadata here, before the caller can construct or send INPUT.
pub(crate) fn mouse_button(
    button: Button,
    direction: Direction,
    click_count: u8,
) -> Result<EventSpec> {
    if !crate::is_valid_click_count(click_count) {
        return Err(InputError::new(
            "invalid_button",
            format!("click_count {click_count} is out of range 1..=3"),
        ));
    }
    let down = match button {
        Button::Left => MOUSEEVENTF_LEFTDOWN,
        Button::Right => MOUSEEVENTF_RIGHTDOWN,
        Button::Middle => MOUSEEVENTF_MIDDLEDOWN,
    };
    let up = match button {
        Button::Left => MOUSEEVENTF_LEFTUP,
        Button::Right => MOUSEEVENTF_RIGHTUP,
        Button::Middle => MOUSEEVENTF_MIDDLEUP,
    };
    Ok(EventSpec::Mouse {
        dx: 0,
        dy: 0,
        flags: if direction == Direction::Press {
            down
        } else {
            up
        },
        mouse_data: 0,
    })
}

/// Map screen coordinates to normalized absolute coordinates over the
/// virtual screen (inclusive 0..=65535 per axis), clamping off-screen
/// positions into range. Uses i64 math to avoid overflow.
pub(crate) fn absolute_coords(x: i32, y: i32, m: ScreenMetrics) -> (u16, u16) {
    fn norm(pos: i32, origin: i32, extent: i32) -> u16 {
        if extent <= 1 {
            return 0;
        }
        let rel = (pos - origin) as i64;
        let scaled = rel * 65535 / (extent as i64 - 1);
        scaled.clamp(0, 65535) as u16
    }
    (norm(x, m.vx, m.cx), norm(y, m.vy, m.cy))
}

pub(crate) fn mouse_move(x: i32, y: i32, m: ScreenMetrics) -> Result<EventSpec> {
    if m.cx <= 0 || m.cy <= 0 {
        return Err(InputError::new(
            "no_display",
            format!(
                "invalid virtual-screen metrics: {}x{} at ({},{})",
                m.cx, m.cy, m.vx, m.vy
            ),
        ));
    }
    let (dx, dy) = absolute_coords(x, y, m);
    Ok(EventSpec::Mouse {
        dx: dx as i32,
        dy: dy as i32,
        flags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        mouse_data: 0,
    })
}

/// Wheel event for `length` ticks. Contract: positive length = RIGHT
/// (horizontal) / DOWN (vertical); Windows vertical positive delta = up,
/// so the vertical delta is negated.
pub(crate) fn wheel(length: i32, axis: Axis) -> Result<EventSpec> {
    if length == 0 {
        return Err(InputError::new(
            "input_failed",
            "scroll length must be non-zero",
        ));
    }
    let delta = length
        .checked_mul(WHEEL_DELTA)
        .ok_or_else(|| InputError::new("input_failed", "scroll length overflows wheel delta"))?;
    let (flags, signed) = match axis {
        Axis::Vertical => (MOUSEEVENTF_WHEEL, -delta),
        Axis::Horizontal => (MOUSEEVENTF_HWHEEL, delta),
    };
    Ok(EventSpec::Mouse {
        dx: 0,
        dy: 0,
        flags,
        mouse_data: signed as u32,
    })
}

// --- Text planning ---

fn vk_click(vk: u16) -> TextPlan {
    TextPlan {
        events: vec![
            EventSpec::Key {
                vk,
                scan: 0,
                flags: 0,
            },
            EventSpec::Key {
                vk,
                scan: 0,
                flags: KEYEVENTF_KEYUP,
            },
        ],
        up_of: vec![Some(1), None],
    }
}

/// Plan injection of one Unicode scalar value:
/// - NUL and other C0 control characters (except \n \r \t) are rejected.
/// - '\n' / '\r' each produce exactly one Return click (CRLF is normalized
///   once by the caller).
/// - '\t' produces exactly one Tab click.
/// - BMP scalars use one KEYEVENTF_UNICODE down/up pair with the UTF-16
///   code unit in the scan field and wVk = 0, per the KEYBDINPUT contract
///   ("wVk must be zero for KEYEVENTF_UNICODE"; VK_PACKET is only the
///   message code the system synthesizes afterwards).
/// - Supplementary scalars use highDown, lowDown, highUp, lowUp surrogate
///   pair events.
pub(crate) fn text_plan(ch: char) -> Result<TextPlan> {
    match ch {
        '\n' | '\r' => Ok(vk_click(0x0D)), // VK_RETURN
        '\t' => Ok(vk_click(0x09)),        // VK_TAB
        '\0' => Err(InputError::new(
            "invalid_text",
            "NUL cannot be injected as text",
        )),
        c if (c as u32) < 0x20 || (c as u32) == 0x7F => Err(InputError::new(
            "invalid_text",
            format!(
                "control character U+{:04X} is not injectable as text",
                c as u32
            ),
        )),
        c if (c as u32) <= 0xFFFF => {
            let unit = c as u16;
            Ok(TextPlan {
                events: vec![
                    EventSpec::Key {
                        vk: 0,
                        scan: unit,
                        flags: KEYEVENTF_UNICODE,
                    },
                    EventSpec::Key {
                        vk: 0,
                        scan: unit,
                        flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    },
                ],
                up_of: vec![Some(1), None],
            })
        }
        c => {
            let mut units = [0u16; 2];
            let encoded = c.encode_utf16(&mut units);
            debug_assert_eq!(encoded.len(), 2, "non-BMP scalar encodes as a pair");
            let (high, low) = (units[0], units[1]);
            Ok(TextPlan {
                events: vec![
                    EventSpec::Key {
                        vk: 0,
                        scan: high,
                        flags: KEYEVENTF_UNICODE,
                    },
                    EventSpec::Key {
                        vk: 0,
                        scan: low,
                        flags: KEYEVENTF_UNICODE,
                    },
                    EventSpec::Key {
                        vk: 0,
                        scan: high,
                        flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    },
                    EventSpec::Key {
                        vk: 0,
                        scan: low,
                        flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    },
                ],
                up_of: vec![Some(2), Some(3), None, None],
            })
        }
    }
}

/// Events needed to release every down we injected whose up was not itself
/// injected, given that `sent` events of `plan` reached SendInput success.
pub(crate) fn text_cleanup(plan: &TextPlan, sent: usize) -> Vec<EventSpec> {
    let sent = sent.min(plan.events.len());
    plan.up_of
        .iter()
        .take(sent)
        .filter_map(|up| match up {
            Some(u) if *u >= sent => Some(plan.events[*u]),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VK_PACKET (0xE7) is the virtual-key code the system synthesizes for
    /// the WM_KEYDOWN/WM_KEYUP messages *resulting* from a KEYEVENTF_UNICODE
    /// input; per the KEYBDINPUT contract it must never be placed in an
    /// input's wVk.
    const VK_PACKET: u16 = 0xE7;

    fn key(vk: u16, scan: u16, flags: u32) -> EventSpec {
        EventSpec::Key { vk, scan, flags }
    }

    fn mouse(flags: u32, mouse_data: u32) -> EventSpec {
        EventSpec::Mouse {
            dx: 0,
            dy: 0,
            flags,
            mouse_data,
        }
    }

    // --- key_vk / key_event ---

    #[test]
    fn named_keys_map_to_expected_virtual_keys() {
        let cases: &[(Key, u16, bool)] = &[
            (Key::Control, 0x11, false),
            (Key::Shift, 0x10, false),
            (Key::Alt, 0x12, false),
            (Key::Meta, 0x5B, false),
            (Key::Return, 0x0D, false),
            (Key::Tab, 0x09, false),
            (Key::Space, 0x20, false),
            (Key::Backspace, 0x08, false),
            (Key::Delete, 0x2E, true),
            (Key::Escape, 0x1B, false),
            (Key::UpArrow, 0x26, true),
            (Key::DownArrow, 0x28, true),
            (Key::LeftArrow, 0x25, true),
            (Key::RightArrow, 0x27, true),
            (Key::Home, 0x24, true),
            (Key::End, 0x23, true),
            (Key::PageUp, 0x21, true),
            (Key::PageDown, 0x22, true),
            (Key::F1, 0x70, false),
            (Key::F2, 0x71, false),
            (Key::F3, 0x72, false),
            (Key::F4, 0x73, false),
            (Key::F5, 0x74, false),
            (Key::F6, 0x75, false),
            (Key::F7, 0x76, false),
            (Key::F8, 0x77, false),
            (Key::F9, 0x78, false),
            (Key::F10, 0x79, false),
            (Key::F11, 0x7A, false),
            (Key::F12, 0x7B, false),
        ];
        for (key, vk, ext) in cases {
            assert_eq!(key_vk(key).unwrap(), (*vk, *ext), "key {:?}", key);
        }
    }

    #[test]
    fn character_maps_ascii_letters_and_digits_to_physical_vk() {
        assert_eq!(key_vk(&Key::Character('a')).unwrap(), (0x41, false));
        assert_eq!(key_vk(&Key::Character('Z')).unwrap(), (0x5A, false));
        assert_eq!(key_vk(&Key::Character('0')).unwrap(), (0x30, false));
        assert_eq!(key_vk(&Key::Character('9')).unwrap(), (0x39, false));
    }

    #[test]
    fn character_rejects_non_ascii_alphanumeric_before_effects() {
        for c in ['!', 'é', '中', ' ', '\u{0}', '\n'] {
            let err = key_vk(&Key::Character(c)).unwrap_err();
            assert_eq!(err.code, "invalid_key", "char {:?}", c);
        }
    }

    #[test]
    fn key_event_sets_extended_and_keyup_flags() {
        let press = key_event(&Key::Delete, Direction::Press).unwrap();
        assert_eq!(
            press,
            key(0x2E, 0, KEYEVENTF_EXTENDEDKEY),
            "Delete press must carry the extended-key flag"
        );
        let release = key_event(&Key::Delete, Direction::Release).unwrap();
        assert_eq!(
            release,
            key(0x2E, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP)
        );
        let plain = key_event(&Key::Return, Direction::Press).unwrap();
        assert_eq!(plain, key(0x0D, 0, 0));
        let plain_up = key_event(&Key::Return, Direction::Release).unwrap();
        assert_eq!(plain_up, key(0x0D, 0, KEYEVENTF_KEYUP));
    }

    // --- mouse ---

    #[test]
    fn mouse_button_flags_per_button_and_direction() {
        assert_eq!(
            mouse_button(Button::Left, Direction::Press, 1).unwrap(),
            mouse(MOUSEEVENTF_LEFTDOWN, 0)
        );
        assert_eq!(
            mouse_button(Button::Left, Direction::Release, 1).unwrap(),
            mouse(MOUSEEVENTF_LEFTUP, 0)
        );
        assert_eq!(
            mouse_button(Button::Right, Direction::Press, 1).unwrap(),
            mouse(MOUSEEVENTF_RIGHTDOWN, 0)
        );
        assert_eq!(
            mouse_button(Button::Right, Direction::Release, 1).unwrap(),
            mouse(MOUSEEVENTF_RIGHTUP, 0)
        );
        assert_eq!(
            mouse_button(Button::Middle, Direction::Press, 1).unwrap(),
            mouse(MOUSEEVENTF_MIDDLEDOWN, 0)
        );
        assert_eq!(
            mouse_button(Button::Middle, Direction::Release, 1).unwrap(),
            mouse(MOUSEEVENTF_MIDDLEUP, 0)
        );
    }

    #[test]
    fn atomic_button_counts_build_one_real_event_and_reject_invalid_metadata() {
        for (button, down, up) in [
            (Button::Left, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            (Button::Right, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            (Button::Middle, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        ] {
            for (direction, flags) in [(Direction::Press, down), (Direction::Release, up)] {
                for count in 1..=3 {
                    assert_eq!(
                        mouse_button(button, direction, count).unwrap(),
                        EventSpec::Mouse {
                            dx: 0,
                            dy: 0,
                            flags,
                            mouse_data: 0,
                        }
                    );
                }
                for bad in [0, 4, 255] {
                    assert_eq!(
                        mouse_button(button, direction, bad).unwrap_err().code,
                        "invalid_button"
                    );
                }
            }
        }
    }

    #[test]
    fn absolute_coords_maps_virtual_screen_edges_inclusively() {
        // Three-monitor desktop: 1920 px left monitor at negative origin.
        let m = ScreenMetrics {
            vx: -1920,
            vy: 0,
            cx: 5760,
            cy: 1080,
        };
        assert_eq!(absolute_coords(m.vx, m.vy, m), (0, 0));
        assert_eq!(
            absolute_coords(m.vx + m.cx - 1, m.vy + m.cy - 1, m),
            (65535, 65535)
        );
        // Off-screen positions clamp into range instead of wrapping.
        assert_eq!(absolute_coords(m.vx - 1, m.vy, m), (0, 0));
        assert_eq!(absolute_coords(m.vx + m.cx, m.vy + m.cy, m), (65535, 65535));
    }

    #[test]
    fn absolute_coords_single_monitor() {
        let m = ScreenMetrics {
            vx: 0,
            vy: 0,
            cx: 1920,
            cy: 1080,
        };
        assert_eq!(absolute_coords(1919, 1079, m), (65535, 65535));
        assert_eq!(absolute_coords(0, 0, m), (0, 0));
    }

    #[test]
    fn absolute_coords_is_monotonic() {
        let m = ScreenMetrics {
            vx: -1920,
            vy: 0,
            cx: 5760,
            cy: 1080,
        };
        let mut last = 0u16;
        for x in m.vx..m.vx + m.cx {
            let (nx, _) = absolute_coords(x, 0, m);
            assert!(nx >= last, "non-monotonic at x={}", x);
            last = nx;
        }
        assert_eq!(last, 65535);
    }

    #[test]
    fn mouse_move_uses_absolute_virtualdesk_move_flags() {
        let m = ScreenMetrics {
            vx: 0,
            vy: 0,
            cx: 1920,
            cy: 1080,
        };
        let ev = mouse_move(960, 540, m).unwrap();
        match ev {
            EventSpec::Mouse { flags, .. } => assert_eq!(
                flags,
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK
            ),
            other => panic!("expected mouse event, got {:?}", other),
        }
    }

    #[test]
    fn mouse_move_rejects_invalid_metrics() {
        let m = ScreenMetrics {
            vx: 0,
            vy: 0,
            cx: 0,
            cy: 0,
        };
        let err = mouse_move(0, 0, m).unwrap_err();
        assert_eq!(err.code, "no_display");
    }

    // --- wheel ---

    #[test]
    fn wheel_positive_vertical_means_down_negated_delta() {
        let ev = wheel(2, Axis::Vertical).unwrap();
        assert_eq!(
            ev,
            mouse(MOUSEEVENTF_WHEEL, (-240i32) as u32),
            "vertical positive length must produce negative wheel delta (down)"
        );
    }

    #[test]
    fn wheel_positive_horizontal_means_right() {
        let ev = wheel(2, Axis::Horizontal).unwrap();
        assert_eq!(ev, mouse(MOUSEEVENTF_HWHEEL, 240u32));
    }

    #[test]
    fn wheel_negative_vertical_means_up() {
        let ev = wheel(-1, Axis::Vertical).unwrap();
        assert_eq!(ev, mouse(MOUSEEVENTF_WHEEL, 120u32));
    }

    #[test]
    fn wheel_rejects_zero_and_overflow() {
        assert_eq!(wheel(0, Axis::Vertical).unwrap_err().code, "input_failed");
        assert_eq!(
            wheel(i32::MAX, Axis::Vertical).unwrap_err().code,
            "input_failed"
        );
    }

    // --- text plan ---

    #[test]
    fn newline_and_return_each_produce_one_return_click() {
        let expected = TextPlan {
            events: vec![key(0x0D, 0, 0), key(0x0D, 0, KEYEVENTF_KEYUP)],
            up_of: vec![Some(1), None],
        };
        assert_eq!(text_plan('\n').unwrap(), expected);
        assert_eq!(text_plan('\r').unwrap(), expected);
    }

    #[test]
    fn tab_produces_one_tab_click() {
        let plan = text_plan('\t').unwrap();
        assert_eq!(plan.events.len(), 2);
        assert_eq!(plan.events[0], key(0x09, 0, 0));
        assert_eq!(plan.events[1], key(0x09, 0, KEYEVENTF_KEYUP));
    }

    #[test]
    fn nul_and_control_characters_are_rejected() {
        assert_eq!(text_plan('\0').unwrap_err().code, "invalid_text");
        for c in ['\x01', '\x1F', '\x7F'] {
            assert_eq!(
                text_plan(c).unwrap_err().code,
                "invalid_text",
                "char {:?}",
                c
            );
        }
    }

    #[test]
    fn bmp_scalar_uses_unicode_down_up_pair() {
        let plan = text_plan('é').unwrap(); // U+00E9
        assert_eq!(
            plan.events,
            vec![
                key(0, 0xE9, KEYEVENTF_UNICODE),
                key(0, 0xE9, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            ]
        );
        assert_eq!(plan.up_of, vec![Some(1), None]);
    }

    #[test]
    fn supplementary_scalar_uses_high_down_low_down_high_up_low_up() {
        let plan = text_plan('\u{1F600}').unwrap(); // 😀 → D83D DE00
        assert_eq!(
            plan.events,
            vec![
                key(0, 0xD83D, KEYEVENTF_UNICODE),
                key(0, 0xDE00, KEYEVENTF_UNICODE),
                key(0, 0xD83D, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
                key(0, 0xDE00, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            ]
        );
        assert_eq!(plan.up_of, vec![Some(2), Some(3), None, None]);
    }

    /// Regression: the KEYBDINPUT contract requires wVk == 0 for
    /// KEYEVENTF_UNICODE events. VK_PACKET (0xE7) is the message code the
    /// system synthesizes from such an event — it must never be planned as
    /// the input's virtual-key code. This guards against the earlier bug
    /// where text plans assigned vk = VK_PACKET.
    #[test]
    fn unicode_events_never_carry_vk_packet_as_input_vk() {
        for ch in ['a', 'é', '中', '\u{1F600}', '\u{10FFFD}'] {
            let plan = text_plan(ch).unwrap();
            for ev in &plan.events {
                match *ev {
                    EventSpec::Key { vk, flags, .. } => {
                        assert_eq!(flags & KEYEVENTF_UNICODE, KEYEVENTF_UNICODE);
                        assert_eq!(
                            vk, 0,
                            "wVk must be zero for KEYEVENTF_UNICODE ({ch:?}); \
                             VK_PACKET is a resulting message code, not an input"
                        );
                        assert_ne!(vk, VK_PACKET);
                    }
                    other => panic!("expected key event, got {other:?}"),
                }
            }
        }
    }

    // --- partial-send cleanup ---

    #[test]
    fn cleanup_releases_only_unreleased_downs() {
        let plan = text_plan('\u{1F600}').unwrap();
        assert!(text_cleanup(&plan, 0).is_empty());
        // high down injected: release both halves (low down never injected,
        // but its up is harmless-free; we only release injected downs).
        assert_eq!(text_cleanup(&plan, 1), vec![plan.events[2]]);
        // Both downs injected: release both ups.
        assert_eq!(text_cleanup(&plan, 2), vec![plan.events[2], plan.events[3]]);
        // highUp already injected: only lowUp pending.
        assert_eq!(text_cleanup(&plan, 3), vec![plan.events[3]]);
        // Fully sent: nothing to clean.
        assert!(text_cleanup(&plan, 4).is_empty());
    }

    #[test]
    fn cleanup_bmp_partial() {
        let plan = text_plan('a').unwrap();
        assert_eq!(text_cleanup(&plan, 1), vec![plan.events[1]]);
        assert!(text_cleanup(&plan, 2).is_empty());
        assert!(text_cleanup(&plan, 0).is_empty());
    }

    #[test]
    fn cleanup_return_click_partial() {
        let plan = text_plan('\n').unwrap();
        assert_eq!(text_cleanup(&plan, 1), vec![plan.events[1]]);
        assert!(text_cleanup(&plan, 2).is_empty());
    }

    #[test]
    fn cleanup_sent_above_len_is_clamped() {
        let plan = text_plan('a').unwrap();
        assert!(text_cleanup(&plan, 99).is_empty());
    }
}
