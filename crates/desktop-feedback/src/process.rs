//! Host-private subprocess interfaces. This library never actually spawns a
//! renderer. Platform Host adapters supply private pipe and child ownership.
use crate::{
    codec::{decode_renderer_line, NdjsonDecoder, MAX_LINE_BYTES},
    control::StopInbox,
    protocol::RendererMessage,
    state::{Failure, ProtocolMachine, ReceiveOutcome, Timeouts},
    Diagnostic,
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackMode {
    Disabled,
    Enabled,
}
#[derive(Debug, Clone, Copy)]
pub struct SupervisorConfig {
    pub timeouts: Timeouts,
    pub stop_queue_capacity: usize,
}
impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            timeouts: Timeouts::default(),
            stop_queue_capacity: 16,
        }
    }
}

/// Shell-free argv, no file existence checks. Configuration is trusted Host
/// input, never fields taken from a renderer message. Style is deliberately small.
#[derive(Debug, Clone)]
pub struct RendererCommand {
    executable: PathBuf,
    args: Vec<String>,
}
impl RendererCommand {
    pub fn new(
        executable: impl Into<PathBuf>,
        accent: Option<String>,
        label: Option<String>,
    ) -> Result<Self, Diagnostic> {
        let executable = executable.into();
        if executable.as_os_str().is_empty() {
            return Err(Diagnostic::InvalidCommand);
        }
        let mut args = Vec::with_capacity(4);
        if let Some(accent) = accent {
            if accent.len() != 7
                || !accent.starts_with('#')
                || !accent.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
            {
                return Err(Diagnostic::InvalidCommand);
            }
            args.extend(["--accent".into(), accent]);
        }
        if let Some(label) = label {
            if label.is_empty()
                || label.chars().count() > 64
                || label.len() > 256
                || label.chars().any(char::is_control)
            {
                return Err(Diagnostic::InvalidCommand);
            }
            args.extend(["--label".into(), label]);
        }
        Ok(Self { executable, args })
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    pub fn args(&self) -> &[String] {
        &self.args
    }
    /// Only constructs a command; never executes it. Pipes are private; stderr
    /// defaults to null so an undrained diagnostic pipe cannot deadlock startup.
    /// Adapters may capture stderr only using a bounded independent drainer.
    pub fn to_std_command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        command
    }
}

