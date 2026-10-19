//! X-server-owned cooperative writer lease. No path, UID, DISPLAY spelling or
//! screen index is an identity. The selection namespace spans every root.
//!
//! The private, unmapped owner window dies with this X connection. A leaked
//! DesktopLock intentionally retains the connection for process lifetime. The
//! watcher owns a second reference and revokes the shared driver token on ANY
//! loss/error/panic; drivers independently verify it on their own connection.
use super::LockError;
use rpa_native_input::{X11Lease, X11_LEASE_NONCE_PROPERTY, X11_LEASE_SELECTION};
use std::os::fd::AsRawFd;
#[cfg(test)]
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use xcb::{x, Connection, CookieWithReplyChecked, Xid};

mod acquisition;
mod auth;
mod connection;
mod transport;

const ACQUIRE_BUDGET: Duration = Duration::from_secs(10);
const GRAB_BUDGET: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(10);
const PROBE_PERIOD: Duration = Duration::from_millis(250);
const PROBE_BUDGET: Duration = Duration::from_secs(1);

pub(super) struct ServerLease {
    connection: Arc<Connection>,
    token: X11Lease,
    owner: x::Window,
    selection: x::Atom,
    root: x::Window,
}

impl ServerLease {
    pub(super) fn acquire() -> Result<Self, LockError> {
        let display = std::env::var("DISPLAY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| unavailable("DISPLAY is missing or empty"))?;
        if std::env::var("WAYLAND_DISPLAY").is_ok_and(|s| !s.is_empty())
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s.eq_ignore_ascii_case("wayland"))
        {
            return Err(unavailable(
                "Wayland/XWayland is not supported; an X11 session is required",
            ));
        }
        Self::acquire_display(display, ACQUIRE_BUDGET, SetupProbe::default())
    }

    // Same real-socket seam as the frozen RED probes. Production has the named
    // 10s budget; probes inject 400ms without replacing the XCB protocol path.
    fn acquire_display(
        display: String,
        budget: Duration,
        probe: SetupProbe,
    ) -> Result<Self, LockError> {
        acquisition::run(display, budget, probe)
    }

    fn acquire_on_connection(
        connection: Connection,
        nonce: [u8; 16],
        deadline: Instant,
        budget: &acquisition::Budget,
        _probe: &mut SetupProbe,
    ) -> Result<Self, LockError> {
        let connection = Arc::new(connection);
        budget.check()?;
        let root = connection
            .get_setup()
            .roots()
            .next()
            .ok_or_else(|| unavailable("X server has no root screen"))?
            .root();
        let selection = intern(&connection, X11_LEASE_SELECTION, deadline)?;
        let property = intern(&connection, X11_LEASE_NONCE_PROPERTY, deadline)?;
        let owner: x::Window = connection.generate_id();
        let create = connection.send_request_checked(&x::CreateWindow {
            depth: 0,
            wid: owner,
            parent: root,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            border_width: 0,
            class: x::WindowClass::InputOnly,
            visual: 0,
            value_list: &[x::Cw::EventMask(x::EventMask::STRUCTURE_NOTIFY)],
        });
        let annotate = connection.send_request_checked(&x::ChangeProperty {
            mode: x::PropMode::Replace,
            window: owner,
            property,
            r#type: x::ATOM_STRING,
            data: &nonce,
        });
        barrier(&connection, deadline)?;
        connection.check_request(create).map_err(unavailable)?;
        connection.check_request(annotate).map_err(unavailable)?;
        budget.check()?;
        {
            // No file I/O, connection setup, nonce generation or watcher
            // startup inside the grab. Only the server-wide check/set/check.
            let short_deadline = budget.enter_grab()?;
            let mut grab = ServerGrab::new(&connection, budget);
            let owner_query = connection.send_request(&x::GetSelectionOwner { selection });
            #[cfg(test)]
            _probe.before_owner_reply(&connection);
            let current = reply(&connection, owner_query, short_deadline)?;
            // The owner reply is also a bounded barrier for GrabServer.
            connection
                .check_request(grab.cookie.take().expect("grab checked once"))
                .map_err(unavailable)?;
            if !current.owner().is_none() {
                return Err(LockError::Contended);
            }
            budget.check()?;
            let set = connection.send_request_checked(&x::SetSelectionOwner {
                owner,
                selection,
                time: x::CURRENT_TIME,
            });
            let current = reply(
                &connection,
                connection.send_request(&x::GetSelectionOwner { selection }),
                short_deadline,
            )?;
            connection.check_request(set).map_err(unavailable)?;
            if current.owner() != owner {
                return Err(unavailable("X server did not confirm writer ownership"));
            }
        } // ungrab before any publication/watcher work
        barrier(&connection, deadline)?;
        let token = X11Lease::new(selection.resource_id(), owner.resource_id(), nonce);
        let lease = Self {
            connection: connection.clone(),
            token: token.clone(),
            owner,
            selection,
            root,
        };
        budget.check()?;
        Ok(lease)
    }

    fn start_monitor(&self) -> Result<(), LockError> {
        let monitor = MonitorGuard {
            connection: self.connection.clone(),
            token: self.token.clone(),
        };
        let selection = self.selection;
        let owner = self.owner;
        std::thread::Builder::new()
            .name("computer-x11-lease".into())
            .spawn(move || monitor.run(selection, owner))
            .map_err(|e| unavailable(format!("cannot monitor X11 ownership: {e}")))?;
        Ok(())
    }

    pub(super) fn token(&self) -> Result<X11Lease, LockError> {
        if self.token.is_revoked() {
            Err(unavailable(
                "X11 desktop writer authority has been permanently revoked",
            ))
        } else {
            Ok(self.token.clone())
        }
    }

