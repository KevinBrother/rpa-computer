//! Portable production process boundaries: bounded drain and startup cancellation.
//! No threads, signals, native connections or blocking waits are started here.
use super::{
    startup_input::StartupInput, Exit, Native, PipeBridge, ReadyEnqueue, Runtime, Style, Transport,
    UiFailure,
};
use rpa_desktop_feedback::{
    protocol::{Ready, RendererError, RendererMessage, V1},
    Diagnostic,
};
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub struct PipeState {
    pub(super) bridge: PipeBridge,
    oldest: Option<Instant>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerDecision {
    pub end: bool,
    pub fault: bool,
}
impl PipeState {
    pub fn new() -> Self {
        Self {
            bridge: PipeBridge::new(),
            oldest: None,
        }
    }
    pub fn enqueue(
        &mut self,
        message: RendererMessage,
        now: impl FnOnce() -> Instant,
    ) -> Result<(), Diagnostic> {
        self.bridge.enqueue(message)?;
        self.oldest.get_or_insert(now());
        Ok(())
    }
    /// Called under the same mailbox lock as worker reads: EOF and Ready enqueue
    /// have an explicit ordering, without reclassifying an unrelated enqueue fault.
    pub fn enqueue_ready(
        &mut self,
        ready: Ready,
        now: impl FnOnce() -> Instant,
    ) -> Result<ReadyEnqueue, Diagnostic> {
        let result = self.bridge.enqueue_ready(ready)?;
        if result == ReadyEnqueue::Enqueued {
            self.oldest.get_or_insert(now());
        }
        Ok(result)
    }
    pub fn buffered_frames(&self) -> usize {
        self.bridge.buffered_frames()
    }
    /// Same worker turn as before extraction: one bounded read/write, unchanged
    /// 1s queue-age and 200ms closing budgets. Production supplies Instant::now;
    /// tests supply a fixed monotonic instant without sleeps or OS I/O.
    pub fn turn<R: Read, W: Write>(
        &mut self,
        read: &mut R,
        write: &mut W,
        closing: Option<Instant>,
        now: impl Fn() -> Instant,
    ) -> WorkerDecision {
        if closing.is_none() {
            let _ = self.bridge.read_once(read);
        }
        let _ = self.bridge.write_once(write);
        if self.bridge.buffered_frames() == 0 {
            self.oldest = None;
        }
        if self
            .oldest
            .is_some_and(|at| now().saturating_duration_since(at) >= Duration::from_secs(1))
        {
            self.bridge.poison(Diagnostic::TransportLost);
        }
        // An expired drain with queued (including partially written) output is
        // a terminal transport fault, never a successful end or implicit discard.
        let drain_expired = closing
            .is_some_and(|at| now().saturating_duration_since(at) >= Duration::from_millis(200));
        if drain_expired && self.bridge.buffered_frames() != 0 {
            self.bridge.poison(Diagnostic::TransportLost);
        }
        let failed = self.bridge.failure().is_some();
        let end = failed || (closing.is_some() && self.bridge.buffered_frames() == 0);
        WorkerDecision { end, fault: failed }
    }
}

/// The exact RAII done/fault signaling used by the OS worker, including unwind.
pub struct WorkerExit {
    done: Arc<AtomicBool>,
    fault: Arc<AtomicBool>,
    clean: bool,
}
impl WorkerExit {
    pub fn new(done: Arc<AtomicBool>, fault: Arc<AtomicBool>) -> Self {
        Self {
            done,
            fault,
            clean: false,
        }
    }
    pub fn mark_clean(&mut self) {
        self.clean = true;
    }
}
impl Drop for WorkerExit {
    fn drop(&mut self) {
        if !self.clean {
            self.fault.store(true, Ordering::SeqCst);
        }
        self.done.store(true, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionSnapshot {
    pub done: bool,
    pub fault: bool,
    /// None means the queue cannot be observed without waiting (or was poisoned).
    /// This is not an empty-queue receipt, nor an acknowledgment from the parent.
    pub pending_frames: Option<usize>,
}
/// Kept by the main thread after Runtime/Pipes drop; never joins or blocks on a lock.
pub struct Completion {
    state: Arc<Mutex<PipeState>>,
    done: Arc<AtomicBool>,
    fault: Arc<AtomicBool>,
}
impl Completion {
    pub fn new(
        state: Arc<Mutex<PipeState>>,
        done: Arc<AtomicBool>,
        fault: Arc<AtomicBool>,
    ) -> Self {
        Self { state, done, fault }
    }
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
    pub fn snapshot(&self) -> CompletionSnapshot {
        let done = self.is_done();
        let fault = self.fault.load(Ordering::SeqCst);
        let pending_frames = self.state.try_lock().ok().map(|s| s.buffered_frames());
        CompletionSnapshot {
            done,
            fault,
            pending_frames,
        }
    }
}
/// Clean input EOF is only clean process completion when the worker acknowledged
/// shutdown, no output/worker fault occurred, and the queue is confirmed empty.
/// This is local drain accounting, not proof the parent acted on a Stop.
pub fn process_exit_code(exit: Exit, completion: CompletionSnapshot) -> i32 {
    if exit == Exit::ParentEof
        && completion.done
        && !completion.fault
        && completion.pending_frames == Some(0)
    {
        0
    } else {
        65
    }
}

/// Main's original 300ms wait, with clock/pause supplied by the OS adapter.
/// Tests exercise deadline expiry without sleeping or launching the worker.
pub fn finish_process(
    exit: Exit,
    completed: &Completion,
    now: impl Fn() -> Instant,
    mut pause: impl FnMut(),
) -> i32 {
    let deadline = now() + Duration::from_millis(300);
    while !completed.is_done() && now() < deadline {
        pause();
    }
    process_exit_code(exit, completed.snapshot())
}

pub enum Startup<N: Native, T: Transport> {
    Running(Runtime<N, T>),
    /// Startup already finished transport/partial native ownership. Main must
    /// still verify the bounded worker completion receipt before returning clean.
    Closed(Exit),
}
/// Production connect/start ordering. The callback retains the binary's bounded
/// connection-error drain wait; it is not called on initialization failure.
pub fn start_renderer<N: Native, T: Transport>(
    mut transport: T,
    style: Style,
    connect: impl FnOnce() -> Result<N, UiFailure>,
    now: impl FnOnce() -> Duration,
    connection_failure_wait: impl FnOnce(),
) -> Result<Startup<N, T>, UiFailure> {
    let mut pending = StartupInput::default();
    match pending.continuing(&mut transport) {
        Ok(true) => {}
        Ok(false) => {
            transport.finish();
            return Ok(Startup::Closed(Exit::ParentEof));
        }
        Err(error) => {
            transport.finish();
            return Err(error);
        }
    }
    let native = match connect() {
        Ok(native) => native,
        Err(error) => {
            // Even a failed native call is a startup boundary. If the parent
            // cancelled while it ran, no later native/error/Ready work is needed.
            // Actual input/output faults must not be mistaken for cancellation.
            match pending.continuing(&mut transport) {
                Ok(false) => {
                    transport.finish();
                    return Ok(Startup::Closed(Exit::ParentEof));
                }
                Err(fault) => {
                    transport.finish();
                    return Err(fault);
                }
                Ok(true) => {}
            }
            let _ = transport.enqueue(RendererMessage::Error(RendererError {
                version: V1,
                code: "renderer_native_failed".into(),
            }));
            transport.finish();
            connection_failure_wait();
            return Err(error);
        }
    };
    let mut runtime = Runtime::new(native, transport, style);
    runtime.seed_startup_input(pending);
    runtime.start(now())?;
    match runtime.exit_reason() {
        Some(exit) => Ok(Startup::Closed(exit)),
        None => Ok(Startup::Running(runtime)),
    }
}
