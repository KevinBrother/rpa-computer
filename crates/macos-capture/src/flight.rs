use crate::{CaptureError, Deadline, ErrorKind, Result, Stage};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Instant;

#[derive(Debug)]
struct State<T> {
    result: Option<Result<T>>,
    completed: bool,
    abandoned: bool,
    discovery_claimed: bool,
    image_claimed: bool,
}
#[derive(Debug)]
pub(crate) struct Flight<T> {
    state: Mutex<State<T>>,
    wake: Condvar,
    deadline: Deadline,
}
#[derive(Debug)]
pub(crate) enum Poll<T> {
    Pending,
    Ready(Result<T>),
    Timeout,
    Consumed,
}

impl<T> Flight<T> {
    pub(crate) fn new(deadline: Deadline) -> Self {
        Self {
            state: Mutex::new(State {
                result: None,
                completed: false,
                abandoned: false,
                discovery_claimed: false,
                image_claimed: false,
            }),
            wake: Condvar::new(),
            deadline,
        }
    }
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        // No native objects or user closures are manipulated under this lock.
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
    pub(crate) fn is_active(&self) -> bool {
        !self.lock().completed
    }
    // Fn blocks may be copied/invoked by Objective-C. Even an unexpected duplicate
    // must not issue another screenshot or concurrent PNG encode for this flight.
    pub(crate) fn claim_discovery(&self) -> bool {
        let mut state = self.lock();
        if state.completed || state.discovery_claimed {
            return false;
        }
        state.discovery_claimed = true;
        true
    }
    pub(crate) fn claim_image(&self) -> bool {
        let mut state = self.lock();
        if state.completed || state.image_claimed {
            return false;
        }
        state.image_claimed = true;
        true
    }

    pub(crate) fn may_continue_at(&self, now: Instant) -> bool {
        let mut state = self.lock();
        if self.deadline.remaining_at(now).is_none() {
            state.abandoned = true;
        }
        !state.abandoned && !state.completed
    }
    pub(crate) fn complete_at(&self, result: Result<T>, now: Instant) {
        let mut state = self.lock();
        if state.completed {
            return;
        } // Duplicate callback cannot overwrite a result.
        state.completed = true;
        if state.abandoned || self.deadline.remaining_at(now).is_none() {
            state.abandoned = true;
            state.result = None;
        } else {
            state.result = Some(result);
        }
        self.wake.notify_all();
    }
    fn poll_locked(&self, state: &mut State<T>, now: Instant) -> Poll<T> {
        if state.abandoned || self.deadline.remaining_at(now).is_none() {
            state.abandoned = true;
            state.result = None;
            return Poll::Timeout;
        }
        if let Some(result) = state.result.take() {
            return Poll::Ready(result);
        }
        if state.completed {
            Poll::Consumed
        } else {
            Poll::Pending
        }
    }
    #[cfg(test)]
    pub(crate) fn poll_at(&self, now: Instant) -> Poll<T> {
        self.poll_locked(&mut self.lock(), now)
    }
    pub(crate) fn wait(&self) -> Result<T> {
        let mut state = self.lock();
        loop {
            let now = Instant::now();
            match self.poll_locked(&mut state, now) {
                Poll::Ready(result) => return result,
                Poll::Timeout => return Err(CaptureError::new(ErrorKind::Timeout, Stage::Wait)),
                Poll::Consumed => {
                    return Err(CaptureError::new(ErrorKind::CaptureFailed, Stage::Wait))
                }
                Poll::Pending => {}
            }
            let Some(remaining) = self.deadline.remaining_at(now) else {
                continue;
            };
            state = self
                .wake
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poison| poison.into_inner())
                .0;
        }
    }
}

/// Per-client sticky quarantine after timeout. This slot does not forget a live
/// OS call when a caller gives up; it is reusable only once the callback completes.
#[derive(Debug)]
pub(crate) struct FlightSlot<T> {
    current: Option<Arc<Flight<T>>>,
}
impl<T> Default for FlightSlot<T> {
    fn default() -> Self {
        Self { current: None }
    }
}
impl<T> FlightSlot<T> {
    pub(crate) fn begin(&mut self, deadline: Deadline) -> Result<Arc<Flight<T>>> {
        if self
            .current
            .as_ref()
            .is_some_and(|flight| flight.is_active())
        {
            return Err(CaptureError::new(ErrorKind::Busy, Stage::Wait));
        }
        let flight = Arc::new(Flight::new(deadline));
        self.current = Some(Arc::clone(&flight));
        Ok(flight)
    }
    pub(crate) fn is_busy(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|flight| flight.is_active())
    }
}
