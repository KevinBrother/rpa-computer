//! Owned stdio process and bounded asynchronous pipe I/O.
//! Successful exit is cleanup evidence; dedicated pre-ready statuses prove
//! no input grant was made. Killing or uncertain init is never cleanup proof.
use crate::mcp::{jsonrpc, worker::Worker};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) const EXIT_GRACE: Duration = Duration::from_secs(
    Worker::TRANSPORT_CALL_TIMEOUT.as_secs()
        + crate::mcp::worker::MAX_SHUTDOWN_CONFIRM.as_secs()
        + 3,
);
const KILL_GRACE: Duration = Duration::from_secs(3);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_success_or_pre_ready_lock_refusal_proves_retirement() {
        assert_eq!(classify_exit(Some(0), false, false), Retirement::Clean);
        assert_eq!(classify_exit(Some(3), false, false), Retirement::NoGrant);
        assert_eq!(classify_exit(Some(3), true, false), Retirement::Unknown);
        for code in [None, Some(4), Some(7), Some(1)] {
            assert_eq!(classify_exit(code, false, false), Retirement::Unknown);
        }
        assert_eq!(classify_exit(Some(0), true, true), Retirement::Unknown);
    }
    #[test]
    fn pre_ready_lock_refusal_survives_initializer_broken_pipe() {
        let (mut child, _events, _writes) = Executor::test_pair();
        child.ready = false;
        child.broken.store(true, Ordering::SeqCst);
        child.test_exit(3);
        assert_eq!(child.poll_exit(), Some(Retirement::NoGrant));
    }
    // Exit 9 is reserved for a positively identified no-input setup failure.
    // Exit 4 remains ambiguous; deadline/panic/cleanup failure must not use 9.
    #[test]
    fn pre_ready_no_input_setup_failure_is_not_unknown_cleanup() {
        assert_eq!(classify_exit(Some(9), false, false), Retirement::NoGrant);
        assert_eq!(classify_exit(Some(9), true, false), Retirement::Unknown);
        assert_eq!(classify_exit(Some(9), false, true), Retirement::Unknown);
        assert_eq!(classify_exit(Some(4), false, false), Retirement::Unknown);
        assert_eq!(classify_exit(None, false, false), Retirement::Unknown);
    }

    #[test]
    fn pre_ready_no_input_failure_survives_initializer_broken_pipe() {
        let (mut child, _events, _writes) = Executor::test_pair();
        child.ready = false;
        child.broken.store(true, Ordering::SeqCst);
        child.test_exit(9);
        assert_eq!(child.poll_exit(), Some(Retirement::NoGrant));
    }

    #[test]
    fn read_failure_or_helper_failure_is_sticky_despite_zero_exit() {
        for helper_failure in [false, true] {
            let (mut child, _events, _writes) = Executor::test_pair();
            if helper_failure {
                child.uncertain = true;
            } else {
                child.broken.store(true, Ordering::SeqCst);
            }
            child.test_exit(0);
            assert_eq!(child.poll_exit(), Some(Retirement::Unknown));
            child.broken.store(false, Ordering::SeqCst);
            assert_eq!(child.poll_exit(), Some(Retirement::Unknown));
        }
    }
    #[test]
    fn helper_drain_grace_starts_at_observed_exit_not_shutdown_request() {
        let (mut child, _events, _writes) = Executor::test_pair();
        let (ready_tx, ready_rx) = sync_channel(0);
        let (release_tx, release_rx) = sync_channel(1);
        child.reader = Some(std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        }));
        ready_rx.recv_timeout(KILL_GRACE).unwrap();
        // Native shutdown legitimately consumed more than the pipe-drain
        // grace, but less than its independent process-exit allowance.
        child.stopping = Some(Instant::now() - KILL_GRACE - Duration::from_secs(1));
        child.test_exit(0);
        let first = child.poll_exit();
        // Release even on the pre-fix failure path so fixture teardown never
        // leaves a gated helper behind when the assertion reports RED.
        release_tx.send(()).unwrap();
        let until = Instant::now() + KILL_GRACE;
        while !child.reader.as_ref().unwrap().is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            first, None,
            "a newly observed exit gets its own drain grace"
        );
        assert!(child.reader.as_ref().unwrap().is_finished());
        assert_eq!(child.poll_exit(), Some(Retirement::Clean));
    }

    #[test]
    fn forced_exit_is_never_cleanup_proof() {
        let (mut child, _events, _writes) = Executor::test_pair();
        child.forced = true;
        child.test_exit(0);
        assert_eq!(child.poll_exit(), Some(Retirement::Unknown));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Retirement {
    Clean,
    NoGrant,
    Revoked,
    Unknown,
}

fn classify_exit(code: Option<i32>, ready: bool, forced: bool) -> Retirement {
    if forced {
        Retirement::Unknown
    } else if code == Some(0) {
        Retirement::Clean
    } else if matches!(code, Some(3 | 9)) && !ready {
        Retirement::NoGrant
    } else if code == Some(8) {
        Retirement::Revoked
    } else {
        Retirement::Unknown
    }
}

#[derive(Debug)]
pub(super) enum Event {
    Reply(Value),
    Broken,
}

pub(super) struct Executor {
    child: Option<Child>,
    input: Option<SyncSender<String>>,
    pub events: Receiver<Event>,
    stop: Arc<AtomicBool>,
    broken: Arc<AtomicBool>,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    pub ready: bool,
    stopping: Option<Instant>,
    killed_at: Option<Instant>,
    status: Option<ExitStatus>,
    /// Independent of shutdown/kill grace: helpers can drain only after exit.
    exit_observed_at: Option<Instant>,
    pub forced: bool,
    uncertain: bool,
}

impl Executor {
    pub fn spawn(mock: bool, feedback: &crate::feedback::FeedbackConfig) -> std::io::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        if mock {
            command.arg("--mock-backend");
        }
        feedback.append_args(&mut command);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = sync_channel::<String>(4);
        let (events_tx, events) = sync_channel(4);
        let stop = Arc::new(AtomicBool::new(false));
        let broken = Arc::new(AtomicBool::new(false));
        // From this point on, even helper-thread creation failure returns the
        // owned process handle to the broker for bounded, fail-closed retirement.
        let mut executor = Self {
            child: Some(child),
            input: Some(tx),
            events,
            stop: stop.clone(),
            broken: broken.clone(),
            writer: None,
            reader: None,
            ready: false,
            stopping: None,
            killed_at: None,
            status: None,
            exit_observed_at: None,
            forced: false,
            uncertain: false,
        };
        let writer_stop = stop.clone();
        let writer_broken = broken.clone();
        let writer = std::thread::Builder::new()
            .name("desktop-task-stdin".into())
            .spawn(move || {
                while !writer_stop.load(Ordering::SeqCst) {
                    match rx.recv_timeout(Duration::from_millis(10)) {
                        Ok(line) => {
                            if writer_stop.load(Ordering::SeqCst) {
                                break;
                            }
                            if stdin
                                .write_all(line.as_bytes())
                                .and_then(|_| stdin.write_all(b"\n"))
                                .and_then(|_| stdin.flush())
                                .is_err()
                            {
                                writer_broken.store(true, Ordering::SeqCst);
                                break;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(_) => break,
                    }
                }
                // Dropping the unique stdin handle signals EOF directly to the child's
                // priority ingress, even while the main stdio thread is in Worker::call.
            });
        match writer {
            Ok(h) => executor.writer = Some(h),
            Err(_) => {
                executor.uncertain = true;
                executor.stop();
                return Ok(executor);
            }
        };
        let reader_broken = broken.clone();
        let reader = std::thread::Builder::new()
            .name("desktop-task-stdout".into())
            .spawn(move || {
                let mut input = BufReader::new(stdout);
                let mut line = Vec::new();
                loop {
                    // fill_buf/consume bounds allocation BEFORE appending, unlike read_line.
                    let chunk = match input.fill_buf() {
                        Ok([]) => break,
                        Err(_) => {
                            reader_broken.store(true, Ordering::SeqCst);
                            break;
                        }
                        Ok(bytes) => bytes,
                    };
                    let n = chunk
                        .iter()
                        .position(|b| *b == b'\n')
                        .map_or(chunk.len(), |i| i + 1);
                    if line.len() + n > jsonrpc::MAX_OUT_LINE_BYTES {
                        reader_broken.store(true, Ordering::SeqCst);
                        break;
                    }
                    let complete = chunk[n - 1] == b'\n';
                    line.extend_from_slice(&chunk[..n]);
                    input.consume(n);
                    if complete {
                        let reply = serde_json::from_slice(&line);
                        line.clear();
                        match reply {
                            Ok(value) => {
                                if events_tx.try_send(Event::Reply(value)).is_err() {
                                    reader_broken.store(true, Ordering::SeqCst);
                                    break;
                                }
                            }
                            Err(_) => {
                                reader_broken.store(true, Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                }
                if !line.is_empty() {
                    reader_broken.store(true, Ordering::SeqCst);
                }
                let _ = events_tx.try_send(Event::Broken);
            });
        match reader {
            Ok(h) => executor.reader = Some(h),
            Err(_) => {
                executor.uncertain = true;
                executor.stop();
            }
        }
        Ok(executor)
    }

    pub fn send(&self, value: Value) -> bool {
        self.input
            .as_ref()
            .is_some_and(|tx| tx.try_send(value.to_string()).is_ok())
    }

    pub fn stop(&mut self) {
        self.stopping.get_or_insert_with(Instant::now);
        self.stop.store(true, Ordering::SeqCst);
        self.input.take();
    }

    pub fn is_broken(&self) -> bool {
        self.broken.load(Ordering::SeqCst)
    }

    /// Nonblocking reaper. Some(Unknown) may retain a live unkillable process;
    /// the broker MUST retain this Executor and quarantine, never spawn another.
    pub fn poll_exit(&mut self) -> Option<Retirement> {
        if self.status.is_none() {
            if let Some(child) = &mut self.child {
                match child.try_wait() {
                    Ok(status) => self.status = status,
                    Err(_) => {
                        self.uncertain = true;
                        self.stop();
                    }
                }
            }
        }
        if let Some(status) = self.status {
            let exit_observed_at = *self.exit_observed_at.get_or_insert_with(Instant::now);
            self.stop();
            let joined = self.writer.as_ref().is_none_or(|h| h.is_finished())
                && self.reader.as_ref().is_none_or(|h| h.is_finished());
            if joined {
                let mut panicked = false;
                if let Some(h) = self.writer.take() {
                    panicked |= h.join().is_err();
                }
                if let Some(h) = self.reader.take() {
                    panicked |= h.join().is_err();
                }
                self.uncertain |= panicked;
                let preliminary =
                    classify_exit(status.code(), self.ready, self.forced || self.uncertain);
                // Exit 3 (lease refusal) and 9 (confirmed no-input setup failure)
                // are pre-ready only. Their initializer
                // write may race the process exit: broken pipe cannot turn that
                // proven non-grant into an unknown cleanup outcome.
                let result = if preliminary == Retirement::NoGrant {
                    preliminary
                } else if self.is_broken() {
                    Retirement::Unknown
                } else {
                    preliminary
                };
                self.uncertain |= result == Retirement::Unknown;
                return Some(result);
            }
            if exit_observed_at.elapsed() >= KILL_GRACE {
                self.uncertain = true;
                return Some(Retirement::Unknown);
            }
        } else if self.stopping.is_some_and(|s| s.elapsed() >= EXIT_GRACE)
            && self.killed_at.is_none()
        {
            self.forced = true;
            self.killed_at = Some(Instant::now());
            if let Some(child) = &mut self.child {
                let _ = child.kill();
            }
        } else if self.killed_at.is_some_and(|s| s.elapsed() >= KILL_GRACE) {
            self.uncertain = true;
            return Some(Retirement::Unknown);
        }
        None
    }

    #[cfg(test)]
    pub(super) fn test_pair() -> (Self, SyncSender<Event>, Receiver<String>) {
        let (input, writes) = sync_channel(4);
        let (output, events) = sync_channel(4);
        (
            Self {
                child: None,
                input: Some(input),
                events,
                stop: Arc::new(AtomicBool::new(false)),
                broken: Arc::new(AtomicBool::new(false)),
                writer: None,
                reader: None,
                ready: true,
                stopping: None,
                killed_at: None,
                status: None,
                exit_observed_at: None,
                forced: false,
                uncertain: false,
            },
            output,
            writes,
        )
    }

    #[cfg(test)]
    pub(super) fn test_exit(&mut self, code: i32) {
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            self.status = Some(ExitStatus::from_raw(code as u32));
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            self.status = Some(ExitStatus::from_raw(code << 8));
        }
    }

    pub fn fully_reaped(&self) -> bool {
        self.status.is_some() && self.writer.is_none() && self.reader.is_none()
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.stop();
        if self.status.is_none() && self.child.is_some() {
            self.forced = true;
            if let Some(child) = &mut self.child {
                let _ = child.kill();
            }
            let until = Instant::now() + KILL_GRACE;
            while Instant::now() < until {
                if let Ok(Some(status)) = self.child.as_mut().expect("owned process").try_wait() {
                    self.status = Some(status);
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        if let Some(h) = self.writer.take() {
            let _ = super::pump::wait_join(h, KILL_GRACE);
        }
        if let Some(h) = self.reader.take() {
            let _ = super::pump::wait_join(h, KILL_GRACE);
        }
        // Never release Authority here; caller has to prove clean retirement.
    }
}
