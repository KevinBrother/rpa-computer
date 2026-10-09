//! Test-only, one-shot semantic checkpoint and bounded helper-thread cleanup.
//! A gate does nothing until armed; no absolute backend call counts or sleeps
//! are used to decide whether the production path reached the race boundary.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const WAIT_LIMIT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct State {
    armed: bool,
    entered: bool,
    released: bool,
}

#[derive(Default)]
pub(crate) struct RaceGate {
    state: Mutex<State>,
    changed: Condvar,
}
impl RaceGate {
    fn arm(&self) {
        let mut state = self.state.lock().unwrap();
        assert!(!state.armed && !state.released, "race gate is one-shot");
        state.armed = true;
    }

    /// Called at the actual backend boundary. Unarmed setup calls pass through.
    pub(crate) fn checkpoint(&self) -> Result<(), &'static str> {
        let mut state = self.state.lock().unwrap();
        if !state.armed || state.released {
            return Ok(());
        }
        state.entered = true;
        self.changed.notify_all();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, WAIT_LIMIT, |s| !s.released)
            .unwrap();
        if state.released {
            Ok(())
        } else {
            Err("race checkpoint release timed out")
        }
    }

    fn wait_entered(&self) -> Result<(), &'static str> {
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, WAIT_LIMIT, |s| !s.entered && !s.released)
            .unwrap();
        if state.entered && !state.released {
            Ok(())
        } else {
            Err("semantic race boundary was not entered before timeout/cleanup")
        }
    }

    fn release(&self) {
        // Unwind cleanup must not panic a second time if an assertion poisoned it.
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.released = true;
        self.changed.notify_all();
    }
}

/// Arms after successful test setup, then performs the adversarial action ONLY
/// after checkpoint entry. Panic/timeout releases the checkpoint and cancels.
/// Success does not mutate cancel on Drop (that could hide a resume bug).
pub(crate) struct RaceTask<T> {
    gate: Arc<RaceGate>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<T>>,
}
impl<T: Send + 'static> RaceTask<T> {
    pub(crate) fn spawn(
        gate: Arc<RaceGate>,
        cancel: Arc<AtomicBool>,
        action: impl FnOnce() -> T + Send + 'static,
    ) -> Self {
        // Construct the cleanup owner before spawning, including spawn failure.
        let mut task = Self {
            gate: gate.clone(),
            cancel: cancel.clone(),
            thread: None,
        };
        gate.arm();
        task.thread = Some(thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                gate.wait_entered()
                    .expect("production race checkpoint must be reached");
                action()
            }));
            if result.is_err() {
                cancel.store(true, Ordering::SeqCst);
            }
            gate.release();
            match result {
                Ok(value) => value,
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }));
        task
    }

    pub(crate) fn finish(mut self) -> T {
        assert!(
            wait_finished(self.thread.as_ref().unwrap()),
            "race helper completion timed out"
        );
        match self.thread.take().unwrap().join() {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}

fn wait_finished<T>(handle: &JoinHandle<T>) -> bool {
    let deadline = Instant::now() + WAIT_LIMIT;
    while !handle.is_finished() && Instant::now() < deadline {
        // Only bounded thread teardown polling, never race scheduling or a
        // substitute for the semantic checkpoint.
        thread::park_timeout(Duration::from_millis(1));
    }
    handle.is_finished()
}

impl<T> Drop for RaceTask<T> {
    fn drop(&mut self) {
        if self.thread.is_some() || thread::panicking() {
            self.cancel.store(true, Ordering::SeqCst);
            self.gate.release();
        }
        if let Some(handle) = self.thread.take() {
            if wait_finished(&handle) {
                let _ = handle.join(); // no second panic while unwinding
            } else {
                // Never hang the suite in an unbounded join. Test fails above
                // or is already unwinding; do not claim successful cleanup.
                eprintln!(
                    "test harness: race helper did not terminate; detached after bounded cleanup"
                );
            }
        }
    }
}
