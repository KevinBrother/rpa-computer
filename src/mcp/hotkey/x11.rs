//! Dedicated core-XCB registration. The caller owns a duplicate socket for
//! deadline shutdown; there are no detached setup tasks or global callbacks.
mod candidate_gate;
mod connection;
#[cfg(test)]
mod invalidation_diagnostic;
mod revalidation;
mod semantic_image;
mod transport;
#[cfg(test)]
mod transport_tests;
mod watchdog;
mod xkb_aux_wire;
mod xkb_events;
mod xkb_map;
mod xkb_wire;

use super::x11_auth::local_display_number;
use super::x11_candidate::{Candidate, Decision};
use super::x11_lifecycle::Control;
use super::x11_plan::{self, KeyboardMap};
use super::HotkeyStatus;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use xcb::x;

const INIT_BUDGET: Duration = Duration::from_secs(2);
const CLEANUP_GRACE: Duration = Duration::from_millis(200);
const IDLE_POLL: Duration = Duration::from_millis(10);
const EVENT_BATCH: usize = 64;

pub(super) struct Registration {
    control: Arc<Control>,
    interrupt: Arc<UnixStream>,
    done: Receiver<()>,
    join: JoinHandle<()>,
    watchdog: watchdog::Watchdog,
}

impl Registration {
    pub(super) fn failure(&self) -> Option<&HotkeyStatus> {
        self.control.failure.get()
    }

    pub(super) fn disarm(self) {
        self.control.stop.store(true, Ordering::SeqCst);
        self.join.thread().unpark();
        // Give healthy servers a chance to acknowledge explicit UngrabKey.
        // A stalled handshake, checked request, flush or cleanup instead gets
        // transport shutdown, NOT an abandoned registration thread.
        if self.done.recv_timeout(CLEANUP_GRACE).is_err() {
            let _ = self.interrupt.shutdown(Shutdown::Both);
        }
        // All worker I/O uses that same socket. After shutdown it cannot keep
        // blocking in XCB or acquire a new connection. Work per loop is bounded.
        let _ = self.join.join();
        self.watchdog.finish();
    }
}

pub(super) fn arm(cancel: Arc<AtomicBool>) -> Result<Registration, String> {
    let deadline = Instant::now() + INIT_BUDGET;
    let display = std::env::var("DISPLAY").ok();
    x11_plan::check_session(
        display.as_deref(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )?;
    let number = local_display_number(display.as_deref().unwrap_or_default())?;
    // Tracked F4 blocker: inherited authority-file I/O is still synchronous.
    // Do not claim end-to-end bounded setup until the shared credential-only
    // helper passes native containment and this consumer is explicitly migrated.
    let cookie = transport::load_cookie(&number, deadline)?;
    let socket = transport::open_socket(&number, deadline)?;
    arm_socket(socket, cookie, cancel, deadline)
}

fn arm_socket(
    socket: UnixStream,
    mut cookie: Option<Vec<u8>>,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
) -> Result<Registration, String> {
    transport::check_deadline(deadline)?;
    // try_clone creates another OWNED descriptor of the same socket. It is
    // retained before XCB can block in setup and is never a borrowed raw fd.
    let interrupt = Arc::new(
        socket
            .try_clone()
            .map_err(|e| format!("cannot duplicate X11 socket: {e}"))?,
    );
    let control = Arc::new(Control::default());
    let worker_control = Arc::clone(&control);
    let worker_interrupt = Arc::clone(&interrupt);
    let (ready_tx, ready_rx) = channel();
    let (done_tx, done_rx) = channel();
    let watchdog = watchdog::Watchdog::start(
        Arc::clone(&interrupt),
        Arc::clone(&control),
        Arc::clone(&cancel),
        deadline,
    )?;
    let worker_lease = watchdog.lease();
    let join = std::thread::Builder::new()
        .name("computer-hotkey-x11".into())
        .spawn(move || {
            // A constructor assertion racing a deadline is still fail-closed and
            // releases its raw connection via RAII. No panic crosses arm's API.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                serve(
                    socket,
                    cookie.as_deref(),
                    &cancel,
                    &worker_control,
                    deadline,
                    &ready_tx,
                    &worker_lease,
                )
            }))
            .unwrap_or_else(|_| {
                Err("X11 hotkey worker terminated during setup/event handling".into())
            });
            if let Some(cookie) = cookie.as_mut() {
                cookie.fill(0);
            }
            // Even a duplicate descriptor would keep the server connection alive
            // after xcb_disconnect closes its original fd. Shutdown closes the
            // transport now, so all partial/active grabs are released on every exit.
            if let Err(error) = &result {
                worker_control.begin_failed_cleanup(error, &cancel);
            }
            let _ = worker_interrupt.shutdown(Shutdown::Both);
            if let Err(error) = result {
                let _ = ready_tx.send(Err(error));
            }
            worker_lease.finish();
            drop(cancel);
            let _ = done_tx.send(());
        });
    let join = match join {
        Ok(join) => join,
        Err(error) => {
            watchdog.finish();
            return Err(format!("cannot spawn X11 hotkey worker: {error}"));
        }
    };
    let registration = Registration {
        control,
        interrupt,
        done: done_rx,
        join,
        watchdog,
    };
    let result = ready_rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    match result {
        Ok(Ok(())) if registration.control.accept_ready(deadline) => {
            registration.join.thread().unpark();
            Ok(registration)
        }
        result => {
            // Invalidate the connection immediately, before waiting for any
            // cleanup. Late queued grabs cannot outlive this connection.
            // Preserve the first failure (not the secondary XCB error produced
            // by the watchdog's shutdown). In particular do not hide timeout
            // diagnostics, or replace an actual grab collision with a timeout.
            let failure = match registration.control.failure.get() {
                Some(HotkeyStatus::Failed(error)) => Some(error.clone()),
                _ => None,
            };
            registration.control.stop.store(true, Ordering::SeqCst);
            let _ = registration.interrupt.shutdown(Shutdown::Both);
            registration.disarm();
            Err(failure.unwrap_or_else(|| match result {
                Ok(Err(error)) => error,
                _ => "X11 hotkey initialization timed out or worker exited; connection shut down"
                    .into(),
            }))
        }
    }
}

