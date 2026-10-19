//! Stage 2: post-arm stopped-server teardown. No input, windows or map writes.
mod control;
mod probe;
// Read-only frozen baseline utility; no Stage 1 source is edited or executed as a test.
#[path = "../x11_qualification/snapshot.rs"]
mod baseline;

use super::super::{arm, HotkeyGuard};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use xcb::{x, xkb};

struct Guard(HotkeyGuard);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.disarm();
    }
}

#[test]
#[ignore = "CC serial only: runner-owned private Xvfb, pidfd stop/resume protocol; no input"]
fn post_arm_stopped_server_disarm_releases_grabs_after_resume() {
    let mut owner = control::Control::connect();
    let display = std::env::var("DISPLAY").unwrap();
    probe::require_owned_endpoint(&display, owner.server);
    let (observer, _) =
        xcb::Connection::connect_with_extensions(Some(&display), &[], &[xcb::Extension::Xkb])
            .unwrap();
    let roots: Vec<_> = observer.get_setup().roots().map(|s| s.root()).collect();
    assert_eq!(roots.len(), 1, "fresh single-screen server only");
    assert!(observer
        .wait_for_reply(observer.send_request(&x::QueryTree { window: roots[0] }))
        .unwrap()
        .children()
        .is_empty());
    assert!(
        baseline::held(&observer).is_empty(),
        "never reset pre-held keys"
    );
    let pointer = observer
        .wait_for_reply(observer.send_request(&x::QueryPointer { window: roots[0] }))
        .unwrap();
    assert_eq!(
        pointer.mask().bits() & 0x1f00,
        0,
        "pre-held pointer buttons"
    );
    assert!(observer
        .wait_for_reply(observer.send_request(&xkb::UseExtension {
            wanted_major: 1,
            wanted_minor: 0
        }))
        .unwrap()
        .supported());
    baseline::require_default_carriers(&observer);
    let map_before = baseline::capture(&observer);
    let state_before = baseline::state_bytes(&observer);
    let focus_before = observer
        .wait_for_reply(observer.send_request(&x::GetInputFocus {}))
        .unwrap();
    assert!(focus_before.focus() == roots[0] || xcb::Xid::resource_id(&focus_before.focus()) <= 1);
    let plan = super::read_plan(&observer);
    assert_eq!(plan.keycodes, vec![9]);
    assert_eq!(plan.modifier_masks, vec![13, 15, 29, 31]);
    let entries = probe::entries(&roots, &plan);
    // Establish no previous owner BEFORE arming the product.
    probe::prove_vacant(&observer, &entries);
    let cancel = Arc::new(AtomicBool::new(false));
    let mut guard = Guard(arm(Arc::clone(&cancel)));
    assert!(
        guard.0.status().is_armed(),
        "{}",
        guard.0.status().describe()
    );
    assert!(!cancel.load(Ordering::SeqCst));
    // Every passive grab must REALLY collide, not just return an Armed label.
    probe::prove_occupied(&observer, &entries);
    assert!(
        guard.0.status().is_armed(),
        "{}",
        guard.0.status().describe()
    );
    assert!(!cancel.load(Ordering::SeqCst));
    owner.check_server(false);
    eprintln!("HOTKEY_STAGE2 healthy_arm_proven grabs={}", entries.len());
    owner.stopped();
    // No fixture X requests between STOPPED and RESUMED. Only product teardown
    // may hit the stopped server; its own deadline must interrupt that socket.
    let start = Instant::now();
    guard.0.disarm();
    let elapsed = start.elapsed();
    owner.check_server(true);
    assert!(
        elapsed < Duration::from_secs(2),
        "stopped-server disarm exceeded 2s: {elapsed:?}"
    );
    assert!(!guard.0.status().is_armed());
    assert!(!guard.0.disarm_supported());
    assert_eq!(
        Arc::strong_count(&cancel),
        1,
        "disarm returned with owned callback/watchdog references"
    );
    assert!(
        !cancel.load(Ordering::SeqCst),
        "explicit teardown should not synthesize an emergency press"
    );
    eprintln!(
        "HOTKEY_STAGE2 stopped_disarm_us={} cancel_refs=1 server_still_stopped=true",
        elapsed.as_micros()
    );
    owner.resumed(elapsed.as_micros());
    // This client predates arming; it is not the hotkey's disconnected client.
    // A resumed-server roundtrip and full independent acquisition prove release.
    observer
        .wait_for_reply(observer.send_request(&x::GetInputFocus {}))
        .unwrap();
    probe::prove_vacant(&observer, &entries);
    assert!(
        baseline::capture(&observer) == map_before,
        "map/control/compatibility baseline changed"
    );
    assert_eq!(
        baseline::state_bytes(&observer),
        state_before,
        "keyboard state changed"
    );
    assert!(baseline::held(&observer).is_empty());
    let focus_after = observer
        .wait_for_reply(observer.send_request(&x::GetInputFocus {}))
        .unwrap();
    assert_eq!(focus_after.focus(), focus_before.focus());
    assert_eq!(focus_after.revert_to(), focus_before.revert_to());
    assert!(observer
        .wait_for_reply(observer.send_request(&x::QueryTree { window: roots[0] }))
        .unwrap()
        .children()
        .is_empty());
    assert_eq!(Arc::strong_count(&cancel), 1);
    owner.check_server(false);
    eprintln!(
        "HOTKEY_STAGE2 resumed_old_grabs_absent={} exact_baseline=true",
        entries.len()
    );
    drop(observer);
    owner.finish();
}
