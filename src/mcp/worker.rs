//! Worker threads owning the backend, the runtime, and native calls.
//!
//! Threading model (designed so cancellation and shutdown never wait behind
//! a possibly-stuck native operation):
//!
//! - **coordinator thread** (`computer-runtime`): owns the command queue,
//!   the session lifetime watchdog, and the cancel policy. It never runs
//!   native code, so a stuck `capture()`/`inject()` cannot block
//!   EOF/timeout/quit handling.
//! - **native thread** (`computer-native`): owns the [`Runtime`] and the
//!   backend. Both are constructed inside this thread (the backend is not
//!   required to be `Send` for the caller) and all OS resources live and die
//!   here. One native call is in flight at a time.
//!
//! Cancellation and lifetime rules (reviewed):
//! - The cancel flag is set immediately on pause/close/cancelled
//!   notification/EOF/session-lifetime timeout; the runtime polls it between
//!   atomic input events.
//! - The session lifetime budget is an ABSOLUTE wall-clock deadline anchored
//!   at a *successful* `computer_open`; requests do NOT extend it. Only a
//!   *successful* `computer_close` (or an open failure / watchdog close)
//!   changes the timer — an invalid close must not erase a live session's
//!   deadline.
//! - Stop generations: every stop bumps a monotonically increasing
//!   generation STAMPED AT TRANSPORT INGRESS on each accepted frame (the
//!   stamp travels with the frame through the bounded reader→main queue, so
//!   a resume accepted BEFORE a pause/cancel cannot clear the stop no matter
//!   how long it sits in the raw queue). A `computer_open` /
//!   `computer_resume` stamped with an OLDER generation is refused with
//!   `cancelled` and may not clear the stop flag; a FRESH deliberate
//!   open/resume (current stamp) proceeds — the runtime re-validates state
//!   and clears the flag itself only after safe cleanup/geometry checks.
//! - `Quit` is CONTROL: it rides its own unbounded channel so a flooded
//!   client queue can never delay shutdown. It waits a bounded grace period
//!   for an in-flight native call, then reports `shutdown_unknown` instead
//!   of hanging forever — and never claims a clean stop it cannot guarantee.
//! - Timeouts are truthful: a native call that exceeds the reply deadline is
//!   reported `unknown` (never `not_started`, never success), the worker is
//!   faulted, and NO further work is dispatched on the abandoned native
//!   thread — a late native return cannot execute queued new input.
//! - All queues are bounded across the WHOLE pending population (ingress
//!   channel plus coordinator stash, accounted atomically); a flooding
//!   client gets `server_busy` errors instead of unbounded memory.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::backend::BackendError;
use crate::mcp::backend_factory::BackendFactory;
use crate::runtime::Reply;

mod native;
#[cfg(test)]
mod tests;
mod tracker;

pub use tracker::{ActiveRequest, Admission, CancelHandle, CancelVerdict, TransportGate};

use native::{Completion, CompletionKind, WorkItem};

/// Total session budget; an active session is closed by the watchdog after
/// this wall-clock duration even if the agent goes idle. (Design §5: initial
/// cap 10 minutes, not extendable by the model.)
pub const SESSION_LIFETIME: Duration = Duration::from_secs(600);
/// Watchdog poll granularity.
const WATCHDOG_POLL: Duration = Duration::from_millis(50);
/// Grace period for an in-flight native call to finish once `Quit` has been
/// requested. If a native operation (e.g. an OS text injection) does not
/// return within this bound we report `shutdown_unknown` instead of hanging
/// forever — and we never claim "stopped cleanly" for it.
pub const QUIT_GRACE: Duration = Duration::from_secs(2);
/// Hard bound on queued commands. A runaway client that floods requests gets
/// bounded memory and a `server_busy` tool error instead of an unbounded
/// channel backlog.
pub const MAX_QUEUED_COMMANDS: usize = 64;
/// Bound on the internal native-call completion queue. Only one native call
/// is in flight at a time (work items are processed serially), so this stays
/// tiny; it exists so a finishing native call can never block behind an
/// unbounded backlog either.
const COMPLETION_QUEUE: usize = 4;
/// Upper bound for the protocol-visible confirmation that the runtime
/// shutdown completed. Exceeding it makes the diagnostic `shutdown_unknown`.
pub const MAX_SHUTDOWN_CONFIRM: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerError {
    /// Could not construct the backend inside the worker thread.
    BackendUnavailable { code: String, message: String },
    /// Worker thread died or channel broke.
    Dead,
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerError::BackendUnavailable { code, message } => {
                write!(f, "backend unavailable ({code}): {message}")
            }
            WorkerError::Dead => write!(f, "runtime worker is not running"),
        }
    }
}

