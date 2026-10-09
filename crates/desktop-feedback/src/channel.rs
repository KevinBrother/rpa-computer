//! Single latest-snapshot storage. Never queue pointer positions. All shared
//! critical sections are bounded, contain no I/O, and use `try_lock` only.
use crate::{protocol::Snapshot, Diagnostic};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, TryLockError,
};

const MAX_SUBSCRIBERS: usize = 64;
struct Shared {
    state: Mutex<Slot>,
    subscribers: AtomicUsize,
    max_subscribers: usize,
}
struct Slot {
    latest: Option<Arc<Snapshot>>,
    sequence: Option<u64>,
}

pub struct Publisher {
    shared: Arc<Shared>,
}
#[derive(Debug)]
pub struct PublishError {
    pub diagnostic: Diagnostic,
    pub snapshot: Box<Snapshot>,
}
impl PublishError {
    fn new(diagnostic: Diagnostic, snapshot: Snapshot) -> Self {
        Self {
            diagnostic,
            snapshot: Box::new(snapshot),
        }
    }
}
impl Publisher {
    pub fn new(max_subscribers: usize) -> Result<Self, Diagnostic> {
        if max_subscribers == 0 || max_subscribers > MAX_SUBSCRIBERS {
            return Err(Diagnostic::InvalidLimit);
        }
        Ok(Self {
            shared: Arc::new(Shared {
                state: Mutex::new(Slot {
                    latest: None,
                    sequence: None,
                }),
                subscribers: AtomicUsize::new(0),
                max_subscribers,
            }),
        })
    }
    /// Full state only. On contention/rejection the snapshot is returned intact.
    /// The Host must retain/retry its latest authoritative state on a worker;
    /// do not loop or block the input thread. Accepted snapshots always increase.
    pub fn try_publish(&self, snapshot: Snapshot) -> Result<(), PublishError> {
        if let Err(error) = snapshot.validate() {
            return Err(PublishError::new(error, snapshot));
        }
        let mut slot = match self.shared.state.try_lock() {
            Ok(slot) => slot,
            Err(error) => return Err(PublishError::new(lock_error(error), snapshot)),
        };
        if slot.sequence == Some(u64::MAX) {
            return Err(PublishError::new(Diagnostic::SequenceExhausted, snapshot));
        }
        if slot
            .sequence
            .is_some_and(|sequence| snapshot.sequence <= sequence)
        {
            return Err(PublishError::new(Diagnostic::SequenceRegression, snapshot));
        }
        slot.sequence = Some(snapshot.sequence);
        slot.latest = Some(Arc::new(snapshot));
        Ok(())
    }
    /// A new subscriber always resynchronizes from the current complete state.
    pub fn subscribe(&self) -> Result<Subscriber, Diagnostic> {
        self.shared
            .subscribers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < self.shared.max_subscribers).then_some(count + 1)
            })
            .map_err(|_| Diagnostic::TooManySubscribers)?;
        Ok(Subscriber {
            shared: Arc::clone(&self.shared),
            sequence: None,
        })
    }
    pub fn retained_snapshot_count(&self) -> Result<usize, Diagnostic> {
        let slot = self.shared.state.try_lock().map_err(lock_error)?;
        Ok(usize::from(slot.latest.is_some()))
    }
}

pub struct Subscriber {
    shared: Arc<Shared>,
    sequence: Option<u64>,
}
impl Subscriber {
    pub fn poll_latest(&mut self) -> Result<Option<Arc<Snapshot>>, Diagnostic> {
        let slot = self.shared.state.try_lock().map_err(lock_error)?;
        if slot.sequence == self.sequence {
            return Ok(None);
        }
        self.sequence = slot.sequence;
        Ok(slot.latest.clone())
    }
}
impl Drop for Subscriber {
    fn drop(&mut self) {
        self.shared.subscribers.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Renderer-side presentation cursor: older snapshots do not regress the UI.
/// Fresh private process/stream => fresh cursor (not arbitrary reset on a live stream).
#[derive(Debug, Default)]
pub struct SnapshotCursor {
    sequence: Option<u64>,
}
impl SnapshotCursor {
    pub fn accept(&mut self, snapshot: &Snapshot) -> Result<bool, Diagnostic> {
        snapshot.validate()?;
        if self
            .sequence
            .is_some_and(|sequence| snapshot.sequence <= sequence)
        {
            return Ok(false);
        }
        self.sequence = Some(snapshot.sequence);
        Ok(true)
    }
    pub fn sequence(&self) -> Option<u64> {
        self.sequence
    }
}
pub(crate) fn lock_error<T>(error: TryLockError<T>) -> Diagnostic {
    match error {
        TryLockError::WouldBlock => Diagnostic::Contended,
        TryLockError::Poisoned(_) => Diagnostic::TransportLost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Cleanup, Phase, V1};

    fn snapshot(sequence: u64) -> Snapshot {
        Snapshot {
            version: V1,
            sequence,
            session: None,
            phase: Phase::Starting,
            cleanup: Cleanup::NotNeeded,
            surface: None,
            pointer: None,
        }
    }

    #[test]
    fn publication_contention_returns_original_without_waiting_or_advancing_sequence() {
        let publisher = Publisher::new(1).unwrap();
        let lock = publisher.shared.state.lock().unwrap();
        let error = publisher.try_publish(snapshot(7)).unwrap_err();
        assert_eq!(error.diagnostic, Diagnostic::Contended);
        assert_eq!(error.snapshot.sequence, 7);
        assert!(lock.sequence.is_none());
        drop(lock);
        publisher.try_publish(*error.snapshot).unwrap();
        assert_eq!(
            publisher
                .subscribe()
                .unwrap()
                .poll_latest()
                .unwrap()
                .unwrap()
                .sequence,
            7
        );
    }

    #[test]
    fn subscriber_contention_does_not_advance_cursor_or_allocate_a_position_queue() {
        let publisher = Publisher::new(1).unwrap();
        let mut subscriber = publisher.subscribe().unwrap();
        publisher.try_publish(snapshot(1)).unwrap();
        let lock = publisher.shared.state.lock().unwrap();
        assert_eq!(subscriber.poll_latest(), Err(Diagnostic::Contended));
        assert!(subscriber.sequence.is_none());
        drop(lock);
        assert_eq!(subscriber.poll_latest().unwrap().unwrap().sequence, 1);
    }
}
