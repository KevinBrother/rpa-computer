//! Independent CC only, isolated X servers. Manipulates lease protocol state,
//! never emits keyboard/pointer input. Native same-connection input guards
//! require their own independent tests; these tests prove the host's monitor.
use super::linux_regression_tests::{display, Contender, Scratch};
use rpa_native_input::{X11_LEASE_NONCE_PROPERTY, X11_LEASE_SELECTION};
use std::time::{Duration, Instant};
use xcb::{x, Connection, Xid};

fn atom(c: &Connection, name: &str) -> x::Atom {
    c.wait_for_reply(c.send_request(&x::InternAtom {
        only_if_exists: true,
        name: name.as_bytes(),
    }))
    .unwrap()
    .atom()
}
fn owner(c: &Connection) -> x::Window {
    let selection = atom(c, X11_LEASE_SELECTION);
    assert!(
        !selection.is_none(),
        "canonical server-wide selection atom must exist"
    );
    let owner = c
        .wait_for_reply(c.send_request(&x::GetSelectionOwner { selection }))
        .unwrap()
        .owner();
    assert!(
        !owner.is_none(),
        "production lease must own the canonical selection"
    );
    owner
}
fn nonce(c: &Connection, owner: x::Window) -> Vec<u8> {
    let property = atom(c, X11_LEASE_NONCE_PROPERTY);
    assert!(!property.is_none());
    let reply = c
        .wait_for_reply(c.send_request(&x::GetProperty {
            delete: false,
            window: owner,
            property,
            r#type: x::ATOM_STRING,
            long_offset: 0,
            long_length: 5,
        }))
        .unwrap();
    assert_eq!(reply.r#type(), x::ATOM_STRING);
    assert_eq!(reply.format(), 8);
    assert_eq!(reply.bytes_after(), 0);
    let bytes = reply.value::<u8>().to_vec();
    assert_eq!(
        bytes.len(),
        16,
        "nonce is exactly 128 bits, not a DISPLAY string"
    );
    bytes
}
fn wait_revoked(child: &mut Contender) {
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let state = child.probe();
        if state == "revoked" {
            break;
        }
        assert_eq!(state, "live");
        assert!(
            Instant::now() < until,
            "lease loss must revoke a process-leaked token"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "independent CC: A_SCREEN_1 is screen 1 of isolated two-screen server A"]
fn canonical_selection_uses_private_first_root_window_and_exact_nonce() {
    let scratch = Scratch::new();
    let endpoint = display("LINUX_LEASE_TEST_DISPLAY_A_SCREEN_1");
    let first = Contender::spawn(Some(&endpoint), &scratch.dir("owner"));
    assert_eq!(first.result(), "owned");
    let (c, _) = Connection::connect(Some(&endpoint)).unwrap();
    let owned = owner(&c);
    let root = c.get_setup().roots().next().unwrap().root();
    assert_ne!(
        owned, root,
        "root lifetime cannot represent this client's ownership"
    );
    let tree = c
        .wait_for_reply(c.send_request(&x::QueryTree { window: owned }))
        .unwrap();
    assert_eq!(
        tree.parent(),
        root,
        "screen suffix must not change the rendezvous root"
    );
    let attrs = c
        .wait_for_reply(c.send_request(&x::GetWindowAttributes { window: owned }))
        .unwrap();
    assert_eq!(attrs.class(), x::WindowClass::InputOnly);
    assert_eq!(attrs.map_state(), x::MapState::Unmapped);
    let original_nonce = nonce(&c, owned);
    let second = Contender::spawn(Some(&endpoint), &scratch.dir("contender"));
    assert_eq!(second.result(), "contended");
    assert_eq!(
        owner(&c),
        owned,
        "contender must never steal selection ownership"
    );
    assert_eq!(
        nonce(&c, owned),
        original_nonce,
        "contender must not modify owner identity"
    );
}

#[test]
#[ignore = "independent CC: isolated server A; synthetic SelectionClear, no input"]
fn selection_clear_revokes_leaked_token_and_cannot_be_rearmed() {
    let scratch = Scratch::new();
    let endpoint = display("LINUX_LEASE_TEST_DISPLAY_A");
    let mut first = Contender::spawn(Some(&endpoint), &scratch.dir("owner"));
    assert_eq!(first.result(), "owned");
    let (c, _) = Connection::connect(Some(&endpoint)).unwrap();
    let old_nonce = nonce(&c, owner(&c));
    c.send_and_check_request(&x::SetSelectionOwner {
        owner: x::Window::none(),
        selection: atom(&c, X11_LEASE_SELECTION),
        time: x::CURRENT_TIME,
    })
    .unwrap();
    wait_revoked(&mut first);
    let successor = Contender::spawn(Some(&endpoint), &scratch.dir("successor"));
    assert_eq!(successor.result(), "owned");
    assert_ne!(
        nonce(&c, owner(&c)),
        old_nonce,
        "each ownership has a fresh OS-random identity"
    );
    assert_eq!(
        first.probe(),
        "revoked",
        "new ownership must not rearm the old token"
    );
}

#[test]
#[ignore = "independent CC: isolated server A; kills only the test owner's X client"]
fn x_connection_death_revokes_token_while_host_process_survives() {
    let scratch = Scratch::new();
    let endpoint = display("LINUX_LEASE_TEST_DISPLAY_A");
    let mut first = Contender::spawn(Some(&endpoint), &scratch.dir("owner"));
    assert_eq!(first.result(), "owned");
    let (c, _) = Connection::connect(Some(&endpoint)).unwrap();
    c.send_and_check_request(&x::KillClient {
        resource: owner(&c).resource_id(),
    })
    .unwrap();
    wait_revoked(&mut first);
    let successor = Contender::spawn(Some(&endpoint), &scratch.dir("successor"));
    assert_eq!(successor.result(), "owned");
    assert_eq!(first.probe(), "revoked");
}

#[test]
#[ignore = "independent CC: isolated server A; destroys only the test owner's window"]
fn owner_window_destruction_revokes_token() {
    let scratch = Scratch::new();
    let endpoint = display("LINUX_LEASE_TEST_DISPLAY_A");
    let mut first = Contender::spawn(Some(&endpoint), &scratch.dir("owner"));
    assert_eq!(first.result(), "owned");
    let (c, _) = Connection::connect(Some(&endpoint)).unwrap();
    c.send_and_check_request(&x::DestroyWindow { window: owner(&c) })
        .unwrap();
    wait_revoked(&mut first);
}

#[test]
#[ignore = "independent CC: simultaneous acquisition on isolated server A, no input"]
fn simultaneous_acquisition_never_publishes_two_owners() {
    let scratch = Scratch::new();
    let endpoint = display("LINUX_LEASE_TEST_DISPLAY_A");
    let mut contenders: Vec<_> = (0..4)
        .map(|i| Contender::spawn_gated(&endpoint, &scratch.dir(&format!("contender-{i}"))))
        .collect();
    for contender in &contenders {
        assert_eq!(contender.result(), "waiting");
    }
    for contender in &mut contenders {
        contender.start_acquire();
    }
    let results: Vec<_> = contenders.iter().map(Contender::result).collect();
    assert_eq!(
        results.iter().filter(|r| r.as_str() == "owned").count(),
        1,
        "{results:?}"
    );
    assert_eq!(
        results.iter().filter(|r| r.as_str() == "contended").count(),
        3,
        "{results:?}"
    );
}
