//! Registration-owned watchdog; no borrowed-fd closes or detached helper tasks.
use super::super::x11_lifecycle::Control;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

struct State {
    deadline: Option<Instant>,
    finished: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
#[derive(Clone)]
pub(super) struct Lease(Arc<Shared>);
pub(super) struct Watchdog {
    lease: Lease,
    join: JoinHandle<()>,
}

impl Watchdog {
    pub fn start(
        socket: Arc<UnixStream>,
        control: Arc<Control>,
        cancel: Arc<AtomicBool>,
        deadline: Instant,
    ) -> Result<Self, String> {
        let lease = Lease(Arc::new(Shared {
            state: Mutex::new(State {
                deadline: Some(deadline),
                finished: false,
            }),
            changed: Condvar::new(),
        }));
        let worker = lease.clone();
        let join = std::thread::Builder::new()
            .name("hotkey-x11-deadline".into())
            .spawn(move || {
                let shared = &worker.0;
                let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if state.finished {
                        break;
                    }
                    if let Some(deadline) = state.deadline {
                        let now = Instant::now();
                        if now >= deadline {
                            // Keep lease locked until failure is published: finishing an
                            // operation cannot race an already-won timeout into success.
                            control.begin_failed_cleanup(
                                "X11 hotkey operation timed out; connection shut down",
                                &cancel,
                            );
                            control.stop.store(true, Ordering::SeqCst);
                            let _ = socket.shutdown(Shutdown::Both);
                            break;
                        }
                        let (next, _) = shared
                            .changed
                            .wait_timeout(state, deadline - now)
                            .unwrap_or_else(|e| e.into_inner());
                        state = next;
                    } else {
                        state = shared
                            .changed
                            .wait(state)
                            .unwrap_or_else(|e| e.into_inner());
                    }
                }
            })
            .map_err(|e| format!("cannot spawn X11 deadline owner: {e}"))?;
        Ok(Self { lease, join })
    }
    pub fn lease(&self) -> Lease {
        self.lease.clone()
    }
    pub fn finish(self) {
        self.lease.finish();
        let _ = self.join.join();
    }
}
impl Lease {
    /// Arm a deadline for a bounded query/replay/cleanup roundtrip, not idle time.
    pub fn begin(&self, budget: Duration) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        let next = Instant::now() + budget;
        state.deadline = Some(state.deadline.map_or(next, |old| old.min(next)));
        self.0.changed.notify_all();
    }
    pub fn clear(&self) {
        self.0
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .deadline = None;
        self.0.changed.notify_all();
    }
    pub fn finish(&self) {
        self.0
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .finished = true;
        self.0.changed.notify_all();
    }
}
