//! Single-connection core XCB operations. Socket shutdown is owned by the caller.
use super::{keys, Control};
use crate::backend::linux_clipboard::{
    policy::{error, GuardSnapshot},
    protocol::{Atoms, SelectionData},
    transaction::{self, AuthoritySnapshot, PrepareIo},
};
use crate::backend::BackendError;
use rpa_native_input::{X11Lease, X11_LEASE_NONCE_PROPERTY, X11_LEASE_SELECTION};
use std::collections::VecDeque;
use std::os::fd::AsRawFd;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use xcb::{x, Connection, CookieWithReplyChecked, Event, Xid, XidNew};

pub(super) const SHORT_GRAB: Duration = Duration::from_millis(100);
pub(super) const EVENT_BUDGET: Duration = Duration::from_millis(250);
pub(super) const MAX_QUEUED: usize = 128;
pub(super) fn atom(id: u32) -> x::Atom {
    x::Atom::new(id)
}
pub(super) fn window(id: u32) -> x::Window {
    x::Window::new(id)
}
pub(super) fn xerror(e: impl std::fmt::Display) -> BackendError {
    error(format!("X11 clipboard protocol failure: {e}"))
}

pub(super) struct Context {
    pub c: Connection,
    pub control: Arc<Control>,
}
impl Context {
    pub fn live(&self, deadline: Instant) -> Result<(), BackendError> {
        if self.control.stop.load(Ordering::SeqCst) || Instant::now() >= deadline {
            return Err(error("X11 clipboard operation cancelled or expired"));
        }
        self.c.has_error().map_err(xerror)
    }
    pub fn barrier(&self, deadline: Instant) -> Result<(), BackendError> {
        reply(self, self.c.send_request(&x::GetInputFocus {}), deadline).map(|_| ())
    }
    pub fn checked(
        &self,
        cookie: xcb::VoidCookieChecked,
        deadline: Instant,
    ) -> Result<(), BackendError> {
        self.barrier(deadline)?; // processed cookie before xcb_request_check; no unbounded reply wait
        self.c.check_request(cookie).map_err(xerror)
    }
    pub fn intern(&self, name: &str, deadline: Instant) -> Result<u32, BackendError> {
        Ok(reply(
            self,
            self.c.send_request(&x::InternAtom {
                only_if_exists: false,
                name: name.as_bytes(),
            }),
            deadline,
        )?
        .atom()
        .resource_id())
    }
    pub fn selection_owner(&self, selection: u32, deadline: Instant) -> Result<u32, BackendError> {
        Ok(reply(
            self,
            self.c.send_request(&x::GetSelectionOwner {
                selection: atom(selection),
            }),
            deadline,
        )?
        .owner()
        .resource_id())
    }
}
pub(super) fn reply<C: CookieWithReplyChecked>(
    ctx: &Context,
    cookie: C,
    deadline: Instant,
) -> Result<C::Reply, BackendError> {
    ctx.live(deadline)?;
    ctx.c.flush().map_err(xerror)?; // caller's retained socket can interrupt blocked XCB flush
    loop {
        ctx.live(deadline)?;
        if let Some(result) = ctx.c.poll_for_reply(&cookie) {
            return result.map_err(xerror);
        }
        // Poll only this server socket. No sleeping or waiting for another client
        // inside a grab, and never a wait_for_reply that outlives our deadline.
        let mut fd = libc::pollfd {
            fd: ctx.c.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .clamp(1, 5) as i32;
        let result = unsafe { libc::poll(&mut fd, 1, ms) };
        if result < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return Err(xerror(std::io::Error::last_os_error()));
        }
    }
}

