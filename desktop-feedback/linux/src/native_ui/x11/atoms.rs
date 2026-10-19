//! Actual connection bootstrap, extracted from xcb 1.7.1's atoms_struct macro.
//! Required names must be available without a window manager precreating them.
//! Preserve name/field order, send-all/wait-in-order, and original error boundary.
use xcb::x;

#[derive(Debug)]
pub(super) struct Atoms {
    pub protocols: x::Atom,
    pub delete: x::Atom,
    pub utf8: x::Atom,
    pub name: x::Atom,
    pub pid: x::Atom,
    pub role: x::Atom,
    pub state: x::Atom,
    pub above: x::Atom,
    pub skip_taskbar: x::Atom,
    pub skip_pager: x::Atom,
    pub workarea: x::Atom,
    pub desktop: x::Atom,
    pub resources: x::Atom,
}

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

fn requests() -> [x::InternAtom<'static>; 13] {
    NAMES.map(|name| x::InternAtom {
        only_if_exists: false, // Create a required name when absent; retain existing IDs.
        name,
    })
}

/// Narrow transport port: tests observe actual xcb request fields and supply
/// replies/errors. Neither the port nor its adapter decides bootstrap policy.
pub(super) trait InternTransport {
    type Cookie;
    type Error;
    fn send(&mut self, request: &x::InternAtom<'_>) -> Self::Cookie;
    fn wait(&mut self, cookie: Self::Cookie) -> Result<x::Atom, Self::Error>;
}
impl InternTransport for &xcb::Connection {
    type Cookie = x::InternAtomCookie;
    type Error = xcb::Error;
    fn send(&mut self, request: &x::InternAtom<'_>) -> Self::Cookie {
        self.send_request(request)
    }
    fn wait(&mut self, cookie: Self::Cookie) -> Result<x::Atom, Self::Error> {
        self.wait_for_reply(cookie).map(|reply| reply.atom())
    }
}
impl Atoms {
    pub fn intern_all(connection: &xcb::Connection) -> xcb::Result<Self> {
        Self::intern_with(&mut &*connection)
    }
    pub(super) fn intern_with<T: InternTransport>(transport: &mut T) -> Result<Self, T::Error> {
        let cookies = requests().map(|request| transport.send(&request));
        let mut values = [x::ATOM_NONE; 13];
        for (value, cookie) in values.iter_mut().zip(cookies) {
            *value = transport.wait(cookie)?;
        }
        let [protocols, delete, utf8, name, pid, role, state, above, skip_taskbar, skip_pager, workarea, desktop, resources] =
            values;
        Ok(Self {
            protocols,
            delete,
            utf8,
            name,
            pid,
            role,
            state,
            above,
            skip_taskbar,
            skip_pager,
            workarea,
            desktop,
            resources,
        })
    }
}