enum Command {
    Call {
        name: String,
        args: serde_json::Value,
        reply: Sender<Reply>,
        /// Request generation stamped when the call was accepted. Used to
        /// distinguish pre-stop queued open/resume from fresh deliberate ones.
        gen: u64,
    },
}

/// Control messages ride on their own unbounded channel so a full client
/// queue can never delay shutdown or the session-lifetime watchdog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    /// End the session: cancel, run shutdown, then exit the loop.
    Quit,
    /// Runtime shutdown at session lifetime expiry. Its completion is
    /// handled explicitly by the watchdog, never as an ordinary reply.
    LifetimeClose,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Call { name, .. } => f.debug_tuple("Call").field(name).finish(),
        }
    }
}

/// Outcome of the bounded worker shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownStatus {
    /// Runtime cleanup finished within the confirmation bound.
    Clean,
    /// No session was ever started; nothing to confirm.
    NotNeeded,
    /// A native call did not finish within the grace period or cleanup
    /// failed; the process cannot guarantee cleanup completed. Reported,
    /// never hidden. Once Unknown the worker is FAULTED: the abandoned
    /// native thread may still inject input, so the worker refuses all
    /// further calls, shutdown never claims Clean afterwards, and the
    /// transport must quarantine instead of reconnecting.
    Unknown,
}

/// True when the shutdown could not be confirmed and the worker must be
/// treated as quarantined (an abandoned native thread may still be live).
pub fn shutdown_is_quarantined(status: ShutdownStatus) -> bool {
    status == ShutdownStatus::Unknown
}

impl ShutdownStatus {
    pub fn name(self) -> &'static str {
        match self {
            ShutdownStatus::Clean => "clean",
            ShutdownStatus::NotNeeded => "not_needed",
            ShutdownStatus::Unknown => "unknown",
        }
    }
}

struct WorkerHandle {
    tx: SyncSender<Command>,
    /// Control channel: never full, so Quit/watchdog close are never stuck
    /// behind a flooded client queue.
    ctrl_tx: Sender<Control>,
    session_started: Arc<Mutex<Option<Instant>>>,
    join: Mutex<Option<JoinHandle<()>>>,
    shutdown_status: Arc<Mutex<Option<ShutdownStatus>>>,
    /// Shared with the coordinator: exact number of calls it has taken out of
    /// the ingress channel but not yet dispatched (the stash). `call` counts
    /// ingress + stash so the WHOLE pending population is bounded; draining
    /// the bounded channel into a stash can no longer hide a flood.
    stash_len: Arc<std::sync::atomic::AtomicUsize>,
}

/// Request generation, bumped every time a GLOBAL stop is requested (pause,
/// close, cancel of the in-flight request, EOF, watchdog, shutdown). A
/// `computer_open` / `computer_resume` stamped with the CURRENT generation
/// is a fresh, deliberate request and may proceed; one stamped with an
/// OLDER generation was queued before the stop and must not clear it. A
/// per-request cancel of a QUEUED id bumps nothing (see
/// [`tracker::CancelVerdict`]).
#[derive(Clone)]
pub struct RequestGeneration(Arc<std::sync::atomic::AtomicU64>);

impl RequestGeneration {
    fn new() -> Self {
        RequestGeneration(Arc::new(std::sync::atomic::AtomicU64::new(0)))
    }
    /// Bump on every stop. Returns the new generation.
    pub fn bump(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
    /// Current generation; stamp on a request when it is ACCEPTED.
    pub fn current(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    /// Standalone clock for ingress unit tests.
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::new()
    }
}

pub struct Worker {
    handle: Option<WorkerHandle>,
    cancel: Arc<AtomicBool>,
    active: ActiveRequest,
    generation: RequestGeneration,
    /// Transport-visible cancel gate (registered/tombstoned request ids).
    transport_gate: TransportGate,
    /// Set when shutdown could not be confirmed (an abandoned native thread
    /// may still be live). Once set, the worker refuses every call and never
    /// reports a clean state again — the transport must quarantine.
    faulted: Arc<AtomicBool>,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("alive", &self.handle.is_some())
            .finish()
    }
}

