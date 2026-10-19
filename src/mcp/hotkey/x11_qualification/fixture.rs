use super::super::super::{arm, HotkeyGuard};
use super::{environment, input::Input, snapshot};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use xcb::{x, xkb, Xid};

const CASE_BUDGET: Duration = Duration::from_secs(1);

pub(super) struct Fixture {
    c: xcb::Connection,
    input: Input,
    root: x::Window,
    window: x::Window,
    created: bool,
    old_focus: x::Window,
    old_revert: x::InputFocus,
    baseline: snapshot::Snapshot,
    baseline_state: Vec<u8>,
    baseline_locks: u8,
    guard: Option<HotkeyGuard>,
    cancel: Arc<AtomicBool>,
    done: bool,
}

impl Fixture {
    pub fn new() -> Self {
        let display = environment::require_owned_server();
        let (c, _) =
            xcb::Connection::connect_with_extensions(Some(&display), &[], &[xcb::Extension::Xkb])
                .expect("fixture XCB connection");
        let roots: Vec<_> = c.get_setup().roots().map(|s| s.root()).collect();
        assert_eq!(
            roots.len(),
            1,
            "only a fresh single-screen Xvfb is permitted"
        );
        let root = roots[0];
        assert!(children(&c, root).is_empty(), "refusing unexpected root children: no WM, clipboard, or other GUI fixture may share this server");
        assert!(
            snapshot::held(&c).is_empty(),
            "refusing pre-held keys; never reset another owner's keyboard state"
        );
        let focus = c
            .wait_for_reply(c.send_request(&x::GetInputFocus {}))
            .unwrap();
        assert!(
            focus.focus().resource_id() <= 1 || focus.focus() == root,
            "unexpected baseline input focus"
        );
        let pointer = c
            .wait_for_reply(c.send_request(&x::QueryPointer { window: root }))
            .unwrap();
        assert_eq!(
            pointer.mask().bits() & 0x1f00,
            0,
            "refusing pre-held pointer buttons"
        );
        let version = c
            .wait_for_reply(c.send_request(&xkb::UseExtension {
                wanted_major: 1,
                wanted_minor: 0,
            }))
            .unwrap();
        assert!(version.supported(), "real XKB required");
        snapshot::require_default_carriers(&c);
        let state = snapshot::state(&c);
        assert!(
            state.base_mods().is_empty() && state.latched_mods().is_empty(),
            "unexpected base/latched modifiers"
        );
        assert_eq!(
            state.locked_mods().bits() & !18,
            0,
            "only Caps/Num baseline locks permitted"
        );
        assert_eq!(state.base_group(), 0);
        assert_eq!(state.latched_group(), 0);
        assert_eq!(
            state.locked_group(),
            xkb::Group::N1,
            "fresh default group required"
        );
        let baseline = snapshot::capture(&c);
        let baseline_state = snapshot::state_bytes(&c);
        let input = Input::open(&display);
        let window = c.generate_id();
        // Establish cleanup ownership before the first window/focus/input mutation.
        let mut fixture = Self {
            c,
            input,
            root,
            window,
            created: false,
            old_focus: focus.focus(),
            old_revert: focus.revert_to(),
            baseline,
            baseline_state,
            baseline_locks: state.locked_mods().bits() as u8,
            guard: None,
            cancel: Arc::new(AtomicBool::new(false)),
            done: false,
        };
        fixture
            .c
            .check_request(fixture.c.send_request_checked(&x::CreateWindow {
                depth: 0,
                wid: window,
                parent: root,
                x: 0,
                y: 0,
                width: 160,
                height: 80,
                border_width: 0,
                class: x::WindowClass::InputOutput,
                visual: 0,
                value_list: &[
                    x::Cw::OverrideRedirect(true),
                    x::Cw::EventMask(x::EventMask::KEY_PRESS | x::EventMask::KEY_RELEASE),
                ],
            }))
            .expect("create owned receiver");
        fixture.created = true;
        fixture
            .c
            .check_request(fixture.c.send_request_checked(&x::MapWindow { window }))
            .unwrap();
        fixture
            .c
            .check_request(fixture.c.send_request_checked(&x::SetInputFocus {
                revert_to: x::InputFocus::PointerRoot,
                focus: window,
                time: x::CURRENT_TIME,
            }))
            .unwrap();
        fixture.check_ownership();
        fixture
    }

