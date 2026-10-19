//! Independent checked-grab observation, never input injection or map mutation.
use super::super::super::x11_plan::HotkeyPlan;
use std::{
    fs,
    time::{Duration, Instant},
};
use xcb::x;

pub(super) type Entry = (x::Window, u8, u16);
pub(super) fn entries(roots: &[x::Window], plan: &HotkeyPlan) -> Vec<Entry> {
    roots
        .iter()
        .flat_map(|root| {
            plan.keycodes.iter().flat_map(move |key| {
                plan.modifier_masks
                    .iter()
                    .map(move |mask| (*root, *key, *mask))
            })
        })
        .collect()
}
fn grab(c: &xcb::Connection, (root, key, mask): Entry) -> Result<(), xcb::ProtocolError> {
    c.check_request(c.send_request_checked(&x::GrabKey {
        owner_events: false,
        grab_window: root,
        key,
        modifiers: x::ModMask::from_bits_truncate(u32::from(mask)),
        pointer_mode: x::GrabMode::Async,
        keyboard_mode: x::GrabMode::Async,
    }))
}
fn release(c: &xcb::Connection, (root, key, mask): Entry) {
    c.check_request(c.send_request_checked(&x::UngrabKey {
        grab_window: root,
        key,
        modifiers: x::ModMask::from_bits_truncate(u32::from(mask)),
    }))
    .expect("release only this probe client's grab");
}
fn access(error: &xcb::ProtocolError) -> bool {
    matches!(error, xcb::ProtocolError::X(x::Error::Access(_), _))
}
pub(super) fn prove_occupied(c: &xcb::Connection, entries: &[Entry]) {
    for &entry in entries {
        match grab(c, entry) {
            Ok(()) => {
                release(c, entry);
                panic!("Armed label without actual passive grab: {entry:?}");
            }
            Err(error) => assert!(
                access(&error),
                "expected BadAccess, not infrastructure/protocol failure: {error}"
            ),
        }
    }
    c.has_error().expect("live independent collision observer");
}
pub(super) fn prove_vacant(c: &xcb::Connection, entries: &[Entry]) {
    // Shutdown while Xvfb is stopped cannot be observed until it resumes. Allow
    // its event loop a bounded scheduling interval, never a keymap/server reset.
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let mut acquired = Vec::new();
        let mut collision = false;
        for &entry in entries {
            match grab(c, entry) {
                Ok(()) => acquired.push(entry),
                Err(error) if access(&error) => {
                    collision = true;
                    break;
                }
                Err(error) => {
                    for entry in acquired {
                        release(c, entry);
                    }
                    panic!("independent release probe failed: {error}");
                }
            }
        }
        for entry in acquired.iter().copied() {
            release(c, entry);
        }
        c.has_error().expect("live independent release observer");
        if !collision {
            assert_eq!(acquired.len(), entries.len());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "old hotkey grab remains after server resumed"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn require_owned_endpoint(display: &str, server: u32) {
    super::super::super::x11_plan::check_session(
        Some(display),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )
    .unwrap();
    let number = super::super::super::x11_auth::local_display_number(display).unwrap();
    assert_eq!(display, format!(":{number}"));
    let executable = fs::read_link(format!("/proc/{server}/exe")).unwrap();
    assert_eq!(executable.file_name().unwrap(), "Xvfb");
    let command = fs::read(format!("/proc/{server}/cmdline")).unwrap();
    let args: Vec<_> = command.split(|byte| *byte == 0).collect();
    assert!(args.contains(&display.as_bytes()));
    assert!(args.contains(&b"-noreset".as_slice()));
    let fds: Vec<_> = fs::read_dir(format!("/proc/{server}/fd"))
        .unwrap()
        .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let sockets = fs::read_to_string("/proc/net/unix").unwrap();
    let name = format!("/tmp/.X11-unix/X{number}");
    assert!(
        sockets.lines().any(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.len() >= 8
                && (fields[7] == name || fields[7] == format!("@{name}"))
                && fields[3] == "00010000"
                && fds.contains(&format!("socket:[{}]", fields[6]))
        }),
        "declared child does not own DISPLAY's listener"
    );
}
