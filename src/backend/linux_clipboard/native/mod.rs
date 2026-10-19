//! Dedicated clipboard owner, scoped to DriverInput. All blocking XCB work uses
//! one preopened socket with a caller-owned shutdown handle; no detached setup
//! task can later connect, publish, or acquire a new transport.
mod auth;
mod connection;
mod control;
mod keys;
mod server;
mod service;
mod transport;
mod worker;

use super::{
    policy::{self, error, GuardSnapshot, ReceiptSlot, RECEIPT_TTL},
    transaction,
};
use crate::backend::BackendError;
use control::Control;
use rpa_native_input::X11Lease;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::{
    atomic::Ordering,
    mpsc::{self, Receiver, SyncSender},
    Arc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use worker::Operation;

const SETUP_BUDGET: Duration = Duration::from_secs(2);
const REQUEST_BUDGET: Duration = Duration::from_millis(500);
const SHUTDOWN_BUDGET: Duration = Duration::from_millis(200);

struct Command {
    operation: Operation,
    deadline: Instant,
    response: SyncSender<Result<(GuardSnapshot, Instant), BackendError>>,
}
struct Worker {
    control: Arc<Control>,
    interrupt: UnixStream,
    commands: SyncSender<Command>,
    join: Option<JoinHandle<()>>,
    watch: Option<JoinHandle<()>>,
}

/// Created lazily with explicit authority. No environment variable can replace
/// a missing lease; DISPLAY/XAUTHORITY only address/authenticate the X server.
pub(crate) struct Clipboard {
    lease: X11Lease,
    worker: Option<Worker>,
    receipts: ReceiptSlot,
    prepared: bool,
    failed: Option<BackendError>,
    #[cfg(test)]
    verify_barrier: Option<verify_failure_tests::VerifyBarrier>,
}
impl Clipboard {
    pub fn new(lease: X11Lease) -> Self {
        Self {
            lease,
            worker: None,
            receipts: ReceiptSlot::default(),
            prepared: false,
            failed: None,
            #[cfg(test)]
            verify_barrier: None,
        }
    }
    pub fn prepare(&mut self, text: &str) -> Result<(), BackendError> {
        self.prepared = false;
        let epoch = self.receipts.begin_prepare();
        if self.lease.is_revoked() {
            return Err(transaction::denied());
        }
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        let bytes = policy::prepare_payload(text, policy::MAX_BYTES)?;
        // BackendCore handles empty paste before this adapter. No clipboard
        // mutation if a caller bypasses the core either; no receipt is armed.
        if bytes.is_empty() {
            return Ok(());
        }
        if self.worker.is_none() {
            self.worker = Some(Worker::start(self.lease.clone())?);
        }
        let result = self
            .worker
            .as_mut()
            .unwrap()
            .request(Operation::Prepare(bytes));
        match result {
            Ok((snapshot, published_at)) => {
                if self.lease.is_revoked() {
                    return Err(transaction::denied());
                }
                self.receipts
                    .arm(epoch, snapshot, published_at, RECEIPT_TTL)?;
                if Instant::now().saturating_duration_since(published_at) >= RECEIPT_TTL {
                    self.receipts.invalidate();
                    return Err(error("paste receipt expired; clipboard already modified"));
                }
                self.prepared = true;
                Ok(())
            }
            Err(e) => {
                self.capture_failure();
                Err(e)
            }
        }
    }
    pub fn verify(&mut self) -> Result<(), BackendError> {
        let prepared = std::mem::replace(&mut self.prepared, false);
        if !prepared || self.lease.is_revoked() {
            self.receipts.invalidate();
            return Err(if self.lease.is_revoked() {
                transaction::denied()
            } else {
                error("no prepared clipboard paste receipt")
            });
        }
        let result = self
            .worker
            .as_mut()
            .ok_or_else(|| error("clipboard owner unavailable"))?
            .request(Operation::Verify)
            .and_then(|(snapshot, _)| {
                if self.lease.is_revoked() {
                    return Err(transaction::denied());
                }
                self.receipts.consume(snapshot, Instant::now())
            });
        self.receipts.invalidate(); // consume even on transport/focus/lease failure
        #[cfg(test)]
        if let Some(barrier) = self.verify_barrier.take() {
            barrier.arrive(&result);
        }
        self.capture_failure();
        // An observed terminal owner failure vetoes even an earlier valid reply.
        // This is a point-in-time check, not a guarantee of future ownership.
        let final_health = self.failed.clone().map_or(Ok(()), Err);
        transaction::combine(result, final_health).map_err(|e| {
            BackendError::new(
                e.code,
                format!(
                    "{}; clipboard already modified; no paste chord dispatched",
                    e.message
                ),
            )
        })
    }
    fn capture_failure(&mut self) {
        if let Some(worker) = &self.worker {
            if let Err(e) = worker.health() {
                self.failed = Some(e);
            }
        }
    }
    /// Session cleanup invalidates authorization, NOT the already published data.
    /// A healthy owner remains available until driver drop. Failed transport
    /// cleanup is reported and retryable; it cannot manufacture a success receipt.
    pub fn cleanup(&mut self) -> Result<(), BackendError> {
        self.prepared = false;
        self.receipts.invalidate();
        if let Some(worker) = self.worker.as_mut() {
            if worker.health().is_err() {
                worker.stop()?;
                self.worker = None;
                // No late writes remain, but the session lost its owner. Preserve
                // the failure for explicit callers rather than silently reconnect.
                return Err(self.failed.clone().unwrap_or_else(|| {
                    error("clipboard owner failed; storage/paste outcome unknown")
                }));
            }
        }
        if let Some(e) = &self.failed {
            return Err(e.clone());
        }
        Ok(())
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        self.receipts.invalidate();
        if let Some(worker) = self.worker.as_mut() {
            let _ = worker.stop();
        }
    }
}
impl Worker {
    fn start(lease: X11Lease) -> Result<Self, BackendError> {
        if lease.is_revoked() {
            return Err(transaction::denied());
        }
        let deadline = Instant::now() + SETUP_BUDGET;
        let wayland = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        if !wayland.trim().is_empty() || session.eq_ignore_ascii_case("wayland") {
            return Err(BackendError::new(
                "unsupported_session",
                "clipboard paste requires native X11, not Wayland/XWayland",
            ));
        }
        let display = std::env::var("DISPLAY")
            .map_err(|_| BackendError::new("no_display", "X11 DISPLAY is missing"))?;
        let number = auth::local_display_number(&display)
            .map_err(|e| BackendError::new("unsupported_session", e))?;
        let cookie = transport::load_cookie(&number, deadline).map_err(error)?;
        let socket = transport::open_socket(&number, deadline).map_err(error)?;
        Self::start_socket(socket, cookie, lease, deadline)
    }
    fn start_socket(
        socket: UnixStream,
        cookie: Option<Vec<u8>>,
        lease: X11Lease,
        deadline: Instant,
    ) -> Result<Self, BackendError> {
        let interrupt = socket
            .try_clone()
            .map_err(|e| error(format!("cannot retain clipboard shutdown socket: {e}")))?;
        let thread_interrupt = interrupt
            .try_clone()
            .map_err(|e| error(format!("cannot retain worker shutdown socket: {e}")))?;
        let control = Arc::new(Control::default());
        control.arm(deadline)?;
        let worker_control = control.clone();
        let (commands, receiver) = mpsc::sync_channel(1);
        let (ready, ready_rx) = mpsc::channel();
        let ready_failure = ready.clone();
        let join = std::thread::Builder::new()
            .name("computer-clipboard-x11".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker::run(
                        socket,
                        cookie,
                        lease,
                        worker_control.clone(),
                        receiver,
                        ready,
                        deadline,
                    )
                }))
                .unwrap_or_else(|_| {
                    Err(error(
                        "clipboard owner worker panicked; clipboard outcome unknown",
                    ))
                });
                // A retained duplicate would otherwise keep a disconnected XCB peer
                // alive. Shut the transport on EVERY worker exit, including panics.
                let _ = thread_interrupt.shutdown(Shutdown::Both);
                if let Err(e) = result {
                    let _ = worker_control.failure.set(e.clone());
                    let _ = ready_failure.send(Err(e));
                }
                worker_control.stop.store(true, Ordering::SeqCst);
            })
            .map_err(|e| error(format!("cannot start clipboard owner: {e}")))?;
        let mut worker = Self {
            control,
            interrupt,
            commands,
            join: Some(join),
            watch: None,
        };
        let watch_socket = worker
            .interrupt
            .try_clone()
            .map_err(|e| error(format!("cannot duplicate watchdog socket: {e}")))?;
        match worker.control.watch(watch_socket) {
            Ok(watch) => worker.watch = Some(watch),
            Err(e) => {
                let cleanup = worker.stop();
                return transaction::combine(Err(e), cleanup);
            }
        }
        match ready_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(())) if Instant::now() < deadline => Ok(worker),
            result => {
                let primary = match result {
                    Ok(Err(e)) => e,
                    _ => error("clipboard owner setup expired; transport interrupted"),
                };
                let cleanup = worker.stop();
                transaction::combine(Err(primary), cleanup)
            }
        }
    }
    fn health(&self) -> Result<(), BackendError> {
        if let Some(e) = self.control.failure.get() {
            return Err(e.clone());
        }
        if self.control.stop.load(Ordering::SeqCst)
            || self.join.as_ref().is_none_or(|j| j.is_finished())
        {
            return Err(error(
                "clipboard owner stopped; storage/paste outcome unknown",
            ));
        }
        Ok(())
    }
    fn request(&mut self, operation: Operation) -> Result<(GuardSnapshot, Instant), BackendError> {
        self.health()?;
        let deadline = Instant::now() + REQUEST_BUDGET;
        let (response, receiver) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command {
                operation,
                deadline,
                response,
            })
            .map_err(|_| error("clipboard owner command unavailable; no automatic retry"))?;
        match receive(&receiver, deadline) {
            Ok(result) => result,
            Err(e) => {
                let _ = self.control.failure.set(e.clone());
                transaction::combine(Err(e), self.stop())
            }
        }
    }
    fn stop(&mut self) -> Result<(), BackendError> {
        self.control.stop.store(true, Ordering::SeqCst);
        // Interrupt setup/read/write/flush BEFORE waiting. This owned duplicate
        // cannot accidentally close a recycled descriptor from another thread.
        let shutdown = self.interrupt.shutdown(Shutdown::Both);
        if let Some(join) = &self.join {
            join.thread().unpark();
        }
        let deadline = Instant::now() + SHUTDOWN_BUDGET;
        if let Some(watch) = &self.watch {
            watch.thread().unpark();
        }
        while [self.join.as_ref(), self.watch.as_ref()]
            .into_iter()
            .flatten()
            .any(|j| !j.is_finished())
            && Instant::now() < deadline
        {
            std::thread::park_timeout(Duration::from_millis(1));
        }
        if [self.join.as_ref(), self.watch.as_ref()]
            .into_iter()
            .flatten()
            .any(|j| !j.is_finished())
        {
            // Keep handle for explicit cleanup retry. Drop still cannot let it
            // write: shutdown has made its only transport permanently unusable.
            return Err(error(
                "clipboard transport interrupted; owner thread termination unconfirmed",
            ));
        }
        if let Some(watch) = self.watch.take() {
            watch
                .join()
                .map_err(|_| error("clipboard watchdog termination panicked"))?;
        }
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| error("clipboard owner termination panicked"))?;
        }
        if let Err(e) = shutdown {
            if e.kind() != std::io::ErrorKind::NotConnected {
                return Err(error(format!(
                    "clipboard transport shutdown uncertain: {e}"
                )));
            }
        }
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
fn receive(
    receiver: &Receiver<Result<(GuardSnapshot, Instant), BackendError>>,
    deadline: Instant,
) -> Result<Result<(GuardSnapshot, Instant), BackendError>, BackendError> {
    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(result) if Instant::now() < deadline => Ok(result),
        _ => Err(error("clipboard operation response expired; transport interrupted; clipboard may already be modified")),
    }
}

#[cfg(test)]
mod transport_tests;

#[cfg(test)]
mod verify_failure_tests;