fn ensure_initializing(control: &Control, deadline: Instant) -> Result<(), String> {
    transport::check_deadline(deadline)?;
    if control.stop.load(Ordering::SeqCst) || control.failure.get().is_some() {
        Err("X11 hotkey initialization abandoned or failed".into())
    } else {
        Ok(())
    }
}

fn serve(
    socket: UnixStream,
    cookie: Option<&[u8]>,
    cancel: &AtomicBool,
    control: &Control,
    deadline: Instant,
    ready: &Sender<Result<(), String>>,
    lease: &watchdog::Lease,
) -> Result<(), String> {
    ensure_initializing(control, deadline)?;
    let connection = connection::connect(socket.into(), cookie)?;
    // Core reachability probe preserves early transport failure detection. The
    // authoritative map is read afresh AFTER XKB subscriptions are installed.
    read_map(&connection)?;
    let connection = connection::with_xkb(connection)?;
    let result = (|| {
        ensure_initializing(control, deadline)?;
        xkb_events::subscribe(&connection)?;
        // Retain the complete validated configuration and the actual coherent
        // capture poll count for the post-grab/preready handoff.
        let mut capture_pending = std::collections::VecDeque::new();
        let mut baseline = revalidation::capture_initial(
            &mut revalidation::LiveSource::new(&connection, control),
            &mut capture_pending,
            deadline,
        )?;
        let plan = &baseline.plan;
        if connection.get_setup().roots().next().is_none() {
            return Err("X11 server has no root windows".into());
        }
        for screen in connection.get_setup().roots() {
            for &key in &plan.keycodes {
                // Core GrabKey accepts modifier bits, not XKB group-state bits.
                // Delivery is independently validated in every effective group.
                for &mask in &plan.modifier_masks {
                    ensure_initializing(control, deadline)?;
                    connection.check_request(connection.send_request_checked(&x::GrabKey {
                        owner_events: false,
                        grab_window: screen.root(),
                        modifiers: x::ModMask::from_bits_truncate(u32::from(mask)),
                        key,
                        pointer_mode: x::GrabMode::Async,
                        keyboard_mode: x::GrabMode::Sync,
                    })).map_err(|e| format!("X11 GrabKey Ctrl+Alt+Shift+Escape failed (collision/invalid registration): {e}"))?;
                    // check_request alone may report Ok on a lost connection.
                    connection
                        .has_error()
                        .map_err(|e| format!("X11 connection lost during GrabKey: {e}"))?;
                }
            }
        }
        // N2: exact post-grab/pre-ready boundary. Do not reset to the old
        // candidate drain's separate256 budget or drop Release/Other here.
        revalidation::finish_initial(
            &mut revalidation::LiveSource::new(&connection, control),
            &mut baseline,
            &mut capture_pending,
            deadline,
        )?;
        let pending = capture_pending.into_iter().collect();
        // This is not an atomic snapshot of all future server state. Recheck
        // deadline/stop/failure at publication; the caller's existing locked
        // accept_ready transition rechecks lifecycle before reporting Armed.
        ensure_initializing(control, deadline)?;
        ready
            .send(Ok(()))
            .map_err(|_| "X11 hotkey caller abandoned registration")?;
        while !control.active.load(Ordering::SeqCst) {
            ensure_initializing(control, deadline)?;
            std::thread::park_timeout(IDLE_POLL);
        }
        lease.clear();
        pump(&connection, baseline, pending, cancel, control, lease)
    })();
    // Publish failure BEFORE cleanup, so queued readiness cannot win afterward.
    // A live server acknowledges rollback before another registration probes it;
    // the same socket-owned lease bounds this cleanup if the server then stalls.
    if let Err(error) = &result {
        control.begin_failed_cleanup(error, cancel);
        if !control.stop.load(Ordering::SeqCst) {
            lease.begin(CLEANUP_GRACE);
            let _ = ungrab_all(&connection);
        }
        // Disconnect is the fallback for failed/timed-out rollback and releases
        // any active grab. A stopped server observes it only when it resumes.
        return result;
    }
    lease.begin(CLEANUP_GRACE);
    let cleanup = ungrab_all(&connection);
    result.and(cleanup)
}

