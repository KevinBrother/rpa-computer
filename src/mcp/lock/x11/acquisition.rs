//! Owned setup-task lifetime. Cancellation shuts down the SAME socket used by
//! every XCB operation, then joins before returning; no background late setup.
use super::{
    auth, connection, transport, unavailable, LockError, ServerLease, SetupProbe, GRAB_BUDGET,
};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) struct Budget {
    overall: Instant,
    active: Mutex<Instant>,
    cancelled: AtomicBool,
}
impl Budget {
    fn new(deadline: Instant) -> Self {
        Self {
            overall: deadline,
            active: Mutex::new(deadline),
            cancelled: AtomicBool::new(false),
        }
    }
    pub(super) fn check(&self) -> Result<(), LockError> {
        if self.cancelled.load(Ordering::SeqCst) || Instant::now() >= self.deadline() {
            self.cancelled.store(true, Ordering::SeqCst);
            Err(unavailable(
                "X11 ownership setup exceeded its deadline or was cancelled",
            ))
        } else {
            Ok(())
        }
    }
    fn deadline(&self) -> Instant {
        *self.active.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub(super) fn enter_grab(&self) -> Result<Instant, LockError> {
        self.check()?;
        let deadline = self.overall.min(Instant::now() + GRAB_BUDGET);
        *self.active.lock().unwrap_or_else(|p| p.into_inner()) = deadline;
        Ok(deadline)
    }
    pub(super) fn leave_grab(&self) {
        // Expiration is sticky: a late ungrab cannot reset an expired budget.
        let _ = self.check();
        *self.active.lock().unwrap_or_else(|p| p.into_inner()) = self.overall;
    }
}

struct Task {
    interrupt: UnixStream,
    budget: Arc<Budget>,
    join: Option<JoinHandle<()>>,
    committed: bool,
}
impl Task {
    fn join(&mut self) -> Result<(), LockError> {
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| unavailable("X11 ownership setup task panicked"))?;
        }
        Ok(())
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        if !self.committed {
            self.budget.cancelled.store(true, Ordering::SeqCst);
            // Never close a borrowed XCB fd. This is an owned duplicate made
            // BEFORE handshake, so shutdown also interrupts xcb_connect_to_fd.
            let _ = self.interrupt.shutdown(Shutdown::Both);
        }
        let _ = self.join();
    }
}

pub(super) fn run(
    display: String,
    duration: Duration,
    mut probe: SetupProbe,
) -> Result<ServerLease, LockError> {
    let deadline = Instant::now() + duration;
    let number = transport::display_number(&display)?;
    // Known separate blocker: filesystem auth loading is NOT interruptible.
    // No X socket/task exists until this returns; see auth.rs and handoff.
    let cookie = auth::load(&number, deadline)?;
    let nonce = transport::nonce(deadline)?;
    let socket = transport::open(&number, deadline)?;
    let interrupt = socket.try_clone().map_err(unavailable)?;
    let budget = Arc::new(Budget::new(deadline));
    let worker_budget = budget.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    transport::check_deadline(deadline)?;
    let join = std::thread::Builder::new()
        .name("computer-x11-acquire".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker_budget.check()?;
                let connection =
                    connection::connect(socket.into(), cookie.0.as_deref()).map_err(unavailable)?;
                worker_budget.check()?;
                ServerLease::acquire_on_connection(
                    connection,
                    nonce,
                    deadline,
                    &worker_budget,
                    &mut probe,
                )
            }))
            .unwrap_or_else(|_| Err(unavailable("X11 ownership setup task panicked")));
            let _ = tx.send(result);
            // probe and credentials are retired before the caller's join completes.
        })
        .map_err(|e| unavailable(format!("cannot start X11 lease setup: {e}")))?;
    let mut task = Task {
        interrupt,
        budget: budget.clone(),
        join: Some(join),
        committed: false,
    };
    let result = loop {
        budget.check()?; // Task::drop shuts down + joins on EVERY error/unwind.
        let wait = budget
            .deadline()
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(5));
        match rx.recv_timeout(wait) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(unavailable("X11 ownership setup ended without a result"))
            }
        }
    };
    // Join even successful setup before publishing. Pending result ownership
    // drops/revokes on cancellation. The monitor starts only AFTER setup retires.
    let lease = result?;
    task.join()?;
    budget.check()?;
    lease.start_monitor()?;
    budget.check()?;
    task.committed = true;
    Ok(lease)
}
