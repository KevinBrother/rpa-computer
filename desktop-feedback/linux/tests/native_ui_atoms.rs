//! Production atom bootstrap RED. No X connection, process, or GUI is started.
//! Requires native-ui on Linux so a missing dependency cannot yield a fake pass.
#![cfg(all(target_os = "linux", feature = "native-ui"))]
#[path = "../src/native_ui/x11/atoms.rs"]
mod atoms;

use atoms::{Atoms, InternTransport};
use std::collections::BTreeMap;
use xcb::{x, Xid, XidNew};

// Independent expected names/order; not imported from the implementation table.
const NAMES: [&[u8]; 13] = [
    b"WM_PROTOCOLS",
    b"WM_DELETE_WINDOW",
    b"UTF8_STRING",
    b"_NET_WM_NAME",
    b"_NET_WM_PID",
    b"_RPA_DESKTOP_FEEDBACK_ROLE",
    b"_NET_WM_STATE",
    b"_NET_WM_STATE_ABOVE",
    b"_NET_WM_STATE_SKIP_TASKBAR",
    b"_NET_WM_STATE_SKIP_PAGER",
    b"_NET_WORKAREA",
    b"_NET_CURRENT_DESKTOP",
    b"RESOURCE_MANAGER",
];

#[derive(Debug, PartialEq, Eq)]
struct ReplyError(usize);

#[derive(Default)]
struct Replies {
    names: BTreeMap<Vec<u8>, u32>,
    requests: Vec<(Vec<u8>, bool)>,
    order: Vec<(char, usize)>,
    fail_at: Option<usize>,
}
impl InternTransport for Replies {
    type Cookie = usize;
    type Error = ReplyError;
    fn send(&mut self, request: &x::InternAtom<'_>) -> usize {
        let cookie = self.requests.len();
        self.requests
            .push((request.name.to_vec(), request.only_if_exists));
        self.order.push(('s', cookie));
        cookie
    }
    fn wait(&mut self, cookie: usize) -> Result<x::Atom, ReplyError> {
        self.order.push(('w', cookie));
        if self.fail_at == Some(cookie) {
            return Err(ReplyError(cookie));
        }
        let (name, only_if_exists) = &self.requests[cookie];
        // Protocol response oracle only: XML InternAtom returns NONE for an
        // absent name when only_if_exists=true. No renderer policy implemented.
        let id = if let Some(id) = self.names.get(name) {
            *id
        } else if *only_if_exists {
            0
        } else {
            let id = 100 + cookie as u32;
            self.names.insert(name.clone(), id);
            id
        };
        Ok(x::Atom::new(id))
    }
}
fn values(a: Atoms) -> [u32; 13] {
    [
        a.protocols,
        a.delete,
        a.utf8,
        a.name,
        a.pid,
        a.role,
        a.state,
        a.above,
        a.skip_taskbar,
        a.skip_pager,
        a.workarea,
        a.desktop,
        a.resources,
    ]
    .map(|atom| atom.resource_id())
}

#[test]
fn required_atom_requests_must_not_depend_on_a_wm_precreating_names() {
    let mut replies = Replies::default();
    Atoms::intern_with(&mut replies).unwrap();
    assert_eq!(replies.requests.len(), NAMES.len());
    for ((name, only_if_exists), expected) in replies.requests.iter().zip(NAMES) {
        assert_eq!(name.as_slice(), expected);
        assert!(
            !only_if_exists,
            "required atom must be creatable: {expected:?}"
        );
    }
}

#[test]
fn absent_required_names_bootstrap_to_usable_atoms_without_fixture_prewarm() {
    // Empty lookup registry, not a complete fake X server. In particular this
    // does not claim a real server lacks predefined core atoms.
    let mut replies = Replies::default();
    let actual = values(Atoms::intern_with(&mut replies).unwrap());
    for (id, name) in actual.into_iter().zip(NAMES) {
        assert_ne!(
            id, 0,
            "NONE cannot be used as required property/type: {name:?}"
        );
    }
    assert_eq!(replies.names.len(), NAMES.len());
}

#[test]
fn existing_atom_ids_and_send_all_then_wait_order_are_preserved() {
    let mut replies = Replies::default();
    for (i, name) in NAMES.into_iter().enumerate() {
        replies.names.insert(name.to_vec(), 700 + i as u32);
    }
    let actual = values(Atoms::intern_with(&mut replies).unwrap());
    assert_eq!(actual, std::array::from_fn(|i| 700 + i as u32));
    assert_eq!(
        replies
            .requests
            .iter()
            .map(|r| r.0.as_slice())
            .collect::<Vec<_>>(),
        NAMES
    );
    let expected: Vec<_> = (0..13)
        .map(|i| ('s', i))
        .chain((0..13).map(|i| ('w', i)))
        .collect();
    assert_eq!(replies.order, expected);
    assert_eq!(replies.names.len(), 13);
}

#[test]
fn explicit_reply_error_is_propagated_without_partial_success_or_retry() {
    for failed in 0..13 {
        let mut replies = Replies {
            fail_at: Some(failed),
            ..Replies::default()
        };
        assert_eq!(
            Atoms::intern_with(&mut replies).unwrap_err(),
            ReplyError(failed)
        );
        let expected: Vec<_> = (0..13)
            .map(|i| ('s', i))
            .chain((0..=failed).map(|i| ('w', i)))
            .collect();
        assert_eq!(replies.order, expected);
    }
}
