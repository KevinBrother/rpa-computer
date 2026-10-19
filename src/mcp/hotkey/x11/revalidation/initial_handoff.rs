//! Exact post-grab/pre-ready boundary, sharing capture's ORIGINAL init budget.
use super::{queue, Baseline, Source};
use std::collections::VecDeque;
use std::time::Instant;

/// Complete the same initialization; never recapture A/B, renew its deadline or
/// substitute the candidate queue's independent budget/Press-only filtering.
/// The caller still performs ensure_initializing and the existing locked
/// accept_ready transition. Success here is not itself an Armed publication.
pub(in crate::mcp::hotkey::x11) fn finish_initial<S: Source>(
    source: &mut S,
    baseline: &mut Baseline,
    pending: &mut VecDeque<S::Event>,
    deadline: Instant,
) -> Result<(), String> {
    let generation = baseline.generation;
    queue::pending(source, pending, generation, deadline)?;
    // Successful capture must have charged every retained event plus its final
    // empty poll. Reject corrupted/exhausted accounting, do not repair/reset it.
    if baseline.initial_queue_polls <= pending.len() || baseline.initial_queue_polls >= queue::LIMIT
    {
        return Err("XKB init capture poll accounting is invalid/exhausted before handoff".into());
    }
    source.fence(deadline)?;
    queue::drain(
        source,
        pending,
        &mut baseline.initial_queue_polls,
        generation,
        deadline,
    )?;
    // drain checks immediately AFTER queued(None); also check here at the
    // handoff's return boundary. A fence-entry-only guard is insufficient.
    queue::checkpoint(source, generation, deadline)
}
