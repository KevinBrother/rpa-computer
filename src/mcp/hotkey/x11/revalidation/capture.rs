//! Exactly two complete, independently semantically validated observations and
//! one ordered fence. No retry, lease renewal, event swallowing or server writes.
use super::{queue, Baseline, Instant, Source, VecDeque};

pub(super) fn stable<S: Source>(
    source: &mut S,
    pending: &mut VecDeque<S::Event>,
    deadline: Instant,
) -> Result<Baseline, String> {
    let generation = source.generation();
    queue::pending(source, pending, generation, deadline)?;
    // Prefetched FIFO entries cost budget too; all newly polled events (and the
    // final None) are counted by the single drain. Initial FIFO starts empty.
    let mut polls = pending.len();
    let a = Baseline::validated(generation, source.read_image(deadline)?)?;
    queue::checkpoint(source, generation, deadline)?;
    let mut b = Baseline::validated(generation, source.read_image(deadline)?)?;
    queue::checkpoint(source, generation, deadline)?;
    if a.image != b.image || a.plan != b.plan || a.validated.modifiers != b.validated.modifiers {
        return Err("XKB complete observations A/B are not identical".into());
    }
    source.fence(deadline)?;
    queue::drain(source, pending, &mut polls, generation, deadline)?;
    // Carry the REAL count, not pending.len(): the successful empty poll must
    // remain charged when the post-grab/preready handoff takes over (N2/G1).
    b.initial_queue_polls = polls;
    queue::checkpoint(source, generation, deadline)?;
    Ok(b)
}