    pub(super) fn diagnostic(&self) -> String {
        format!(
            "X11 server selection {X11_LEASE_SELECTION}, owner=0x{:x}, first_root=0x{:x}",
            self.owner.resource_id(),
            self.root.resource_id()
        )
    }
}

impl Drop for ServerLease {
    fn drop(&mut self) {
        self.token.revoke();
        disconnect(&self.connection);
    }
}

struct ServerGrab<'a> {
    connection: &'a Connection,
    budget: &'a acquisition::Budget,
    cookie: Option<xcb::VoidCookieChecked>,
}
impl<'a> ServerGrab<'a> {
    fn new(connection: &'a Connection, budget: &'a acquisition::Budget) -> Self {
        Self {
            connection,
            budget,
            cookie: Some(connection.send_request_checked(&x::GrabServer {})),
        }
    }
}
impl Drop for ServerGrab<'_> {
    fn drop(&mut self) {
        self.connection.send_request(&x::UngrabServer {});
        if self.connection.flush().is_err() {
            disconnect(self.connection);
        }
        self.budget.leave_grab();
    }
}

/// Owned before spawning: failure to spawn or panic in the watcher also
/// revokes, shuts down the socket, and cannot leave an unmonitored authority.
struct MonitorGuard {
    connection: Arc<Connection>,
    token: X11Lease,
}
impl MonitorGuard {
    fn run(&self, selection: x::Atom, owner: x::Window) {
        let mut probe = None;
        let mut next_probe = Instant::now();
        let mut probe_deadline = Instant::now();
        while !self.token.is_revoked() {
            if self.connection.has_error().is_err() {
                return;
            }
            // Bound event draining so an event flood cannot starve the probe.
            for _ in 0..32 {
                match self.connection.poll_for_event() {
                    Ok(Some(xcb::Event::X(x::Event::SelectionClear(e))))
                        if e.selection() == selection =>
                    {
                        return
                    }
                    Ok(Some(xcb::Event::X(x::Event::DestroyNotify(e)))) if e.window() == owner => {
                        return
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(_) => return,
                }
            }
            if probe.is_none() && Instant::now() >= next_probe {
                probe = Some(
                    self.connection
                        .send_request(&x::GetSelectionOwner { selection }),
                );
                if self.connection.flush().is_err() {
                    return;
                }
                probe_deadline = Instant::now() + PROBE_BUDGET;
            }
            if let Some(cookie) = &probe {
                if let Some(result) = self.connection.poll_for_reply(cookie) {
                    match result {
                        Ok(r) if r.owner() == owner => {}
                        _ => return,
                    }
                    probe = None;
                    next_probe = Instant::now() + PROBE_PERIOD;
                } else if Instant::now() >= probe_deadline {
                    return;
                }
            }
            std::thread::sleep(POLL);
        }
    }
}
impl Drop for MonitorGuard {
    fn drop(&mut self) {
        self.token.revoke();
        disconnect(&self.connection);
        eprintln!("[computer-host] X11 desktop writer lease revoked; further input is forbidden");
    }
}

fn disconnect(connection: &Connection) {
    // Do not close a borrowed fd (libxcb owns it). shutdown affects every
    // reference to this socket and prevents buffered requests being sent.
    unsafe {
        libc::shutdown(connection.as_raw_fd(), libc::SHUT_RDWR);
    }
}
fn unavailable(error: impl std::fmt::Display) -> LockError {
    LockError::Unavailable(error.to_string())
}
fn intern(connection: &Connection, name: &str, deadline: Instant) -> Result<x::Atom, LockError> {
    Ok(reply(
        connection,
        connection.send_request(&x::InternAtom {
            only_if_exists: false,
            name: name.as_bytes(),
        }),
        deadline,
    )?
    .atom())
}
fn barrier(connection: &Connection, deadline: Instant) -> Result<(), LockError> {
    reply(
        connection,
        connection.send_request(&x::GetInputFocus {}),
        deadline,
    )
    .map(|_| ())
}
fn reply<C: CookieWithReplyChecked>(
    connection: &Connection,
    cookie: C,
    deadline: Instant,
) -> Result<C::Reply, LockError> {
    transport::check_deadline(deadline)?;
    connection.flush().map_err(unavailable)?;
    loop {
        if Instant::now() >= deadline {
            // Especially important under the grab: interrupt the connection
            // instead of waiting indefinitely for another protocol reply.
            disconnect(connection);
            return Err(unavailable(
                "X11 ownership protocol reply exceeded its deadline",
            ));
        }
        connection.has_error().map_err(unavailable)?;
        if let Some(result) = connection.poll_for_reply(&cookie) {
            transport::check_deadline(deadline)?;
            return result.map_err(unavailable);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

// Observes real task lifetime and can deterministically hold one scheduling
// boundary. Empty in production; it does not replace XCB or timeout behavior.
#[derive(Default)]
struct SetupProbe {
    #[cfg(test)]
    retired: Option<mpsc::Sender<()>>,
    #[cfg(test)]
    before_owner_reply: Option<Box<dyn FnOnce(&Connection) + Send>>,
}
#[cfg(test)]
impl SetupProbe {
    fn before_owner_reply(&mut self, connection: &Connection) {
        if let Some(hook) = self.before_owner_reply.take() {
            connection
                .flush()
                .expect("flush real owner query before schedule gate");
            hook(connection);
        }
    }
}
#[cfg(test)]
impl Drop for SetupProbe {
    fn drop(&mut self) {
        if let Some(retired) = &self.retired {
            let _ = retired.send(());
        }
    }
}

#[cfg(test)]
mod acquire_socket_tests;
