//! One bounded polling I/O supervisor, independent of the native/input thread.
use super::{
    process::{Pipes, Spawner},
    FeedbackConfig, FeedbackError, FeedbackHandle,
};
use rpa_desktop_feedback::{
    process::{FeedbackMode, Supervisor, SupervisorConfig},
    protocol::{CaptureExclusion, Ready},
    transport::{SnapshotWriter, WriteProgress},
    Diagnostic,
};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(10);
const TEARDOWN: Duration = Duration::from_secs(2);
const STARTUP_BOUND: Duration = Duration::from_secs(8);
const MAX_READS_PER_TICK: usize = 4;
const WRITE_BOUND: Duration = Duration::from_secs(5);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackShutdown {
    Reaped,
    ProcessUnconfirmed,
    ThreadUnconfirmed,
}
pub struct FeedbackHost {
    stop: Arc<AtomicBool>,
    done: Receiver<FeedbackShutdown>,
    join: Option<JoinHandle<()>>,
    status: Option<FeedbackShutdown>,
}
impl FeedbackHost {
    pub fn start(
        config: &FeedbackConfig,
        facts: FeedbackHandle,
    ) -> Result<Option<Self>, FeedbackError> {
        if !config.enabled() {
            return Ok(None);
        }
        config.validate()?;
        // screenshots on macOS does not currently filter this renderer. This
        // hard refusal is removed only when exact-PID capture is integrated.
        if cfg!(target_os = "macos") {
            return Err(FeedbackError::CaptureUnsupported);
        }
        Self::start_polling(config, facts, false).map(Some)
    }
    pub(crate) fn start_polling(
        config: &FeedbackConfig,
        facts: FeedbackHandle,
        require_pid_filter: bool,
    ) -> Result<Self, FeedbackError> {
        let command = config.command()?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready_tx, ready_rx) = sync_channel(1);
        let (done_tx, done) = sync_channel(1);
        let join = thread::Builder::new()
            .name("computer-feedback-io".into())
            .spawn(move || {
                let status = run(command, facts, thread_stop, ready_tx, require_pid_filter);
                let _ = done_tx.try_send(status);
            })
            .map_err(|_| FeedbackError::WorkerUnavailable)?;
        let mut host = Self {
            stop,
            done,
            join: Some(join),
            status: None,
        };
        match ready_rx.recv_timeout(STARTUP_BOUND) {
            Ok(Ok(())) => Ok(host),
            Ok(Err(e)) => {
                let _ = host.shutdown();
                Err(e)
            }
            Err(_) => {
                let _ = host.shutdown();
                Err(FeedbackError::WorkerUnavailable)
            }
        }
    }
    /// Bounded join on a nonblocking I/O worker. No helper thread is needed.
    pub fn shutdown(&mut self) -> FeedbackShutdown {
        if let Some(status) = self.status {
            return status;
        }
        self.stop.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + TEARDOWN + Duration::from_secs(1);
        while self.join.as_ref().is_some_and(|j| !j.is_finished()) && Instant::now() < deadline {
            thread::sleep(POLL);
        }
        if self.join.as_ref().is_some_and(|j| !j.is_finished()) {
            return FeedbackShutdown::ThreadUnconfirmed;
        }
        if let Some(join) = self.join.take() {
            if join.join().is_err() {
                return FeedbackShutdown::ThreadUnconfirmed;
            }
        }
        let status = self
            .done
            .try_recv()
            .unwrap_or(FeedbackShutdown::ProcessUnconfirmed);
        self.status = Some(status);
        status
    }
}
impl Drop for FeedbackHost {
    fn drop(&mut self) {
        if self.join.is_some() {
            let result = self.shutdown();
            if result != FeedbackShutdown::Reaped {
                eprintln!(
                    "[computer-host] desktop_feedback teardown {result:?}; cleanup not proven"
                );
            }
        }
    }
}
pub(crate) fn validate_ready(ready: &Ready, require_pid_filter: bool) -> Result<(), FeedbackError> {
    if ready.capture_exclusion == CaptureExclusion::Unsupported || require_pid_filter {
        Err(FeedbackError::CaptureUnsupported)
    } else {
        Ok(())
    }
}
fn run(
    command: rpa_desktop_feedback::process::RendererCommand,
    facts: FeedbackHandle,
    stop: Arc<AtomicBool>,
    ready_tx: SyncSender<Result<(), FeedbackError>>,
    require_pid_filter: bool,
) -> FeedbackShutdown {
    let epoch = Instant::now();
    let mut spawner = Spawner::default();
    let mut supervisor = match Supervisor::start(
        FeedbackMode::Enabled,
        Duration::ZERO,
        SupervisorConfig::default(),
        || Ok(command),
        &mut spawner,
    ) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.try_send(Err(FeedbackError::Protocol(e.diagnostic)));
            return FeedbackShutdown::Reaped;
        }
    };
    let mut pipes = spawner.pipes.take().expect("spawn supplies private pipes");
    let subscriber = match facts.subscribe() {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.try_send(Err(e));
            return teardown(&mut supervisor);
        }
    };
    let mut writer = SnapshotWriter::new(subscriber);
    let mut ready_sent = false;
    let mut pending_since: Option<Instant> = None;
    let mut buffer = [0u8; 4096];
    while !stop.load(Ordering::SeqCst) {
        if let Err(error) = read_tick(
            &mut supervisor,
            &mut pipes,
            epoch.elapsed(),
            &mut buffer,
            require_pid_filter,
        ) {
            facts.terminate();
            if !ready_sent {
                let _ = ready_tx.try_send(Err(error));
            } else {
                eprintln!("[computer-host] {error}; control revoked, native cleanup required");
            }
            break;
        }
        supervisor.poll(epoch.elapsed());
        if let Some(failure) = supervisor.machine().failure() {
            facts.terminate();
            if !ready_sent {
                let _ = ready_tx.try_send(Err(FeedbackError::Protocol(failure.diagnostic)));
            } else {
                eprintln!(
                    "[computer-host] {}; control revoked, native cleanup required",
                    failure.diagnostic
                );
            }
            // Keep fault/cleanup snapshots flowing briefly while native worker
            // handles Quit. No heartbeat failure ever re-authorizes control.
            break;
        }
        if !ready_sent && supervisor.machine().can_accept_control() {
            let result = validate_ready(
                supervisor
                    .machine()
                    .capabilities()
                    .expect("ready capabilities"),
                require_pid_filter,
            );
            if let Err(e) = result {
                facts.terminate();
                let _ = ready_tx.try_send(Err(e));
                break;
            }
            ready_sent = true;
            let _ = ready_tx.try_send(Ok(()));
        }
        if supervisor.stop_inbox().is_some() {
            // StopInbox is finite. Authorize against CURRENT facts, not the
            // session visible when the request was read/displayed.
            for _ in 0..16 {
                match supervisor.stop_inbox().expect("enabled inbox").try_pop() {
                    Ok(Some(session)) => {
                        facts.stop(&session);
                    }
                    Ok(None) => break,
                    Err(d) => {
                        supervisor.transport_failed(d);
                        facts.terminate();
                        break;
                    }
                }
            }
        }
        let progress = facts.flush().and_then(|_| {
            writer
                .pump(&mut pipes.writer)
                .map_err(FeedbackError::Protocol)
        });
        match progress {
            Ok(WriteProgress::Pending) if writer.buffered_bytes() > 0 => {
                let since = pending_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= WRITE_BOUND {
                    eprintln!("[computer-host] desktop_feedback snapshot write deadline exceeded; control revoked");
                    supervisor.transport_failed(Diagnostic::TransportLost);
                    facts.terminate();
                }
            }
            Ok(_) => pending_since = None,
            Err(_) => {
                supervisor.transport_failed(Diagnostic::TransportLost);
                facts.terminate();
            }
        }
        thread::sleep(POLL);
    }
    // Native shutdown can still be underway after loss. Avoid pretending kill
    // or a sent snapshot releases input. Worker independently reports unknown.
    let final_deadline = Instant::now() + Duration::from_millis(150);
    while Instant::now() < final_deadline {
        let _ = facts.flush();
        let _ = writer.pump(&mut pipes.writer);
        thread::sleep(POLL);
    }
    drop(pipes);
    teardown(&mut supervisor)
}
fn read_tick(
    supervisor: &mut Supervisor<super::process::ChildControl>,
    pipes: &mut Pipes,
    now: Duration,
    buffer: &mut [u8],
    require_pid_filter: bool,
) -> Result<(), FeedbackError> {
    for _ in 0..MAX_READS_PER_TICK {
        match pipes.reader.read(buffer) {
            Ok(0) => {
                supervisor.eof();
                break;
            }
            Ok(n) => {
                // Validate ready at its exact frame boundary. A renderer may
                // immediately follow unsupported ready with error/EOF; do not
                // erase the precise capability rejection with a generic loss.
                for chunk in buffer[..n].split_inclusive(|b| *b == b'\n') {
                    supervisor
                        .feed(now, chunk)
                        .map_err(FeedbackError::Protocol)?;
                    if let Some(ready) = supervisor.machine().capabilities() {
                        validate_ready(ready, require_pid_filter)?;
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                break
            }
            Err(_) => {
                supervisor.transport_failed(Diagnostic::TransportLost);
                break;
            }
        }
    }
    Ok(())
}
fn teardown(supervisor: &mut Supervisor<super::process::ChildControl>) -> FeedbackShutdown {
    let deadline = Instant::now() + TEARDOWN;
    let mut status = supervisor.begin_shutdown();
    while !status.reaped && Instant::now() < deadline {
        thread::sleep(POLL);
        match supervisor.poll_shutdown() {
            Ok(s) => status = s,
            Err(_) => break,
        }
    }
    if status.reaped {
        FeedbackShutdown::Reaped
    } else {
        eprintln!("[computer-host] desktop_feedback child reap unconfirmed");
        FeedbackShutdown::ProcessUnconfirmed
    }
}