impl Worker {
    /// Spawn coordinator + native threads and construct the backend *inside*
    /// the native thread. `factory` decides native vs explicit test backend.
    pub fn start(factory: BackendFactory) -> Result<Self, WorkerError> {
        let cancel = Arc::new(AtomicBool::new(false));
        let generation = RequestGeneration::new();
        let (tx, rx) = sync_channel::<Command>(MAX_QUEUED_COMMANDS);
        let (ctrl_tx, ctrl_rx) = channel::<Control>();
        let (init_tx, init_rx) = channel::<Result<(), BackendError>>();
        let session_started = Arc::new(Mutex::new(None::<Instant>));
        let shutdown_status = Arc::new(Mutex::new(None::<ShutdownStatus>));
        let stash_len = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cancel_for_thread = Arc::clone(&cancel);
        let generation_for_thread = generation.clone();
        let started_for_thread = Arc::clone(&session_started);
        let status_for_thread = Arc::clone(&shutdown_status);
        let stash_for_thread = Arc::clone(&stash_len);

        let join = std::thread::Builder::new()
            .name("computer-runtime".into())
            .spawn(move || {
                coordinator_main(
                    factory,
                    rx,
                    ctrl_rx,
                    init_tx,
                    CoordShared {
                        cancel: cancel_for_thread,
                        generation: generation_for_thread,
                        session_started: started_for_thread,
                        shutdown_status: status_for_thread,
                        stash_len: stash_for_thread,
                    },
                )
            })
            .map_err(|_| WorkerError::Dead)?;

        match init_rx.recv() {
            Ok(Ok(())) => {
                let transport_gate = TransportGate::new();
                let active = transport_gate.active_view();
                Ok(Worker {
                    handle: Some(WorkerHandle {
                        tx,
                        ctrl_tx,
                        session_started,
                        join: Mutex::new(Some(join)),
                        shutdown_status,
                        stash_len,
                    }),
                    cancel,
                    active,
                    generation,
                    transport_gate,
                    faulted: Arc::new(AtomicBool::new(false)),
                })
            }
            Ok(Err(e)) => {
                let _ = join.join();
                Err(WorkerError::BackendUnavailable {
                    code: e.code,
                    message: e.message,
                })
            }
            Err(_) => {
                let _ = join.join();
                Err(WorkerError::Dead)
            }
        }
    }

    pub fn cancel_handle(&self) -> CancelHandle {
        CancelHandle {
            cancel: Arc::clone(&self.cancel),
            generation: self.generation.clone(),
        }
    }

    /// Shared view of the in-flight request id for validated cancellation.
    pub fn active_request(&self) -> ActiveRequest {
        self.active.clone()
    }

    /// Shared transport cancellation gate. The stdio/TCP readers register
    /// every accepted request id here (stamping the frame generation at the
    /// same time); validated cancels tombstone an id so a still-queued
    /// request is refused at dispatch instead of running to a clean success
    /// after its own cancellation.
    pub fn transport_gate(&self) -> TransportGate {
        self.transport_gate.clone()
    }

    /// Shared stop-generation clock: transport readers stamp each accepted
    /// frame with the CURRENT value at ingress, so staleness is judged where
    /// the frame was accepted, not where it was dispatched.
    pub fn request_generation(&self) -> RequestGeneration {
        self.generation.clone()
    }

    /// Hard bound for the blocking [`Worker::call`] used by the stdio/TCP
    /// transports. Transport loops poll their shutdown flag every ~50ms, so
    /// after this deadline the caller gets a truthful `unknown` reply, marks
    /// the worker faulted, and the next poll tears the transport down. A
    /// dispatched request that never returns is NEVER reported as
    /// not_started or success.
    pub const TRANSPORT_CALL_TIMEOUT: Duration = Duration::from_secs(20);

