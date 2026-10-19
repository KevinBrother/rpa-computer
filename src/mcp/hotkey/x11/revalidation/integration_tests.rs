//! Supplemental G1/G2 regressions for the SAME observed-N2 contract. Authored
//! before its accepting implementation; no separate historical RED claimed.
use super::fixture::{baseline, deadline, Receipt, Script};
use super::*;

#[test]
fn actual_capture_to_handoff_carries_poll_attempts_and_the_entire_fifo() {
    let limit = deadline();
    let mut source = Script::good();
    source.events.extend((0..250).map(|_| Receipt::Other(1)));
    source
        .events
        .extend([Receipt::Press(9), Receipt::Release(9)]);
    let mut pending = VecDeque::new();
    // Actual production capture, not a manually fabricated poll counter.
    let mut b = capture_initial(&mut source, &mut pending, limit).unwrap();
    assert_eq!(b.initial_queue_polls, 253); //252 events and its empty poll
    assert_eq!(pending.len(), 252);
    source
        .events
        .extend([Receipt::Release(10), Receipt::Other(2)]);
    finish_initial(&mut source, &mut b, &mut pending, limit).unwrap();
    assert_eq!(b.initial_queue_polls, 256); //two more events + empty poll
    let mut expected: VecDeque<_> = (0..250).map(|_| Receipt::Other(1)).collect();
    expected.extend([
        Receipt::Press(9),
        Receipt::Release(9),
        Receipt::Release(10),
        Receipt::Other(2),
    ]);
    assert_eq!(pending, expected);
    assert!(source.deadlines.iter().all(|d| *d == limit));
    assert_eq!(source.trace.iter().filter(|v| **v == "image").count(), 2);
    assert_eq!(source.trace.iter().filter(|v| **v == "fence").count(), 2);
    assert_eq!(source.trace.iter().filter(|v| **v == "queued").count(), 256);

    let limit = deadline();
    let mut source = Script::good();
    source.events.extend((0..253).map(|_| Receipt::Other(1)));
    let mut pending = VecDeque::new();
    let mut b = capture_initial(&mut source, &mut pending, limit).unwrap();
    assert_eq!(b.initial_queue_polls, 254);
    source
        .events
        .extend([Receipt::Press(9), Receipt::Release(9), Receipt::Other(2)]);
    assert!(finish_initial(&mut source, &mut b, &mut pending, limit).is_err());
    assert_eq!(source.trace.iter().filter(|v| **v == "queued").count(), 256);
}

#[derive(Clone, Copy)]
enum AfterNone {
    Quiet,
    Stop,
    Failure,
    Generation,
    Expire,
}
struct LastNone {
    inner: Script,
    change: AfterNone,
}
impl Source for LastNone {
    type Event = Receipt;
    fn generation(&self) -> usize {
        self.inner.generation()
    }
    fn checkpoint(&mut self, deadline: Instant) -> Result<(), String> {
        self.inner.checkpoint(deadline)
    }
    fn read_image(&mut self, deadline: Instant) -> Result<SemanticImage, String> {
        self.inner.read_image(deadline)
    }
    fn fence(&mut self, deadline: Instant) -> Result<(), String> {
        self.inner.fence(deadline)
    }
    fn invalidation(&self, event: &Receipt) -> Option<Invalidation> {
        self.inner.invalidation(event)
    }
    fn queued(&mut self, deadline: Instant) -> Result<Option<Receipt>, String> {
        let event = self.inner.queued(deadline)?;
        if event.is_none() {
            // AFTER the final queue adapter's own entry/checkpoint. A fence-only
            // guard or checking before queued is insufficient for this seam.
            match self.change {
                AfterNone::Quiet => {}
                AfterNone::Stop => self.inner.stopped = true,
                AfterNone::Failure => self.inner.failed = true,
                AfterNone::Generation => self.inner.generation += 1,
                AfterNone::Expire => self.inner.now = deadline,
            }
        }
        Ok(event)
    }
}

#[test]
fn handoff_last_none_requires_post_return_lifecycle_and_generation_check() {
    for change in [
        AfterNone::Quiet,
        AfterNone::Stop,
        AfterNone::Failure,
        AfterNone::Generation,
        AfterNone::Expire,
    ] {
        let limit = deadline();
        let mut b = baseline();
        b.initial_queue_polls = 1;
        let mut source = LastNone {
            inner: Script::good(),
            change,
        };
        let result = finish_initial(&mut source, &mut b, &mut VecDeque::new(), limit);
        if matches!(change, AfterNone::Quiet) {
            result.expect("positive control: quiet handoff succeeds");
        } else {
            assert!(
                result.is_err(),
                "late queue-boundary failure cannot hand off"
            );
        }
        assert_eq!(source.inner.trace, ["fence", "queued"]);
        assert!(source.inner.deadlines.iter().all(|d| *d == limit));
    }
    // This tests a finite last-check boundary, NOT every concurrent schedule
    // between return, ready send and the existing locked readiness acceptance.
}