    pub fn arm(&mut self) {
        self.guard = Some(arm(Arc::clone(&self.cancel)));
        self.healthy();
        assert!(
            !self.cancel.load(Ordering::SeqCst),
            "arm is not cancellation evidence"
        );
    }
    fn healthy(&self) {
        let status = self.guard.as_ref().expect("owned guard").status();
        assert!(
            status.is_armed(),
            "protection failure must not count as a positive input result: {}",
            status.describe()
        );
    }
    fn check_ownership(&self) {
        assert_eq!(
            children(&self.c, self.root),
            vec![self.window],
            "unexpected root child appeared"
        );
        assert_eq!(
            self.c
                .wait_for_reply(self.c.send_request(&x::GetInputFocus {}))
                .unwrap()
                .focus(),
            self.window,
            "owned receiver lost focus"
        );
    }
    pub fn locks(&mut self, wanted: u8) {
        assert!(
            snapshot::held(&self.c).is_empty(),
            "lock changes require all fixture keys up"
        );
        let current = snapshot::state(&self.c).locked_mods().bits() as u8;
        assert_eq!((current | wanted) & !18, 0);
        for (mask, key) in [(2, 66), (16, 77)] {
            if (current ^ wanted) & mask != 0 {
                self.input.tap(key);
            }
        }
        assert_eq!(
            snapshot::state(&self.c).locked_mods().bits() as u8,
            wanted,
            "physical Caps/Num taps must produce exact requested lock state"
        );
    }

