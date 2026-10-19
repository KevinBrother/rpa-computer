//! R1: production worker/completion paths, not only a fake Transport's error flag.
#[path = "../src/native_ui/mod.rs"]
mod native_ui;

use native_ui::{process_boundary::*, Exit};
use rpa_desktop_feedback::protocol::{RendererMessage, Session, Stop, V1};
use std::{
    cell::Cell,
    io::{self, Cursor, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

struct Writer {
    bytes: Vec<u8>,
    allowance: usize,
    error: io::ErrorKind,
}
impl Writer {
    fn blocked() -> Self {
        Self {
            bytes: vec![],
            allowance: 0,
            error: io::ErrorKind::WouldBlock,
        }
    }
    fn writable() -> Self {
        Self {
            allowance: usize::MAX,
            ..Self::blocked()
        }
    }
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.allowance == 0 {
            return Err(self.error.into());
        }
        let n = bytes.len().min(self.allowance);
        self.bytes.extend_from_slice(&bytes[..n]);
        self.allowance -= n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Worker {
    state: Arc<Mutex<PipeState>>,
    done: Arc<AtomicBool>,
    fault: Arc<AtomicBool>,
    exit: Option<WorkerExit>,
    completion: Completion,
    origin: Instant,
}
impl Worker {
    fn new(queue_stop: bool) -> Self {
        let origin = Instant::now();
        let state = Arc::new(Mutex::new(PipeState::new()));
        if queue_stop {
            state
                .lock()
                .unwrap()
                .enqueue(
                    RendererMessage::Stop(Stop {
                        version: V1,
                        session: Session {
                            id: "parent-session".into(),
                            generation: 7,
                        },
                    }),
                    || origin,
                )
                .unwrap();
        }
        let done = Arc::new(AtomicBool::new(false));
        let fault = Arc::new(AtomicBool::new(false));
        let completion = Completion::new(state.clone(), done.clone(), fault.clone());
        let exit = Some(WorkerExit::new(done.clone(), fault.clone()));
        Self {
            state,
            done,
            fault,
            exit,
            completion,
            origin,
        }
    }
    fn turn(&mut self, writer: &mut Writer, elapsed_ms: u64) -> WorkerDecision {
        // Same production worker body, fixed time; no thread/sleep or fake PipeBridge.
        let decision = self.state.lock().unwrap().turn(
            &mut Cursor::new(Vec::<u8>::new()),
            writer,
            Some(self.origin),
            || self.origin + Duration::from_millis(elapsed_ms),
        );
        if decision.fault {
            self.fault.store(true, Ordering::SeqCst);
        }
        if decision.end {
            self.exit.as_mut().unwrap().mark_clean();
            drop(self.exit.take());
        }
        decision
    }
    fn eof_code(&self) -> i32 {
        let elapsed = Cell::new(0);
        finish_process(
            Exit::ParentEof,
            &self.completion,
            || self.origin + Duration::from_millis(elapsed.get()),
            || elapsed.set(elapsed.get() + 2),
        )
    }
}

#[test]
fn r1_eof_with_fully_drained_worker_is_clean() {
    let mut w = Worker::new(true);
    let mut output = Writer::writable();
    assert!(w.turn(&mut output, 0).end);
    assert!(!output.bytes.is_empty());
    assert_eq!(
        w.completion.snapshot(),
        CompletionSnapshot {
            done: true,
            fault: false,
            pending_frames: Some(0),
        }
    );
    assert_eq!(w.eof_code(), 0);
}

#[test]
fn r1_closing_budget_does_not_end_before_200ms() {
    let mut w = Worker::new(true);
    assert!(!w.turn(&mut Writer::blocked(), 199).end);
    assert!(!w.done.load(Ordering::SeqCst));
    assert_eq!(w.completion.snapshot().pending_frames, Some(1));
}

#[test]
fn r1_200ms_eagain_with_queued_stop_records_worker_failure() {
    let mut w = Worker::new(true);
    assert!(w.turn(&mut Writer::blocked(), 200).end);
    assert_eq!(w.completion.snapshot().pending_frames, Some(1));
    assert!(
        w.completion.snapshot().fault,
        "R1: 200ms drain expiry with unsent control must not be a clean worker completion"
    );
}

#[test]
fn r1_200ms_eagain_with_queued_stop_cannot_return_clean_eof() {
    let mut w = Worker::new(true);
    assert!(w.turn(&mut Writer::blocked(), 200).end);
    assert_eq!(w.completion.snapshot().pending_frames, Some(1));
    assert_ne!(w.eof_code(), 0, "R1: queued Stop was not drained");
}

#[test]
fn r1_partial_stop_then_eagain_cannot_return_clean_eof() {
    let mut w = Worker::new(true);
    let mut output = Writer {
        allowance: 3,
        ..Writer::blocked()
    };
    assert!(!w.turn(&mut output, 0).end);
    assert_eq!(output.bytes.len(), 3);
    assert!(w.turn(&mut output, 200).end);
    assert_eq!(w.completion.snapshot().pending_frames, Some(1));
    assert_ne!(w.eof_code(), 0, "R1: a partial frame is not delivery");
}

#[test]
fn r1_final_drain_epipe_overrides_parent_eof() {
    let mut w = Worker::new(true);
    let mut output = Writer {
        error: io::ErrorKind::BrokenPipe,
        ..Writer::blocked()
    };
    assert!(w.turn(&mut output, 1).end);
    assert!(w.completion.snapshot().fault);
    assert_ne!(
        w.eof_code(),
        0,
        "R1: final writer fault must affect the process result"
    );
}

#[test]
fn r1_unfinished_worker_at_main_deadline_cannot_return_clean_eof() {
    let w = Worker::new(true);
    // This is the sample taken after the bounded main wait, not a request to join.
    assert!(!w.completion.snapshot().done);
    let elapsed = Cell::new(0);
    let code = finish_process(
        Exit::ParentEof,
        &w.completion,
        || w.origin + Duration::from_millis(elapsed.get()),
        || {
            elapsed.set(elapsed.get() + 2);
            assert!(
                elapsed.get() <= 300,
                "main wait must remain bounded, never join"
            );
        },
    );
    assert_eq!(elapsed.get(), 300);
    assert_ne!(code, 0, "R1: expired wait without completion is not clean");
}

#[test]
fn r1_empty_queue_without_worker_ack_is_not_clean() {
    let w = Worker::new(false);
    assert_eq!(w.completion.snapshot().pending_frames, Some(0));
    assert_ne!(
        w.eof_code(),
        0,
        "R1: queue emptiness cannot replace completion acknowledgment"
    );
}

#[test]
fn r1_unexpected_worker_exit_overrides_even_empty_eof() {
    let mut w = Worker::new(false);
    drop(w.exit.take()); // real production RAII exit, without its clean mark
    assert!(w.completion.snapshot().done);
    assert!(w.completion.snapshot().fault);
    assert_ne!(w.eof_code(), 0, "R1: worker loss must not become clean EOF");
}

#[test]
fn r1_unknown_queue_receipt_cannot_return_clean_eof() {
    let mut w = Worker::new(false);
    assert!(w.turn(&mut Writer::writable(), 0).end);
    let _held = w.state.lock().unwrap();
    assert_eq!(w.completion.snapshot().pending_frames, None);
    assert_ne!(
        w.eof_code(),
        0,
        "R1: contended drain receipt is unknown, not confirmed empty"
    );
}

#[test]
fn r1_ui_failure_is_not_downgraded_by_successful_drain() {
    let mut w = Worker::new(true);
    assert!(w.turn(&mut Writer::writable(), 0).end);
    assert_ne!(process_exit_code(Exit::Failure, w.completion.snapshot()), 0);
}

#[test]
fn r1_partial_stop_can_finish_cleanly_within_budget() {
    let mut w = Worker::new(true);
    let mut output = Writer {
        allowance: 3,
        ..Writer::blocked()
    };
    assert!(!w.turn(&mut output, 0).end);
    output.allowance = usize::MAX;
    assert!(w.turn(&mut output, 199).end);
    assert_eq!(w.completion.snapshot().pending_frames, Some(0));
    assert_eq!(w.eof_code(), 0);
}
