//! Deadline watchdog owns only a duplicated socket, never an XCB connection.
//! It interrupts stalled setup, flush and replies even when the caller is idle.
use crate::backend::{linux_clipboard::policy::error, BackendError};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Control {
    pub stop: AtomicBool,
    pub failure: OnceLock<BackendError>,
    deadline: Mutex<Option<Instant>>,
}
impl Control {
    pub fn arm(&self, until: Instant) -> Result<(), BackendError> {
        let mut deadline = self
            .deadline
            .lock()
            .map_err(|_| error("clipboard deadline state poisoned"))?;
        if self.stop.load(Ordering::SeqCst)
            || deadline.is_some_and(|d| Instant::now() >= d)
            || Instant::now() >= until
        {
            return Err(self.expired());
        }
        *deadline = Some(until);
        Ok(())
    }
    pub fn disarm(&self) -> Result<(), BackendError> {
        let mut deadline = self
            .deadline
            .lock()
            .map_err(|_| error("clipboard deadline state poisoned"))?;
        if self.stop.load(Ordering::SeqCst) || deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(self.expired());
        }
        *deadline = None;
        Ok(())
    }
    fn expired(&self) -> BackendError {
        self.stop.store(true, Ordering::SeqCst);
        let e = error("X11 clipboard I/O deadline exceeded or cancelled; transport interrupted; clipboard outcome unknown");
        let _ = self.failure.set(e.clone());
        e
    }
    pub fn watch(self: &Arc<Self>, socket: UnixStream) -> Result<JoinHandle<()>, BackendError> {
        let control = self.clone();
        std::thread::Builder::new()
            .name("computer-clipboard-deadline".into())
            .spawn(move || {
                loop {
                    if control.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let deadline = match control.deadline.lock() {
                        Ok(deadline) => deadline,
                        Err(_) => {
                            control.expired();
                            break;
                        }
                    };
                    if deadline.is_some_and(|d| Instant::now() >= d) {
                        control.expired();
                        break;
                    }
                    drop(deadline);
                    std::thread::park_timeout(Duration::from_millis(2));
                }
                // shutdown affects the shared socket transport, including the fd
                // held by raw xcb_connect_to_fd, not merely the watchdog's duplicate.
                let _ = socket.shutdown(Shutdown::Both);
            })
            .map_err(|e| error(format!("cannot start clipboard transport watchdog: {e}")))
    }
}