    pub fn chord(&mut self, modifiers: &[u8], expected_mask: u16, positive: bool) {
        self.check_ownership();
        self.healthy();
        assert!(
            snapshot::held(&self.c).is_empty(),
            "previous case left keys down"
        );
        assert!(
            snapshot::capture(&self.c) == self.baseline,
            "keyboard configuration changed; no map workaround is permitted"
        );
        let _ = self.events();
        // Reset only the shared test cancellation flag, never keyboard state.
        self.cancel.store(false, Ordering::SeqCst);
        eprintln!(
            "HOTKEY_CASE start modifiers={modifiers:?} mask={expected_mask:#x} positive={positive}"
        );
        for &key in modifiers {
            self.input.press(key);
        }
        let mut held = snapshot::held(&self.c);
        held.sort_unstable();
        let mut expected = modifiers.to_vec();
        expected.sort_unstable();
        assert_eq!(held, expected, "unexpected held-key state before Escape");
        assert_eq!(
            snapshot::state(&self.c).mods().bits() as u16,
            expected_mask,
            "fixture chord did not produce intended exact state; do not reset/remap to repair it"
        );
        self.input.press(9);
        let deadline = Instant::now() + CASE_BUDGET;
        let mut presses = Vec::new();
        loop {
            presses.extend(
                self.events()
                    .into_iter()
                    .filter(|event| event.0 && event.1 == 9),
            );
            self.healthy();
            if (positive && self.cancel.load(Ordering::SeqCst))
                || (!positive && !presses.is_empty())
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "no actual {} before deadline: modifiers={modifiers:?}, state={expected_mask:#x}",
                if positive {
                    "cancel"
                } else {
                    "receiver Escape press/replay"
                }
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        if positive {
            assert!(
                presses.is_empty(),
                "emergency press unexpectedly reached ordinary receiver"
            );
            assert!(self.cancel.load(Ordering::SeqCst));
        } else {
            assert_eq!(
                presses.len(),
                1,
                "expected exactly one original Escape press, not duplicates"
            );
            assert_eq!(presses[0].2 & 0xff, expected_mask);
            assert!(!presses[0].3, "SendEvent is not real replay evidence");
            assert!(
                !self.cancel.load(Ordering::SeqCst),
                "negative chord canceled"
            );
        }
        self.input.release_all();
        let deadline = Instant::now() + CASE_BUDGET;
        let mut releases = 0;
        loop {
            for event in self.events() {
                if event.1 == 9 {
                    assert!(!event.0, "late/duplicate Escape press at receiver");
                    assert!(!event.3);
                    releases += 1;
                }
            }
            if snapshot::held(&self.c).is_empty() && (positive || releases > 0) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "owned key release did not complete"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        // Observe delayed failures/cancellation too; one receiver event alone
        // cannot establish that a competing callback did not cancel later.
        let settle = Instant::now() + Duration::from_millis(40);
        while Instant::now() < settle {
            self.healthy();
            assert_eq!(
                self.cancel.load(Ordering::SeqCst),
                positive,
                "late cancellation mismatch"
            );
            assert!(
                !self.events().iter().any(|event| event.0 && event.1 == 9),
                "late duplicate Escape press"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        self.check_ownership();
        assert!(
            snapshot::capture(&self.c) == self.baseline,
            "map changed during input"
        );
        eprintln!("HOTKEY_CASE verified modifiers={modifiers:?} state={expected_mask:#x} cancel={positive} receiver_press={}", presses.len());
    }

    // (press, keycode, state, SendEvent). Events must target OUR receiver.
    fn events(&self) -> Vec<(bool, u8, u16, bool)> {
        let mut result = Vec::new();
        for _ in 0..512 {
            let Some(event) = self.c.poll_for_event().expect("receiver event connection") else {
                return result;
            };
            match event {
                xcb::Event::X(x::Event::KeyPress(e)) => {
                    assert_eq!(e.event(), self.window);
                    result.push((
                        true,
                        e.detail(),
                        e.state().bits() as u16,
                        e.response_type() & 0x80 != 0,
                    ));
                }
                xcb::Event::X(x::Event::KeyRelease(e)) => {
                    assert_eq!(e.event(), self.window);
                    result.push((
                        false,
                        e.detail(),
                        e.state().bits() as u16,
                        e.response_type() & 0x80 != 0,
                    ));
                }
                _ => {}
            }
        }
        panic!("receiver exceeded bounded event budget");
    }

    fn cleanup(&mut self) -> bool {
        let mut okay = true;
        // Independent attempts: one failed restoration must not skip the others.
        okay &= attempt("disarm", || {
            if let Some(mut guard) = self.guard.take() {
                guard.disarm();
            }
        });
        okay &= attempt("owned keys up", || self.input.release_all());
        okay &= attempt("restore original Caps/Num", || {
            self.locks(self.baseline_locks)
        });
        okay &= attempt("restore focus", || {
            self.c
                .check_request(self.c.send_request_checked(&x::SetInputFocus {
                    revert_to: self.old_revert,
                    focus: self.old_focus,
                    time: x::CURRENT_TIME,
                }))
                .unwrap();
        });
        if self.created {
            okay &= attempt("destroy owned window", || {
                self.c
                    .check_request(self.c.send_request_checked(&x::DestroyWindow {
                        window: self.window,
                    }))
                    .unwrap();
                self.created = false;
            });
        }
        okay &= attempt("exact baseline verification", || {
            assert!(snapshot::held(&self.c).is_empty());
            assert!(
                snapshot::capture(&self.c) == self.baseline,
                "configuration differs from initial snapshot"
            );
            assert_eq!(
                snapshot::state_bytes(&self.c),
                self.baseline_state,
                "XKB state not exactly restored"
            );
            assert!(children(&self.c, self.root).is_empty());
            let focus = self
                .c
                .wait_for_reply(self.c.send_request(&x::GetInputFocus {}))
                .unwrap();
            assert_eq!(focus.focus(), self.old_focus);
            assert_eq!(focus.revert_to(), self.old_revert);
            assert_eq!(
                Arc::strong_count(&self.cancel),
                1,
                "registration leaked flag ownership"
            );
        });
        eprintln!("HOTKEY_FIXTURE cleanup exact_baseline={okay}");
        okay
    }
    pub fn finish(mut self) {
        let okay = self.cleanup();
        self.done = true;
        assert!(
            okay,
            "fixture cleanup/restoration failed; runner must dispose of its server"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.cleanup();
        }
    }
}
fn children(c: &xcb::Connection, root: x::Window) -> Vec<x::Window> {
    c.wait_for_reply(c.send_request(&x::QueryTree { window: root }))
        .unwrap()
        .children()
        .to_vec()
}
fn attempt(label: &str, action: impl FnOnce()) -> bool {
    let okay = catch_unwind(AssertUnwindSafe(action)).is_ok();
    if !okay {
        eprintln!("HOTKEY_FIXTURE cleanup FAILED: {label}");
    }
    okay
}