/// Methods must be nonblocking; `poll_reaped` confirms actual child reaping,
/// not that a quit message was sent. Keep renderer handles private to this Host.
pub trait ProcessControl {
    fn poll_exit(&mut self) -> Result<bool, Diagnostic>;
    fn request_terminate(&mut self) -> Result<(), Diagnostic>;
    fn poll_reaped(&mut self) -> Result<bool, Diagnostic>;
}
/// The adapter must use the Host interactive desktop, piped stdin/stdout, no
/// network listener, and retain ownership of all child/pipe handles. This trait
/// does not assert that Session0 or a successful spawn constitutes real Ready.
pub trait PrivateSpawner {
    type Process: ProcessControl;
    fn spawn_private(&mut self, command: &RendererCommand) -> Result<Self::Process, Diagnostic>;
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReceiveSummary {
    pub frames: usize,
    pub queued_stops: usize,
    pub stops_before_ready: usize,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownStatus {
    pub terminate_requested: bool,
    pub reaped: bool,
    pub diagnostic: Option<Diagnostic>,
}

pub struct Supervisor<P: ProcessControl> {
    process: Option<P>,
    machine: ProtocolMachine,
    decoder: Option<NdjsonDecoder>,
    stops: Option<StopInbox>,
    shutting_down: bool,
    shutdown_status: ShutdownStatus,
}
impl<P: ProcessControl> Supervisor<P> {
    /// Disabled returns BEFORE validating timeouts, resolving/reading renderer
    /// path, allocating wire buffers, or invoking the spawner.
    /// Enabled success means *awaiting ready*, not successful Host service startup.
    pub fn start<S: PrivateSpawner<Process = P>>(
        mode: FeedbackMode,
        now: Duration,
        config: SupervisorConfig,
        resolve_command: impl FnOnce() -> Result<RendererCommand, Diagnostic>,
        spawner: &mut S,
    ) -> Result<Self, Failure> {
        if mode == FeedbackMode::Disabled {
            return Ok(Self {
                process: None,
                machine: ProtocolMachine::disabled(),
                decoder: None,
                stops: None,
                shutting_down: false,
                shutdown_status: ShutdownStatus::default(),
            });
        }
        let machine = ProtocolMachine::enabled(now, config.timeouts).map_err(Failure::startup)?;
        let stops = StopInbox::new(config.stop_queue_capacity).map_err(Failure::startup)?;
        let decoder = NdjsonDecoder::new(MAX_LINE_BYTES).map_err(Failure::startup)?;
        let command = resolve_command().map_err(Failure::startup)?;
        let process = spawner
            .spawn_private(&command)
            .map_err(|_| Failure::startup(Diagnostic::SpawnFailed))?;
        Ok(Self {
            process: Some(process),
            machine,
            decoder: Some(decoder),
            stops: Some(stops),
            shutting_down: false,
            shutdown_status: ShutdownStatus::default(),
        })
    }
    pub fn machine(&self) -> &ProtocolMachine {
        &self.machine
    }
    pub fn stop_inbox(&self) -> Option<&StopInbox> {
        self.stops.as_ref()
    }
    /// Adapter access for pipe ownership only; never share these with clients.
    pub fn process_mut(&mut self) -> Option<&mut P> {
        self.process.as_mut()
    }
    pub fn receive(
        &mut self,
        now: Duration,
        message: RendererMessage,
    ) -> Result<ReceiveOutcome, Diagnostic> {
        let outcome = self.machine.receive(now, message);
        self.accept_outcome(outcome)
    }
    fn accept_outcome(&mut self, outcome: ReceiveOutcome) -> Result<ReceiveOutcome, Diagnostic> {
        if let ReceiveOutcome::Failed(failure) = &outcome {
            return Err(failure.diagnostic);
        }
        if let ReceiveOutcome::StopCandidate(session) = &outcome {
            if let Err(error) = self
                .stops
                .as_ref()
                .expect("enabled stop queue")
                .try_push(session.clone())
            {
                self.machine.fail(error);
                return Err(error);
            }
        }
        Ok(outcome)
    }
    /// Bounded pure stream ingress. No collected event list; each stop enters the
    /// fixed mailbox, each failure latches once. Call from the lifecycle/I/O owner.
    pub fn feed(&mut self, now: Duration, bytes: &[u8]) -> Result<ReceiveSummary, Diagnostic> {
        if self.shutting_down {
            return Ok(ReceiveSummary::default());
        }
        let Some(decoder) = self.decoder.as_mut() else {
            return Ok(ReceiveSummary::default());
        };
        if let Some(failure) = self.machine.failure() {
            return Err(failure.diagnostic);
        }
        let mut summary = ReceiveSummary::default();
        let machine = &mut self.machine;
        let stops = self.stops.as_ref().expect("enabled stop queue");
        let result = decoder.feed(bytes, |line| {
            let message = decode_renderer_line(line)?;
            summary.frames += 1;
            match machine.receive(now, message) {
                ReceiveOutcome::Failed(failure) => Err(failure.diagnostic),
                ReceiveOutcome::StopCandidate(session) => {
                    stops.try_push(session)?;
                    summary.queued_stops += 1;
                    Ok(())
                }
                ReceiveOutcome::StopBeforeReady => {
                    summary.stops_before_ready += 1;
                    Ok(())
                }
                _ => Ok(()),
            }
        });
        if let Err(error) = result {
            self.machine.fail(error);
            return Err(error);
        }
        Ok(summary)
    }
    /// Clean wire EOF still loses enabled feedback. EOF during deliberate child
    /// shutdown is not evidence of input cleanup and is not a new renderer fault.
    pub fn eof(&mut self) -> Option<Failure> {
        if self.shutting_down {
            return self.machine.failure();
        }
        let decoder = self.decoder.as_mut()?;
        let diagnostic = decoder.finish().err().unwrap_or(Diagnostic::TransportLost);
        Some(self.machine.fail(diagnostic))
    }
    pub fn transport_failed(&mut self, diagnostic: Diagnostic) -> Option<Failure> {
        if self.decoder.is_none() || self.shutting_down {
            return self.machine.failure();
        }
        Some(self.machine.fail(diagnostic))
    }
    /// The Host must schedule this even when no bytes arrive. Feed alone cannot
    /// detect a silent child. Failures are sticky and must trigger native cleanup.
    pub fn poll(&mut self, now: Duration) -> Option<Failure> {
        if self.shutting_down {
            return self.machine.failure();
        }
        self.machine.tick(now);
        if let Some(process) = self.process.as_mut() {
            match process.poll_exit() {
                Ok(false) => {}
                Ok(true) | Err(_) => {
                    self.machine.fail(Diagnostic::ProcessLost);
                }
            }
        }
        self.machine.failure()
    }
    /// Only call for deliberate Host teardown, after denying grants and arranging
    /// input cancellation/cleanup. Does not claim that native input is released.
    pub fn begin_shutdown(&mut self) -> ShutdownStatus {
        self.shutting_down = true;
        self.machine.shutdown();
        self.drive_shutdown()
    }
    pub fn poll_shutdown(&mut self) -> Result<ShutdownStatus, Diagnostic> {
        if !self.shutting_down {
            return Err(Diagnostic::UnexpectedMessage);
        }
        Ok(self.drive_shutdown())
    }
    fn drive_shutdown(&mut self) -> ShutdownStatus {
        let Some(process) = self.process.as_mut() else {
            self.shutdown_status.reaped = true;
            return self.shutdown_status;
        };
        self.shutdown_status.diagnostic = None;
        if !self.shutdown_status.terminate_requested {
            if process.request_terminate().is_err() {
                self.shutdown_status.diagnostic = Some(Diagnostic::TerminateFailed);
                return self.shutdown_status;
            }
            self.shutdown_status.terminate_requested = true;
        }
        match process.poll_reaped() {
            Ok(true) => {
                self.shutdown_status.reaped = true;
                self.process = None;
            }
            Ok(false) => {}
            Err(_) => {
                self.shutdown_status.diagnostic = Some(Diagnostic::ReapFailed);
            }
        }
        self.shutdown_status
    }
}
impl<P: ProcessControl> Drop for Supervisor<P> {
    fn drop(&mut self) {
        // Best effort only, never a cleanup/reap success claim. The integrating
        // Host must explicitly drive shutdown and enforce its own child deadline.
        if let Some(process) = self.process.as_mut() {
            let _ = process.request_terminate();
        }
    }
}
