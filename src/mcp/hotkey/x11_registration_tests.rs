//! CC-only registration checks against an isolated Linux/X11 server.
//!
//! Shortcut explicitly revised on 2026-10-15; frozen F12 failure evidence is unchanged.
//! Ignored by default. These tests never inject input or change keyboard maps.
//! Set RPA_X11_HOTKEY_TESTS=isolated only after selecting an expendable X server.
//! The manual-delivery test additionally requires a separately coordinated press.
use super::{arm, x11_plan, HotkeyGuard, HotkeyStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use xcb::x;

struct DisarmOnDrop(HotkeyGuard);

impl Drop for DisarmOnDrop {
    fn drop(&mut self) {
        self.0.disarm();
    }
}

fn require_isolated_x11() {
    assert_eq!(
        std::env::var("RPA_X11_HOTKEY_TESTS").as_deref(),
        Ok("isolated"),
        "CC must explicitly authorize an isolated X11 server before registration tests"
    );
    x11_plan::check_session(
        std::env::var("DISPLAY").ok().as_deref(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )
    .expect("registration tests require X11, not XWayland");
}

fn read_plan(connection: &xcb::Connection) -> x11_plan::HotkeyPlan {
    let setup = connection.get_setup();
    let first = setup.min_keycode();
    let count = setup
        .max_keycode()
        .checked_sub(first)
        .unwrap()
        .checked_add(1)
        .unwrap();
    let keys = connection
        .wait_for_reply(connection.send_request(&x::GetKeyboardMapping {
            first_keycode: first,
            count,
        }))
        .expect("read keyboard mapping");
    let modifiers = connection
        .wait_for_reply(connection.send_request(&x::GetModifierMapping {}))
        .expect("read modifier mapping");
    let per_modifier = usize::from(modifiers.keycodes_per_modifier());
    x11_plan::plan(&x11_plan::KeyboardMap {
        first_keycode: first,
        keysyms_per_keycode: usize::from(keys.keysyms_per_keycode()),
        keysyms: keys.keysyms().to_vec(),
        modifier_keycodes: std::array::from_fn(|slot| {
            modifiers.keycodes()[slot * per_modifier..(slot + 1) * per_modifier].to_vec()
        }),
    })
    .expect("fixture must expose a supported Ctrl+Alt+Shift+Escape map")
}

fn grab(
    connection: &xcb::Connection,
    root: x::Window,
    key: u8,
    mask: u16,
) -> Result<(), xcb::ProtocolError> {
    connection.check_request(connection.send_request_checked(&x::GrabKey {
        owner_events: false,
        grab_window: root,
        modifiers: x::ModMask::from_bits_truncate(u32::from(mask)),
        key,
        pointer_mode: x::GrabMode::Async,
        keyboard_mode: x::GrabMode::Async,
    }))
}

#[test]
#[ignore = "CC only: isolated X11 server; passive registration, no injected input"]
fn real_registration_idle_disarm_and_rearm_release_cancel_references() {
    require_isolated_x11();
    let cancel = Arc::new(AtomicBool::new(false));
    for iteration in 0..16 {
        let start = Instant::now();
        let mut guard = DisarmOnDrop(arm(Arc::clone(&cancel)));
        assert!(
            guard.0.status().is_armed(),
            "iteration {iteration}: {}",
            guard.0.status().describe()
        );
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "initialization was not bounded"
        );
        assert!(guard
            .0
            .status()
            .describe()
            .contains("Ctrl+Alt+Shift+Escape (Linux/X11 session)"));
        assert!(!guard.0.status().describe().contains("F12"));
        assert!(guard.0.disarm_supported());
        assert!(
            !cancel.load(Ordering::SeqCst),
            "registration must not itself cancel"
        );
        let start = Instant::now();
        guard.0.disarm(); // No key event is sent to wake an idle loop.
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "idle event loop did not stop promptly"
        );
        assert!(!guard.0.status().is_armed());
        assert!(!guard.0.disarm_supported());
        guard.0.disarm(); // Idempotent.
        assert_eq!(
            Arc::strong_count(&cancel),
            1,
            "disarm leaked a cancel reference"
        );
    }
}