pub(super) struct State {
    pub ctx: Context,
    pub atoms: Atoms,
    pub owner: u32,
    pub writer: u32,
    pub nonce_property: u32,
    pub clock_property: u32,
    pub inline_budget: usize,
    pub generation: u64,
    pub data: Option<SelectionData>,
    pub candidate: Option<SelectionData>,
    pub deferred: VecDeque<Event>,
    pub lease: X11Lease,
    pub grab_deadline: Option<Instant>,
}
impl State {
    pub fn new(
        c: Connection,
        control: Arc<Control>,
        lease: X11Lease,
        deadline: Instant,
    ) -> Result<Self, BackendError> {
        let ctx = Context { c, control };
        ctx.live(deadline)?;
        let xwayland = reply(
            &ctx,
            ctx.c.send_request(&x::QueryExtension { name: b"XWAYLAND" }),
            deadline,
        )?;
        if xwayland.present() {
            return Err(BackendError::new(
                "unsupported_session",
                "explicit clipboard paste does not support XWayland",
            ));
        }
        let atoms = Atoms {
            clipboard: ctx.intern("CLIPBOARD", deadline)?,
            targets: ctx.intern("TARGETS", deadline)?,
            utf8: ctx.intern("UTF8_STRING", deadline)?,
            timestamp: ctx.intern("TIMESTAMP", deadline)?,
            multiple: ctx.intern("MULTIPLE", deadline)?,
            atom: x::ATOM_ATOM.resource_id(),
            atom_pair: ctx.intern("ATOM_PAIR", deadline)?,
            integer: x::ATOM_INTEGER.resource_id(),
            incr: ctx.intern("INCR", deadline)?,
        };
        let writer = ctx.intern(X11_LEASE_SELECTION, deadline)?;
        let nonce_property = ctx.intern(X11_LEASE_NONCE_PROPERTY, deadline)?;
        let clock_property = ctx.intern("_RPA_COMPUTER_CLIPBOARD_CLOCK_V1", deadline)?;
        let root = ctx
            .c
            .get_setup()
            .roots()
            .next()
            .ok_or_else(|| error("X11 clipboard server has no root"))?
            .root();
        // Core limit only: no hidden BIG-REQUESTS negotiation or unbounded request.
        let inline_budget = usize::from(ctx.c.get_setup().maximum_request_length()) * 4;
        let inline_budget = inline_budget
            .saturating_sub(24)
            .min(super::super::policy::MAX_BYTES);
        let owner: x::Window = ctx.c.generate_id();
        let create = ctx.c.send_request_checked(&x::CreateWindow {
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
            value_list: &[x::Cw::EventMask(x::EventMask::PROPERTY_CHANGE)],
        });
        ctx.checked(create, deadline)?;
        Ok(Self {
            ctx,
            atoms,
            owner: owner.resource_id(),
            writer,
            nonce_property,
            clock_property,
            inline_budget,
            generation: 0,
            data: None,
            candidate: None,
            deferred: VecDeque::new(),
            lease,
            grab_deadline: None,
        })
    }
    pub fn capture_guards(&mut self, deadline: Instant) -> Result<GuardSnapshot, BackendError> {
        self.queue_pending()?; // observe even a steal-and-return before trusting owner equality
        let focus = reply(
            &self.ctx,
            self.ctx.c.send_request(&x::GetInputFocus {}),
            deadline,
        )?
        .focus()
        .resource_id();
        let blocked = keys::blocked(&self.ctx, deadline)?;
        let clipboard_owner = self.ctx.selection_owner(self.atoms.clipboard, deadline)?;
        let confirm = reply(
            &self.ctx,
            self.ctx.c.send_request(&x::GetInputFocus {}),
            deadline,
        )?
        .focus()
        .resource_id();
        if confirm != focus {
            return Err(error("paste focus changed during inspection"));
        }
        self.queue_pending()?;
        Ok(GuardSnapshot {
            focus,
            clipboard_owner,
            owner_generation: self.generation,
            blocked,
        })
    }
    pub fn inspect_authority(&self, deadline: Instant) -> Result<AuthoritySnapshot, BackendError> {
        let selection_owner = self.ctx.selection_owner(self.writer, deadline)?;
        let property = if selection_owner != 0 {
            Some(reply(
                &self.ctx,
                self.ctx.c.send_request(&x::GetProperty {
                    delete: false,
                    window: window(selection_owner),
                    property: atom(self.nonce_property),
                    r#type: x::ATOM_STRING,
                    long_offset: 0,
                    long_length: 4,
                }),
                deadline,
            )?)
        } else {
            None
        };
        Ok(AuthoritySnapshot {
            canonical_selection: self.writer,
            selection_owner,
            nonce_is_string: property
                .as_ref()
                .is_some_and(|p| p.r#type() == x::ATOM_STRING),
            nonce_format: property.as_ref().map_or(0, |p| p.format()),
            nonce: property
                .as_ref()
                .filter(|p| p.format() == 8)
                .map_or_else(Vec::new, |p| p.value::<u8>().to_vec()),
            nonce_bytes_after: property.as_ref().map_or(0, |p| p.bytes_after()),
        })
    }
    pub fn verify(&mut self, deadline: Instant) -> Result<GuardSnapshot, BackendError> {
        if self.lease.is_revoked() {
            return Err(transaction::denied());
        }
        self.grab(deadline)?;
        let result = (|| {
            transaction::validate(
                &self.lease,
                &self
                    .inspect_authority(deadline)
                    .inspect_err(|_| self.lease.revoke())?,
            )?;
            let snapshot = self.capture_guards(deadline)?;
            if self.lease.is_revoked() {
                return Err(transaction::denied());
            }
            Ok(snapshot)
        })();
        let cleanup = self.ungrab();
        transaction::combine(result, cleanup)
    }
}
impl PrepareIo for State {
    fn cancelled(&self) -> bool {
        self.ctx.control.stop.load(Ordering::SeqCst)
    }
    fn grab(&mut self, deadline: Instant) -> Result<(), BackendError> {
        self.ctx.live(deadline)?;
        self.ctx.control.arm(deadline)?;
        self.grab_deadline = Some(deadline);
        let cookie = self.ctx.c.send_request_checked(&x::GrabServer {});
        if let Err(e) = self.ctx.checked(cookie, deadline) {
            // A failed barrier cannot tell whether GrabServer reached the server.
            // Worker treats this connection as poisoned; caller interrupts it.
            self.ctx.control.stop.store(true, Ordering::SeqCst);
            return Err(e);
        }
        Ok(())
    }
    fn authority(&mut self, deadline: Instant) -> Result<AuthoritySnapshot, BackendError> {
        self.inspect_authority(deadline)
    }
    fn guards(&mut self, deadline: Instant) -> Result<GuardSnapshot, BackendError> {
        self.capture_guards(deadline)
    }
    fn publish_clipboard(
        &mut self,
        _: &[u8],
        deadline: Instant,
    ) -> Result<GuardSnapshot, BackendError> {
        self.ctx.live(deadline)?;
        if self.lease.is_revoked() {
            return Err(transaction::denied());
        }
        let acquired = self
            .candidate
            .as_ref()
            .ok_or_else(|| error("missing prepared clipboard payload"))?
            .acquired();
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("clipboard generation exhausted"))?;
        let set = self.ctx.c.send_request_checked(&x::SetSelectionOwner {
            owner: window(self.owner),
            selection: atom(self.atoms.clipboard),
            time: acquired,
        });
        self.ctx.checked(set, deadline)?;
        if self.ctx.selection_owner(self.atoms.clipboard, deadline)? != self.owner {
            return Err(error("X11 server did not confirm clipboard ownership"));
        }
        self.data = self.candidate.take();
        self.generation = generation;
        self.capture_guards(deadline)
    }
    fn ungrab(&mut self) -> Result<(), BackendError> {
        let deadline = self.grab_deadline.take().unwrap_or_else(Instant::now);
        // Always enqueue Ungrab, even if cancelled. Failure is terminal and caller
        // shutdown releases any surviving grab; no new connection or retry.
        let cookie = self.ctx.c.send_request_checked(&x::UngrabServer {});
        if let Err(e) = self.ctx.checked(cookie, deadline) {
            self.ctx.control.stop.store(true, Ordering::SeqCst);
            return Err(e);
        }
        Ok(())
    }
}

pub(super) use crate::backend::linux_clipboard::protocol::before as before_clear;
