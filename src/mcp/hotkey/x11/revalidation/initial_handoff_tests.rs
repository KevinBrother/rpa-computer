//! N2: direct post-grab/pre-ready production handoff, not another copy of the
//! capture protocol. No actual X/input, readiness publication or test execution.
use super::fixture::{baseline, deadline, Receipt, Script};
use super::*;

fn captured(pending_count: usize) -> Baseline {
    let mut b = baseline();
    // Model a completed initial capture: one poll per retained ordinary event
    // plus its empty-queue observation. This is INPUT state, not a drain model.
    b.initial_queue_polls = pending_count + 1;
    b
}
fn handoff_positive_control() {
    finish_initial(
        &mut Script::good(),
        &mut captured(0),
        &mut VecDeque::new(),
        deadline(),
    )
    .expect("N2 RED: actual post-grab handoff must succeed for a quiet valid queue");
}

#[test]
fn post_grab_handoff_keeps_existing_and_new_press_release_other_fifo() {
    let mut source = Script::good();
    let mut baseline = captured(2);
    let mut pending = VecDeque::from([Receipt::Release(10), Receipt::Other(1)]);
    source
        .events
        .extend([Receipt::Press(9), Receipt::Release(9), Receipt::Other(2)]);
    finish_initial(&mut source, &mut baseline, &mut pending, deadline()).unwrap();
    assert_eq!(
        pending,
        [
            Receipt::Release(10),
            Receipt::Other(1),
            Receipt::Press(9),
            Receipt::Release(9),
            Receipt::Other(2)
        ]
    );
    assert_eq!(
        source.trace,
        ["fence", "queued", "queued", "queued", "queued"]
    );
    assert_eq!(baseline.initial_queue_polls, 7); // capture3 + handoff4
    assert_eq!(
        source.images.len(),
        2,
        "handoff must not repeat A/B snapshot reads"
    );
}

#[test]
fn capture_and_handoff_share_256_total_poll_attempts_including_empty_results() {
    let mut source = Script::good();
    let mut baseline = captured(252); //253 used; two final polls available below limit
    let mut pending = (0..252).map(|_| Receipt::Other(0)).collect();
    source.events.push_back(Receipt::Release(9));
    finish_initial(&mut source, &mut baseline, &mut pending, deadline()).unwrap();
    assert_eq!(baseline.initial_queue_polls, 255);
    assert_eq!(pending.len(), 253);

    for used in [255, 256, 257] {
        let mut source = Script::good();
        let mut baseline = captured(used - 1);
        let mut pending = (0..used - 1).map(|_| Receipt::Other(0)).collect();
        source
            .events
            .extend([Receipt::Press(9), Receipt::Release(9)]);
        assert!(finish_initial(&mut source, &mut baseline, &mut pending, deadline()).is_err());
        let polls = source.trace.iter().filter(|v| **v == "queued").count();
        assert!(
            polls <= 256usize.saturating_sub(used),
            "do not reset budget after grabs"
        );
    }
}

#[test]
fn late_invalidations_and_prefetched_fatal_event_reject_the_handoff() {
    handoff_positive_control();
    for reason in [
        Invalidation::NewKeyboard,
        Invalidation::CoreMapping,
        Invalidation::Map,
        Invalidation::Controls,
        Invalidation::Compatibility,
    ] {
        let mut source = Script::good();
        let mut b = captured(1);
        let mut pending = VecDeque::from([Receipt::Other(1)]);
        source.events.extend([
            Receipt::Release(9),
            Receipt::Fatal(reason),
            Receipt::Press(9),
        ]);
        assert!(finish_initial(&mut source, &mut b, &mut pending, deadline()).is_err());
    }
    let mut source = Script::good();
    let mut pending = VecDeque::from([Receipt::Fatal(Invalidation::Controls)]);
    assert!(finish_initial(&mut source, &mut captured(1), &mut pending, deadline()).is_err());
}

#[test]
fn original_init_deadline_stop_failure_and_generation_cannot_be_reset_at_handoff() {
    let limit = deadline();
    let mut source = Script::good();
    finish_initial(&mut source, &mut captured(0), &mut VecDeque::new(), limit).unwrap();
    assert!(!source.deadlines.is_empty());
    assert!(source.deadlines.iter().all(|d| *d == limit));

    for failure in 0..8 {
        let mut source = Script::good();
        let limit = deadline();
        match failure {
            0 => source.now = limit,
            1 => source.stopped = true,
            2 => source.expire_at_fence = true,
            3 => source.stop_at_fence = true,
            4 => source.fail_at_fence = true,
            5 => source.replace_at_fence = true,
            6 => source.fence_error = true,
            _ => source.queue_error = true,
        }
        assert!(
            finish_initial(&mut source, &mut captured(0), &mut VecDeque::new(), limit).is_err()
        );
        assert!(source.deadlines.iter().all(|d| *d == limit));
        if failure < 2 {
            assert!(
                source.trace.is_empty(),
                "no X request after already won expiry/stop"
            );
        }
    }
}