#[test]
#[ignore = "CC only: isolated X11 server; deliberately occupies one hotkey variant"]
fn real_collision_is_failed_rolls_back_partial_grabs_and_allows_rearm() {
    require_isolated_x11();
    let (holder, _) = xcb::Connection::connect(None).expect("connect isolated fixture");
    let plan = read_plan(&holder);
    let mut grabs = Vec::new();
    for screen in holder.get_setup().roots() {
        for &key in &plan.keycodes {
            for &mask in &plan.modifier_masks {
                grabs.push((screen.root(), key, mask));
            }
        }
    }
    // Put contention on the final entry, not the first, to exercise rollback.
    let &(root, key, mask) = grabs.last().expect("at least one root/key/variant");
    grab(&holder, root, key, mask).expect("fixture owns final variant");
    let cancel = Arc::new(AtomicBool::new(false));
    let mut rejected = DisarmOnDrop(arm(Arc::clone(&cancel)));
    assert!(
        matches!(rejected.0.status(), HotkeyStatus::Failed(_)),
        "collision must be Failed, never Armed/Unsupported: {}",
        rejected.0.status().describe()
    );
    assert!(!rejected.0.disarm_supported());
    rejected.0.disarm();
    assert_eq!(
        Arc::strong_count(&cancel),
        1,
        "failed initialization leaked its callback"
    );
    assert!(!cancel.load(Ordering::SeqCst));

    // A different connection must be able to own EVERY earlier variant now.
    let (probe, _) = xcb::Connection::connect(None).expect("connect rollback probe");
    for &(root, key, mask) in &grabs[..grabs.len() - 1] {
        grab(&probe, root, key, mask).expect("failed registration must release every partial grab");
    }
    drop(probe);
    drop(holder);
    let mut rearmed = DisarmOnDrop(arm(Arc::clone(&cancel)));
    assert!(
        rearmed.0.status().is_armed(),
        "{}",
        rearmed.0.status().describe()
    );
    rearmed.0.disarm();
    assert_eq!(Arc::strong_count(&cancel), 1);
}

#[test]
#[ignore = "CC only: isolated X11 server; no input, two competing guards"]
fn real_second_guard_cannot_claim_armed_and_disarming_loser_keeps_winner() {
    require_isolated_x11();
    let first_flag = Arc::new(AtomicBool::new(false));
    let second_flag = Arc::new(AtomicBool::new(false));
    let mut first = DisarmOnDrop(arm(Arc::clone(&first_flag)));
    assert!(
        first.0.status().is_armed(),
        "{}",
        first.0.status().describe()
    );
    let mut second = DisarmOnDrop(arm(Arc::clone(&second_flag)));
    assert!(matches!(second.0.status(), HotkeyStatus::Failed(_)));
    second.0.disarm();
    let mut third = DisarmOnDrop(arm(Arc::clone(&second_flag)));
    assert!(
        matches!(third.0.status(), HotkeyStatus::Failed(_)),
        "loser released winner's grabs"
    );
    third.0.disarm();
    first.0.disarm();
    let mut next = DisarmOnDrop(arm(Arc::clone(&second_flag)));
    assert!(next.0.status().is_armed(), "{}", next.0.status().describe());
    next.0.disarm();
    assert_eq!(Arc::strong_count(&first_flag), 1);
    assert_eq!(Arc::strong_count(&second_flag), 1);
}

#[test]
#[ignore = "CC serial manual delivery only: arm, then press Ctrl+Alt+Shift+Escape within 15s"]
fn real_manual_emergency_key_sets_shared_cancel() {
    require_isolated_x11();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut guard = DisarmOnDrop(arm(Arc::clone(&cancel)));
    assert!(
        guard.0.status().is_armed(),
        "{}",
        guard.0.status().describe()
    );
    eprintln!("CC fixture ARMED: press Ctrl+Alt+Shift+Escape on the isolated X11 desktop now");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !cancel.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    guard.0.disarm();
    assert!(
        cancel.load(Ordering::SeqCst),
        "no emergency event received before deadline"
    );
    assert_eq!(Arc::strong_count(&cancel), 1);
}

#[test]
#[ignore = "CC only: isolated Xvfb already SIGSTOP'ed by fixture owner; resume it after this test"]
fn real_stopped_xvfb_setup_is_bounded_and_never_reports_armed() {
    require_isolated_x11();
    // The harness, not this test, owns/freezes/resumes the server. Require
    // evidence of an actually stopped matching Xvfb, not merely no DISPLAY.
    let pid: u32 = std::env::var("RPA_X11_HOTKEY_STOPPED_XVFB_PID")
        .expect("CC must provide its stopped Xvfb PID")
        .parse()
        .unwrap();
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    assert!(
        status
            .lines()
            .any(|line| line.starts_with("State:") && line.contains('T')),
        "fixture server is not stopped"
    );
    let command = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap();
    let args: Vec<_> = command.split(|&byte| byte == 0).collect();
    assert_eq!(
        std::path::Path::new(std::str::from_utf8(args[0]).unwrap())
            .file_name()
            .unwrap(),
        "Xvfb"
    );
    let number = super::x11_auth::local_display_number(&std::env::var("DISPLAY").unwrap()).unwrap();
    assert!(
        args.contains(&format!(":{number}").as_bytes()),
        "stopped server PID must match DISPLAY"
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let mut guard = DisarmOnDrop(arm(Arc::clone(&cancel)));
    let elapsed = start.elapsed();
    match guard.0.status() {
        HotkeyStatus::Failed(error) => assert!(
            error.contains("timed out") || error.contains("deadline"),
            "{error}"
        ),
        other => panic!("stopped server must fail honestly: {}", other.describe()),
    }
    assert!(
        elapsed >= Duration::from_secs(1),
        "did not exercise the setup deadline"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "stalled setup was not interrupted and joined"
    );
    guard.0.disarm();
    assert_eq!(Arc::strong_count(&cancel), 1);
    assert!(!cancel.load(Ordering::SeqCst));
}

#[path = "x11_qualification/mod.rs"]
mod qualification;

#[path = "x11_stopped_qualification/mod.rs"]
mod stopped_qualification;