    /// Try to enqueue one tool call WITHOUT waiting for its reply. Used by
    /// callers that need the admission decision (server_busy vs queued)
    /// synchronously while the reply is collected elsewhere. Identical
    /// admission semantics to [`Worker::call_with_deadline`].
    ///
    /// Production callers: [`Worker::call_with_deadline`] routes its
    /// admission through this method (single source of truth), and tests
    /// use the admission-without-wait shape directly.
    pub(crate) fn enqueue(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<Receiver<Reply>, Reply> {
        self.enqueue_impl(name, args, None)
    }

    /// Dispatch one tool call stamped at TRANSPORT ingress with generation
    /// `gen` and wait bounded by [`Worker::TRANSPORT_CALL_TIMEOUT`]. Used by
    /// the stdio/TCP dispatch loops for frames that travelled through the
    /// reader→main queue: the ingress stamp — not the dispatch-time
    /// generation — decides staleness, so an open/resume accepted BEFORE a
    /// pause/cancel is refused no matter how long it sat in the raw queue.
    pub fn call_with_ingress_gen(
        &self,
        name: &str,
        args: serde_json::Value,
        gen: u64,
    ) -> Result<Reply, WorkerError> {
        self.wait_for_reply(
            name,
            self.enqueue_impl(name, args, Some(gen)),
            Self::TRANSPORT_CALL_TIMEOUT,
        )
    }

    fn enqueue_impl(
        &self,
        name: &str,
        args: serde_json::Value,
        ingress_gen: Option<u64>,
    ) -> Result<Receiver<Reply>, Reply> {
        let Some(handle) = self.handle.as_ref() else {
            return Err(Reply::err(crate::runtime::error::ToolError::new(
                "worker_unavailable",
                "runtime worker is not running",
            )));
        };
        if self.faulted.load(Ordering::SeqCst) {
            return Err(Reply::err(crate::runtime::error::ToolError::new(
                "worker_faulted",
                "a previous native call never returned and its input state is unknown; \
                 this worker is quarantined, restart the host",
            )));
        }
        if matches!(name, "computer_pause" | "computer_close") {
            // Epoch first, then the flag (see CancelHandle::cancel): the
            // native epoch guard re-latches conservatively on a new epoch,
            // so this order never lets a guard pass between bump and latch.
            self.generation.bump();
            self.cancel.store(true, Ordering::SeqCst);
        }
        // Staleness is judged by the INGRESS stamp when the frame travelled
        // through a transport queue (accepted before/after a stop is decided
        // where the frame was accepted, not where it was dispatched);
        // direct callers are stamped now.
        let gen = ingress_gen.unwrap_or_else(|| self.generation.current());
        let (reply_tx, reply_rx) = channel();
        let stash = handle.stash_len.load(Ordering::SeqCst);
        if stash >= MAX_QUEUED_COMMANDS {
            return Err(Reply::err(crate::runtime::error::ToolError::new(
                "server_busy",
                format!(
                    "pending call backlog is full ({MAX_QUEUED_COMMANDS}); retry after the current request completes"
                ),
            )));
        }
        match handle.tx.try_send(Command::Call {
            name: name.to_string(),
            args,
            reply: reply_tx,
            gen,
        }) {
            Ok(()) => Ok(reply_rx),
            Err(std::sync::mpsc::TrySendError::Full(_)) => Err(Reply::err(
                crate::runtime::error::ToolError::new(
                    "server_busy",
                    format!(
                        "command queue is full ({MAX_QUEUED_COMMANDS}); retry after the current request completes"
                    ),
                ),
            )),
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => Err(Reply::err(
                crate::runtime::error::ToolError::new(
                    "worker_unavailable",
                    "runtime worker is not running",
                ),
            )),
        }
    }

    /// Dispatch one tool call and wait for its reply, bounded by
    /// [`Worker::TRANSPORT_CALL_TIMEOUT`]. This is the public entry point
    /// used by the transports; it can never block forever.
    pub fn call(&self, name: &str, args: serde_json::Value) -> Result<Reply, WorkerError> {
        self.call_with_deadline(name, args, Self::TRANSPORT_CALL_TIMEOUT)
    }

    /// Dispatch one tool call and wait for its reply with an explicit
    /// deadline.
    ///
    /// Control calls (`computer_pause`, `computer_close`) set the shared
    /// cancel flag *before* reaching the runtime so an in-flight long action
    /// observes cancellation. Queuing is bounded: the ENTIRE pending
    /// population (ingress channel plus the coordinator's stash) is capped,
    /// so a flooding client gets `server_busy` instead of unbounded memory.
    ///
    /// Timeout semantics: if the native call does not reply within
    /// `timeout`, the input may still have reached the desktop, so the reply
    /// is a truthful `unknown` tool error — never success, never
    /// not_started — and the worker is marked faulted. Once faulted, every
    /// subsequent call is refused immediately.
    pub fn call_with_deadline(
        &self,
        name: &str,
        args: serde_json::Value,
        timeout: Duration,
    ) -> Result<Reply, WorkerError> {
        // Admission is identical to `enqueue` (single source of truth for
        // the faulted/pause-close-cancel/stale/busy decisions); the
        // worker_unavailable admission error maps to Dead for this
        // wait-based entry point.
        self.wait_for_reply(name, self.enqueue(name, args), timeout)
    }

    fn wait_for_reply(
        &self,
        name: &str,
        admission: Result<Receiver<Reply>, Reply>,
        timeout: Duration,
    ) -> Result<Reply, WorkerError> {
        let reply_rx = match admission {
            Ok(rx) => rx,
            Err(reply) => {
                let unavailable = reply
                    .data
                    .get("error")
                    .and_then(|e| e.get("code"))
                    .and_then(|c| c.as_str())
                    == Some("worker_unavailable");
                return if unavailable {
                    Err(WorkerError::Dead)
                } else {
                    Ok(reply)
                };
            }
        };
        match reply_rx.recv_timeout(timeout) {
            Ok(reply) => Ok(reply),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // The native call may STILL be executing input; we can never
                // claim not_started/success for it. Quarantine the worker:
                // the coordinator abandons this call after its grace period,
                // but a late native reply must not execute further queued
                // input, and no new work may be accepted behind it.
                self.faulted.store(true, Ordering::SeqCst);
                eprintln!(
                    "[computer-host] native call '{name}' exceeded {}s reply deadline; \
                     outcome unknown, worker quarantined",
                    timeout.as_secs()
                );
                Ok(Reply::err(crate::runtime::error::ToolError::new(
                    "unknown",
                    format!(
                        "native call did not return within {}s; input may have been dispatched, outcome unknown",
                        timeout.as_secs()
                    ),
                )))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(WorkerError::Dead),
        }
    }

    /// True once a shutdown or call could not be confirmed and an abandoned
    /// native thread may still be live. The transport must treat this as
    /// fatal: no new sessions, no lock release-and-continue.
    pub fn is_faulted(&self) -> bool {
        self.faulted.load(Ordering::SeqCst)
    }

    /// True once the worker reports a live session (diagnostics only; never
    /// a substitute for runtime state).
    pub fn session_active(&self) -> bool {
        self.handle
            .as_ref()
            .map(|h| {
                h.session_started
                    .lock()
                    .map(|g| g.is_some())
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Current number of calls the coordinator has pulled out of the
    /// bounded ingress channel but not yet dispatched (the ACCOUNTED
    /// stash). Diagnostics/tests use it to observe genuine queueing;
    /// admission control uses the same counter inside `call_with_deadline`.
    pub fn stash_len(&self) -> usize {
        self.handle
            .as_ref()
            .map(|h| h.stash_len.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Stop the session and workers: set cancel, send Quit over the control
    /// channel (never blocked by a full client queue), join with a bounded
    /// wait. Safe to call multiple times. Returns the shutdown status;
    /// `Unknown` means a native call never returned or cleanup failed — the
    /// worker is then FAULTED and must be quarantined, never reported clean.
    pub fn shutdown(&self) -> ShutdownStatus {
        // Epoch first, then the flag (see CancelHandle::cancel).
        self.generation.bump();
        self.cancel.store(true, Ordering::SeqCst);
        self.active.set(None);
        let Some(handle) = self.handle.as_ref() else {
            return ShutdownStatus::NotNeeded;
        };
        let join = handle.join.lock().ok().and_then(|mut g| g.take());
        let Some(join) = join else {
            // Already shut down once; report the recorded outcome.
            return handle
                .shutdown_status
                .lock()
                .ok()
                .and_then(|g| *g)
                .unwrap_or(ShutdownStatus::NotNeeded);
        };
        // Quit is control, not data: the control channel is unbounded, so a
        // flooded client queue can never delay shutdown.
        let _ = handle.ctrl_tx.send(Control::Quit);
        let (done_tx, done_rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("computer-worker-join".into())
            .spawn(move || {
                let _ = join.join();
                let _ = done_tx.send(());
            });
        if spawned.is_err() {
            // Could not even spawn the join helper: a bounded join is
            // impossible; report unknown honestly.
            eprintln!("[computer-host] cannot spawn join helper; shutdown_unknown");
            self.mark_unknown(handle);
            return ShutdownStatus::Unknown;
        }
        match done_rx.recv_timeout(MAX_SHUTDOWN_CONFIRM) {
            Ok(()) => {
                let status = handle
                    .shutdown_status
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .unwrap_or(ShutdownStatus::Clean);
                // A faulted worker (abandoned native call, failed cleanup)
                // is NEVER reported clean — even if the coordinator managed
                // to exit. Unknown sticks.
                if shutdown_is_quarantined(status) || self.is_faulted() {
                    self.mark_unknown(handle);
                    return ShutdownStatus::Unknown;
                }
                status
            }
            Err(_) => {
                eprintln!(
                    "[computer-host] shutdown confirmation exceeded {}s; reporting shutdown_unknown \
                     (a native call never returned; cleanup state cannot be guaranteed)",
                    MAX_SHUTDOWN_CONFIRM.as_secs()
                );
                self.mark_unknown(handle);
                ShutdownStatus::Unknown
            }
        }
    }

    /// Record a faulted shutdown outcome that cannot be reverted by later
    /// shutdown calls.
    fn mark_unknown(&self, handle: &WorkerHandle) {
        self.faulted.store(true, Ordering::SeqCst);
        if let Ok(mut g) = handle.shutdown_status.lock() {
            *g = Some(ShutdownStatus::Unknown);
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Shared state owned by (or mirrored into) the coordinator thread. Grouped
/// so the thread entry point stays small and each field's role is named.
struct CoordShared {
    /// Cancel flag shared with the native runtime and the I/O side.
    cancel: Arc<AtomicBool>,
    /// Stop epochs: stale queued open/resume are refused, fresh ones pass.
    generation: RequestGeneration,
    /// Absolute session-lifetime anchor (set on verified successful open).
    session_started: Arc<Mutex<Option<Instant>>>,
    /// Recorded final shutdown outcome (read by `Worker::shutdown`).
    shutdown_status: Arc<Mutex<Option<ShutdownStatus>>>,
    /// Exact number of calls taken out of the ingress channel but not yet
    /// dispatched; `Worker::call` counts ingress + stash so the WHOLE pending
    /// population is bounded.
    stash_len: Arc<std::sync::atomic::AtomicUsize>,
}

/// Coordinator thread: queueing, watchdog, cancel policy. Runs no native
/// code so it stays responsive while a native call is in flight.
fn coordinator_main(
    factory: BackendFactory,
    rx: Receiver<Command>,
    ctrl_rx: Receiver<Control>,
    init_tx: Sender<Result<(), BackendError>>,
    shared: CoordShared,
) {
    let CoordShared {
        cancel,
        generation,
        session_started,
        shutdown_status,
        stash_len,
    } = shared;
    // Work items for the native thread, one at a time (serial execution).
    let (work_tx, work_rx) = sync_channel::<WorkItem>(1);
    // Completions coming back. Bounded so the native thread never blocks on
    // an unbounded backlog either.
    let (done_tx, done_rx) = sync_channel::<Completion>(COMPLETION_QUEUE);
    let (native_init_tx, native_init_rx) = channel::<Result<(), BackendError>>();

    let native_cancel = Arc::clone(&cancel);
    let native_started = Arc::clone(&session_started);
    let native_generation = generation.clone();
    let native_join = std::thread::Builder::new()
        .name("computer-native".into())
        .spawn(move || {
            native::native_main(
                factory,
                work_rx,
                done_tx,
                native_init_tx,
                native_cancel,
                native_started,
                native_generation,
            )
        });

    let mut native_join = match native_join {
        Ok(j) => Some(j),
        Err(_) => {
            let _ = init_tx.send(Err(BackendError {
                code: "worker_spawn_failed".into(),
                message: "cannot spawn native worker thread".into(),
            }));
            return;
        }
    };

    // Forward native init to the starter.
    match native_init_rx.recv() {
        Ok(Ok(())) => {
            if init_tx.send(Ok(())).is_err() {
                return;
            }
        }
        Ok(Err(e)) => {
            let _ = init_tx.send(Err(e));
            if let Some(j) = native_join.take() {
                let _ = j.join();
            }
            return;
        }
        Err(_) => {
            let _ = init_tx.send(Err(BackendError {
                code: "worker_died".into(),
                message: "native worker died during init".into(),
            }));
            if let Some(j) = native_join.take() {
                let _ = j.join();
            }
            return;
        }
    }

    let mut native_busy = false;
    let mut native_done = false;
    // The native thread was abandoned after a grace deadline; it may STILL
    // be executing input. From this point on no further work items are ever
    // dispatched — a late native return must not execute queued new input.
    let mut native_abandoned = false;
    // Loopback control channel so the watchdog can route its close through
    // the SAME prioritized control path as an external Quit.
    let (self_ctrl_tx, self_ctrl_rx) = channel::<Control>();
    let mut quitting = false;
    let mut quit_deadline: Option<Instant> = None;
    let mut shutdown_sent = false;
    // `final_status` is always explicitly set on every exit path; the
    // initializer just satisfies the compiler.
    #[allow(unused_assignments)]
    let mut final_status = ShutdownStatus::NotNeeded;
    // Calls pulled from the client queue while the native thread was busy;
    // handled in order once it becomes idle. The stash is ACCOUNTED through
    // `stash_len` so `Worker::call` caps the total pending population —
    // draining the bounded ingress channel into this stash cannot hide a
    // flood from the admission check.
    let mut stash: VecDeque<Command> = VecDeque::new();
    /// Deliver a tool error to a queued call's caller (best effort).
    fn reject_call(cmd: Command, code: &'static str, message: impl Into<String>) {
        let Command::Call { reply, .. } = cmd;
        let _ = reply.send(Reply::err(crate::runtime::error::ToolError::new(
            code, message,
        )));
    }

    loop {
        // 1. Reap native completions (bounded wait doubles as watchdog poll).
        match done_rx.recv_timeout(WATCHDOG_POLL) {
            Ok(completion) => {
                match completion.kind {
                    CompletionKind::Ordinary => {
                        native_busy = false;
                    }
                    CompletionKind::LifetimeClose { is_error } => {
                        // Control completion for the watchdog-forced close:
                        // handled here, never delivered as an ordinary reply.
                        native_busy = false;
                        if is_error {
                            eprintln!(
                                "[computer-host] lifetime close reported cleanup problems; \
                                 session state is unknown, faulting"
                            );
                            // The watchdog close failed: cleanup state cannot
                            // be guaranteed. Fault like an abandoned call.
                            native_done = true;
                            final_status = ShutdownStatus::Unknown;
                            break;
                        }
                    }
                    CompletionKind::Shutdown { is_error, data } => {
                        native_done = true;
                        final_status = if is_error {
                            eprintln!(
                                "[computer-host] shutdown reported cleanup problems: {}",
                                serde_json::to_string(&data).unwrap_or_default()
                            );
                            ShutdownStatus::Unknown
                        } else {
                            ShutdownStatus::Clean
                        };
                        break;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                native_done = true;
                final_status = ShutdownStatus::Unknown;
                break;
            }
        }

        // 2. Session lifetime watchdog: absolute deadline anchored at the
        //    successful open. Requests never extend it; an invalid close
        //    cannot clear it (only the native thread clears it after a
        //    VERIFIED successful close).
        let started_at = session_started.lock().ok().and_then(|g| *g);
        if let Some(started) = started_at {
            if started.elapsed() >= SESSION_LIFETIME {
                eprintln!(
                    "[computer-host] session exceeded {}s lifetime budget; cancelling and closing",
                    SESSION_LIFETIME.as_secs()
                );
                // Epoch first, then the flag (see CancelHandle::cancel).
                generation.bump();
                cancel.store(true, Ordering::SeqCst);
                if let Ok(mut g) = session_started.lock() {
                    *g = None;
                }
                // Control, not data: ride the control channel so a flooded
                // client queue cannot delay the close.
                let _ = self_ctrl_tx.send(Control::LifetimeClose);
            }
        }

        // 3. Control first: Quit / watchdog close must never wait behind
        //    queued client calls.
        loop {
            match ctrl_rx.try_recv().or_else(|_| self_ctrl_rx.try_recv()) {
                Ok(Control::Quit) => {
                    quitting = true;
                    quit_deadline.get_or_insert_with(|| Instant::now() + QUIT_GRACE);
                }
                Ok(Control::LifetimeClose) => {
                    if !native_busy
                        && !native_done
                        && !native_abandoned
                        && work_tx
                            .send(WorkItem {
                                name: "__lifetime_close".into(),
                                args: json!({}),
                                reply: None,
                                gen: native::CONTROL_GEN,
                            })
                            .is_ok()
                    {
                        native_busy = true;
                    }
                    // Busy/done/abandoned: the close is pointless (the
                    // native thread cannot accept new work); the grace /
                    // watchdog paths below handle the fallout.
                }
                Err(_) => break,
            }
        }

        // 4. Feed the native thread when idle. After abandonment, queued
        //    work is rejected outright — a late native return must never
        //    pick up new input.
        if !native_busy && !native_done {
            if native_abandoned {
                while let Some(cmd) = stash.pop_front() {
                    stash_len.fetch_sub(1, Ordering::SeqCst);
                    reject_call(
                        cmd,
                        "unknown",
                        "the previous native call never returned; this request was not started",
                    );
                }
                while let Ok(cmd) = rx.try_recv() {
                    reject_call(
                        cmd,
                        "unknown",
                        "the previous native call never returned; this request was not started",
                    );
                }
            } else if let Some(cmd) = stash.pop_front() {
                stash_len.fetch_sub(1, Ordering::SeqCst);
                if !dispatch_command(cmd, &work_tx, &generation, &mut native_busy) {
                    native_done = true;
                    final_status = ShutdownStatus::Unknown;
                    break;
                }
            } else {
                match rx.try_recv() {
                    Ok(cmd) => {
                        if !dispatch_command(cmd, &work_tx, &generation, &mut native_busy) {
                            native_done = true;
                            final_status = ShutdownStatus::Unknown;
                            break;
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        quitting = true; // EOF: all senders dropped
                        quit_deadline.get_or_insert_with(|| Instant::now() + QUIT_GRACE);
                    }
                }
            }
        } else if !native_done {
            // Native busy: move commands into the ACCOUNTED stash so the
            // total pending population stays bounded; watch for Quit.
            if let Ok(cmd) = rx.try_recv() {
                stash.push_back(cmd);
                stash_len.fetch_add(1, Ordering::SeqCst);
            }
        }

        // 5. Shutdown sequencing.
        if quitting && !native_done && !shutdown_sent {
            if native_busy {
                if let Some(deadline) = quit_deadline {
                    if Instant::now() >= deadline {
                        eprintln!(
                            "[computer-host] in-flight native call exceeded {}s grace; \
                             abandoning wait and reporting shutdown_unknown",
                            QUIT_GRACE.as_secs()
                        );
                        final_status = ShutdownStatus::Unknown;
                        // Recorded for the drain diagnostics below; we exit
                        // immediately, so no further dispatch can happen.
                        let _ = &mut native_abandoned;
                        break;
                    }
                }
            } else if native_abandoned {
                // Nothing can be run on the abandoned native thread; the
                // runtime shutdown state is unknown by definition.
                final_status = ShutdownStatus::Unknown;
                break;
            } else {
                // Idle: run final runtime shutdown on the native thread.
                if work_tx
                    .send(WorkItem {
                        name: "__shutdown".into(),
                        args: json!({}),
                        reply: None,
                        gen: native::CONTROL_GEN,
                    })
                    .is_err()
                {
                    final_status = ShutdownStatus::Unknown;
                    native_done = true;
                    break;
                }
                native_busy = true;
                shutdown_sent = true;
            }
        }
    }

    if let Ok(mut g) = shutdown_status.lock() {
        *g = Some(final_status);
    }
    // Join the native thread only if it is no longer running native code;
    // otherwise leave it (process teardown reclaims it) rather than hang.
    if native_done {
        if let Some(j) = native_join.take() {
            let (join_tx, join_rx) = channel();
            std::thread::spawn(move || {
                let _ = j.join();
                let _ = join_tx.send(());
            });
            let _ = join_rx.recv_timeout(QUIT_GRACE);
        }
    }
    // Drain any remaining client calls with an explicit error. After an
    // unknown/faulted outcome the message must not claim "not started" for
    // calls that may have been dispatched; stash-held calls provably never
    // ran, so "worker_unavailable" is truthful for them.
    let drain_code = if shutdown_is_quarantined(final_status) {
        "worker_unavailable"
    } else {
        "session_state"
    };
    for cmd in stash.into_iter() {
        reject_call(cmd, drain_code, "runtime worker is shutting down");
    }
    while let Ok(cmd) = rx.try_recv() {
        reject_call(cmd, drain_code, "runtime worker is shutting down");
    }
}

/// Gate + forward one client command to the native thread.
/// Returns false if the native thread is gone.
fn dispatch_command(
    cmd: Command,
    work_tx: &SyncSender<WorkItem>,
    generation: &RequestGeneration,
    native_busy: &mut bool,
) -> bool {
    let Command::Call {
        name,
        args,
        reply,
        gen,
    } = cmd;
    // Generation gate: only an open/resume queued BEFORE a stop is refused.
    // A fresh deliberate open/resume (stamped at the current generation)
    // proceeds; the runtime re-validates state and clears the flag itself
    // only after safe cleanup/geometry checks.
    if matches!(name.as_str(), "computer_open" | "computer_resume") && gen != generation.current() {
        let _ = reply.send(Reply::err(crate::runtime::error::ToolError::new(
            "cancelled",
            "this request was queued before a stop was requested; issue a fresh request after cleanup",
        )));
        return true;
    }
    match work_tx.send(WorkItem {
        name,
        args,
        reply: Some(reply),
        gen,
    }) {
        Ok(()) => {
            *native_busy = true;
            true
        }
        Err(_) => false,
    }
}