fn read_map(connection: &xcb::Connection) -> Result<KeyboardMap, String> {
    let setup = connection.get_setup();
    let first = setup.min_keycode();
    let count = setup
        .max_keycode()
        .checked_sub(first)
        .and_then(|n| n.checked_add(1))
        .ok_or("invalid X11 keycode range")?;
    let keys = connection
        .wait_for_reply(connection.send_request(&x::GetKeyboardMapping {
            first_keycode: first,
            count,
        }))
        .map_err(|e| format!("cannot read X11 keyboard mapping: {e}"))?;
    let modifiers = connection
        .wait_for_reply(connection.send_request(&x::GetModifierMapping {}))
        .map_err(|e| format!("cannot read X11 modifier mapping: {e}"))?;
    let width = usize::from(modifiers.keycodes_per_modifier());
    if modifiers.keycodes().len() != width * 8 {
        return Err("invalid X11 modifier mapping length".into());
    }
    Ok(KeyboardMap {
        first_keycode: first,
        keysyms_per_keycode: usize::from(keys.keysyms_per_keycode()),
        keysyms: keys.keysyms().to_vec(),
        modifier_keycodes: std::array::from_fn(|slot| {
            modifiers.keycodes()[slot * width..(slot + 1) * width].to_vec()
        }),
    })
}

fn ungrab_all(connection: &xcb::Connection) -> Result<(), String> {
    connection
        .has_error()
        .map_err(|e| format!("X11 hotkey transport already closed; disconnecting: {e}"))?;
    for screen in connection.get_setup().roots() {
        // ANY here only releases THIS dedicated client's grabs, never those of
        // another owner. No new passive grabs are installed with AnyModifier.
        connection
            .check_request(connection.send_request_checked(&x::UngrabKey {
                key: 0,
                grab_window: screen.root(),
                modifiers: x::ModMask::ANY,
            }))
            .map_err(|e| format!("X11 hotkey ungrab failed; disconnecting: {e}"))?;
    }
    connection
        .has_error()
        .map_err(|e| format!("X11 hotkey connection closed during cleanup: {e}"))
}

