//! Real XCB requestor/owner fixtures. Ignored, disposable X11 ONLY, run by CC.
//! This is selection interoperability, NOT native keyboard/app-receipt proof.
//! Outer process deadline is mandatory: even XCB connect can block in auth.
use super::support::RecordingDriver;
use crate::backend::dispatch::{DriverInput, NativeInput};
use rpa_native_input::{X11Lease, X11_LEASE_NONCE_PROPERTY, X11_LEASE_SELECTION};
use std::time::{Duration, Instant};
use xcb::{x, Connection, CookieWithReplyChecked, Event, Xid};

fn reply<C: CookieWithReplyChecked>(c: &Connection, cookie: C) -> C::Reply {
    c.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(result) = c.poll_for_reply(&cookie) {
            return result.unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "fixture server reply exceeded 2s"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn atom(c: &Connection, name: &str) -> x::Atom {
    reply(
        c,
        c.send_request(&x::InternAtom {
            only_if_exists: false,
            name: name.as_bytes(),
        }),
    )
    .atom()
}
fn owner(c: &Connection, selection: x::Atom) -> x::Window {
    reply(c, c.send_request(&x::GetSelectionOwner { selection })).owner()
}
fn barrier(c: &Connection) {
    let _ = reply(c, c.send_request(&x::GetInputFocus {}));
}
fn window(c: &Connection, root: x::Window) -> x::Window {
    let wid = c.generate_id();
    c.send_request(&x::CreateWindow {
        depth: 0,
        wid,
        parent: root,
        x: 0,
        y: 0,
        width: 32,
        height: 32,
        border_width: 0,
        class: x::WindowClass::InputOutput,
        visual: 0,
        value_list: &[x::Cw::EventMask(x::EventMask::PROPERTY_CHANGE)],
    });
    wid
}

struct Fixture {
    c: Connection,
    lease: X11Lease,
    focus: x::Window,
    requestor: x::Window,
    sentinel: x::Window,
    clipboard: x::Atom,
    utf8: x::Atom,
    property: x::Atom,
}
impl Fixture {
    fn new() -> Self {
        assert_eq!(
            std::env::var("RPA_COMPUTER_X11_CLIPBOARD_FIXTURE").as_deref(),
            Ok("disposable"),
            "only CC's owned disposable X server is allowed; never a user desktop"
        );
        let (c, _) = Connection::connect(None).unwrap();
        let root = c.get_setup().roots().next().unwrap().root();
        let writer = atom(&c, X11_LEASE_SELECTION);
        let nonce_property = atom(&c, X11_LEASE_NONCE_PROPERTY);
        let clipboard = atom(&c, "CLIPBOARD");
        let utf8 = atom(&c, "UTF8_STRING");
        let property = atom(&c, "_RPA_CLIPBOARD_FIXTURE_RESULT");
        assert!(
            owner(&c, writer).is_none(),
            "refuse an occupied writer lease"
        );
        assert!(
            owner(&c, clipboard).is_none(),
            "refuse an existing clipboard owner"
        );
        assert!(
            owner(&c, x::ATOM_PRIMARY).is_none(),
            "refuse an existing PRIMARY owner"
        );
        let sentinel = window(&c, root);
        let focus = window(&c, root);
        let requestor = window(&c, root);
        c.send_request(&x::MapWindow { window: focus });
        c.send_request(&x::ChangeProperty {
            mode: x::PropMode::Replace,
            window: sentinel,
            property: nonce_property,
            r#type: x::ATOM_STRING,
            data: &[7u8; 16],
        });
        // Fixture setup is the only code here that deliberately changes focus.
        // Production prepare/verify must leave it alone.
        c.send_request(&x::SetInputFocus {
            revert_to: x::InputFocus::Parent,
            focus,
            time: x::CURRENT_TIME,
        });
        c.send_request(&x::GrabServer {});
        assert!(
            owner(&c, writer).is_none(),
            "lease raced during fixture setup"
        );
        c.send_request(&x::SetSelectionOwner {
            owner: sentinel,
            selection: writer,
            time: x::CURRENT_TIME,
        });
        c.send_request(&x::SetSelectionOwner {
            owner: sentinel,
            selection: x::ATOM_PRIMARY,
            time: x::CURRENT_TIME,
        });
        c.send_request(&x::UngrabServer {});
        barrier(&c);
        assert_eq!(owner(&c, writer), sentinel);
        let lease = X11Lease::new(writer.resource_id(), sentinel.resource_id(), [7; 16]);
        Self {
            c,
            lease,
            focus,
            requestor,
            sentinel,
            clipboard,
            utf8,
            property,
        }
    }
    fn input(&self) -> DriverInput {
        DriverInput::new_with_x11_lease(Box::new(RecordingDriver::default()), self.lease.clone())
    }
    fn convert(&self, target: x::Atom) -> x::SelectionNotifyEvent {
        self.c.send_request(&x::DeleteProperty {
            window: self.requestor,
            property: self.property,
        });
        self.c.send_request(&x::ConvertSelection {
            requestor: self.requestor,
            selection: self.clipboard,
            target,
            property: self.property,
            time: x::CURRENT_TIME,
        });
        self.c.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(Event::X(x::Event::SelectionNotify(event))) =
                self.c.poll_for_event().unwrap()
            {
                assert_eq!(event.requestor(), self.requestor);
                assert_eq!(event.selection(), self.clipboard);
                assert_eq!(event.target(), target);
                return event;
            }
            assert!(
                Instant::now() < deadline,
                "real owner failed to answer selection request"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn property(&self) -> x::GetPropertyReply {
        reply(
            &self.c,
            self.c.send_request(&x::GetProperty {
                delete: false,
                window: self.requestor,
                property: self.property,
                r#type: x::ATOM_ANY,
                long_offset: 0,
                long_length: 4096,
            }),
        )
    }
    fn utf8(&self) -> Vec<u8> {
        assert_eq!(self.convert(self.utf8).property(), self.property);
        let data = self.property();
        assert_eq!(data.r#type(), self.utf8);
        assert_eq!(data.format(), 8);
        assert_eq!(data.bytes_after(), 0);
        data.value::<u8>().to_vec()
    }
    fn assert_focus_primary_unchanged(&self) {
        assert_eq!(
            reply(&self.c, self.c.send_request(&x::GetInputFocus {})).focus(),
            self.focus
        );
        assert_eq!(owner(&self.c, x::ATOM_PRIMARY), self.sentinel);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.lease.revoke();
    } // socket drop destroys fixture-owned windows
}

#[test]
#[ignore = "CC only: owned disposable X11 + explicit fixture opt-in + external 30s timeout"]
fn live_owner_serves_real_utf8_targets_timestamp_and_unsupported_reply() {
    let fixture = Fixture::new();
    let mut input = fixture.input();
    let text = "中文🙂\tA\rB\nC\r\nD";
    input.prepare_text_paste(text).unwrap();
    input.verify_text_paste().unwrap(); // verify before requestor roundtrips can expire receipt
    assert!(input.verify_text_paste().is_err());
    assert_eq!(fixture.utf8(), text.as_bytes());
    let targets = atom(&fixture.c, "TARGETS");
    assert_eq!(fixture.convert(targets).property(), fixture.property);
    let data = fixture.property();
    assert_eq!(data.r#type(), x::ATOM_ATOM);
    assert_eq!(data.format(), 32);
    let supported = data.value::<u32>();
    for name in ["TARGETS", "UTF8_STRING", "TIMESTAMP", "MULTIPLE"] {
        assert!(supported.contains(&atom(&fixture.c, name).resource_id()));
    }
    assert!(!supported.contains(&atom(&fixture.c, "INCR").resource_id()));
    assert!(!supported.contains(&atom(&fixture.c, "SAVE_TARGETS").resource_id()));
    assert_eq!(
        fixture.convert(atom(&fixture.c, "TIMESTAMP")).property(),
        fixture.property
    );
    let timestamp = fixture.property();
    assert_eq!(timestamp.r#type(), x::ATOM_INTEGER);
    assert_eq!(timestamp.format(), 32);
    assert_eq!(timestamp.value::<u32>().len(), 1);
    assert_ne!(timestamp.value::<u32>()[0], 0);
    assert!(fixture
        .convert(atom(&fixture.c, "_RPA_UNSUPPORTED_TARGET"))
        .property()
        .is_none());
    fixture.assert_focus_primary_unchanged();
}

#[test]
#[ignore = "CC only: owned disposable X11; verifies host lifetime, not application paste"]
fn live_cleanup_keeps_owned_text_but_drop_finishes_owner_without_daemon() {
    let fixture = Fixture::new();
    let mut input = fixture.input();
    input.prepare_text_paste(&"🙂".repeat(4096)).unwrap();
    input.cleanup().unwrap();
    assert!(
        input.verify_text_paste().is_err(),
        "cleanup must invalidate authorization"
    );
    assert_eq!(fixture.utf8(), "🙂".repeat(4096).as_bytes());
    fixture.assert_focus_primary_unchanged();
    let start = Instant::now();
    drop(input);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "drop cannot unbounded-join owner thread"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while !owner(&fixture.c, fixture.clipboard).is_none() {
        assert!(
            Instant::now() < deadline,
            "owner retained beyond driver lifetime without manager"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "CC only: owned disposable X11; no native input is emitted"]
fn live_stolen_selection_and_revoked_lease_block_chord_and_future_mutations() {
    let fixture = Fixture::new();
    let mut input = fixture.input();
    input.prepare_text_paste("old host text").unwrap();
    fixture.c.send_request(&x::SetSelectionOwner {
        owner: fixture.sentinel,
        selection: fixture.clipboard,
        time: x::CURRENT_TIME,
    });
    barrier(&fixture.c);
    assert!(input.verify_text_paste().is_err());
    assert!(input.verify_text_paste().is_err());
    fixture.lease.revoke();
    assert!(input
        .prepare_text_paste("must not replace external text")
        .is_err());
    input.cleanup().unwrap();
    drop(input);
    assert_eq!(owner(&fixture.c, fixture.clipboard), fixture.sentinel);
    fixture.assert_focus_primary_unchanged();
}

#[test]
#[ignore = "CC only: owned disposable X11; fixture switches between its own windows"]
fn live_focus_change_consumes_receipt_without_changing_focus_back() {
    let fixture = Fixture::new();
    let mut input = fixture.input();
    input.prepare_text_paste("text").unwrap();
    fixture.c.send_request(&x::MapWindow {
        window: fixture.requestor,
    });
    fixture.c.send_request(&x::SetInputFocus {
        revert_to: x::InputFocus::Parent,
        focus: fixture.requestor,
        time: x::CURRENT_TIME,
    });
    barrier(&fixture.c);
    assert!(input.verify_text_paste().is_err());
    assert!(input.verify_text_paste().is_err());
    assert_eq!(
        reply(&fixture.c, fixture.c.send_request(&x::GetInputFocus {})).focus(),
        fixture.requestor
    );
    assert_eq!(
        fixture.utf8(),
        b"text",
        "guard failure must not restore old clipboard"
    );
}