fn pump(
    connection: &xcb::Connection,
    mut baseline: revalidation::Baseline,
    pending: Vec<xcb::Event>,
    cancel: &AtomicBool,
    control: &Control,
    lease: &watchdog::Lease,
) -> Result<(), String> {
    let mut pending: std::collections::VecDeque<_> = pending.into();
    while !control.stop.load(Ordering::SeqCst) {
        for _ in 0..EVENT_BATCH {
            if control.stop.load(Ordering::SeqCst) {
                return Ok(());
            }
            let event = match pending.pop_front() {
                Some(event) => Some(event),
                None => connection
                    .poll_for_event()
                    .map_err(|e| format!("X11 event pump failed: {e}"))?,
            };
            let Some(event) = event else {
                break;
            };
            // Covers any reply wait caused by event interpretation, including
            // non-dependent action-map changes and synchronous candidate replay.
            // This deadline is shared by all revalidation reads/fence/queue,
            // while the already-started socket lease remains authoritative.
            let capture_deadline = Instant::now() + CLEANUP_GRACE;
            lease.begin(CLEANUP_GRACE);
            if let xcb::Event::Xkb(xcb::xkb::Event::NewKeyboardNotify(notice)) = &event {
                if notice.response_type() & 0x80 == 0 {
                    let result = revalidation::revalidate(
                        &mut baseline,
                        revalidation::Trigger::Equivalent(revalidation::Notice::from_event(notice)),
                        &mut revalidation::LiveSource::new(connection, control),
                        &mut pending,
                        capture_deadline,
                    );
                    if result.is_err() {
                        #[cfg(test)]
                        invalidation_diagnostic::first(&event);
                    }
                    result?; // Full coherent proof required; metadata alone cannot authorize.
                    lease.clear();
                    continue;
                }
            }
            if xkb_events::changed(
                connection,
                &event,
                &baseline.validated,
                &baseline.image.core,
                &baseline.plan,
            )? {
                #[cfg(test)]
                invalidation_diagnostic::first(&event);
                return Err("XKB/core map validity changed; emergency registration revoked".into());
            }
            if let Some(scratch) = revalidation::scratch_notice(&event)? {
                revalidation::revalidate(
                    &mut baseline,
                    revalidation::Trigger::Scratch(scratch),
                    &mut revalidation::LiveSource::new(connection, control),
                    &mut pending,
                    capture_deadline,
                )?;
            }
            let map = &baseline.image.core;
            let plan = &baseline.plan;
            let valid = &baseline.validated;
            if let xcb::Event::X(x::Event::KeyPress(event)) = event {
                if event.response_type() & 0x80 == 0 {
                    let state = connection
                        .wait_for_reply(connection.send_request(&x::QueryKeymap {}))
                        .map_err(|e| format!("cannot resolve frozen physical modifiers: {e}"))?;
                    let held: Vec<u8> = (8..=255u8)
                        .filter(|key| state.keys()[usize::from(*key) / 8] & (1 << (*key % 8)) != 0)
                        .collect();
                    let Some(decision) = candidate_gate::check_then_decide(
                        &pending,
                        || xkb_events::validate_queue(connection, valid, map, plan),
                        &control.stop,
                        plan,
                        cancel,
                        Candidate {
                            keycode: event.detail(),
                            state: event.state().bits() as u16,
                            is_press: true,
                            synthetic: false,
                            held_keys: &held,
                            modifiers: &valid.modifiers,
                        },
                    )?
                    else {
                        return Ok(());
                    };
                    match decision {
                        Decision::Replay => connection
                            .check_request(connection.send_request_checked(&x::AllowEvents {
                                mode: x::Allow::ReplayKeyboard,
                                time: event.time(),
                            }))
                            .map_err(|e| format!("cannot replay non-emergency chord: {e}"))?,
                        Decision::Cancel => {
                            connection
                                .check_request(connection.send_request_checked(
                                    &x::UngrabKeyboard { time: event.time() },
                                ))
                                .map_err(|e| format!("cannot release emergency active grab: {e}"))?
                        }
                    }
                }
            }
            connection
                .has_error()
                .map_err(|e| format!("X11 emergency connection lost: {e}"))?;
            lease.clear();
        }
        connection
            .has_error()
            .map_err(|e| format!("X11 emergency connection lost: {e}"))?;
        std::thread::park_timeout(IDLE_POLL);
    }
    Ok(())
}
