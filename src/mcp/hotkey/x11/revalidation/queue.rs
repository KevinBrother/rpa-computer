//! Bounded ordered queue observation shared by capture and the init handoff.
//! No callback/readiness/cancel side effects; a returned error revokes upstream.
use super::{Instant, Source, VecDeque};

pub(super) const LIMIT: usize = 256;

pub(super) fn checkpoint<S: Source>(
    source: &mut S,
    generation: usize,
    deadline: Instant,
) -> Result<(), String> {
    source.checkpoint(deadline)?;
    if source.generation() != generation {
        return Err("XKB registration generation changed during observation".into());
    }
    Ok(())
}

pub(super) fn pending<S: Source>(
    source: &mut S,
    events: &VecDeque<S::Event>,
    generation: usize,
    deadline: Instant,
) -> Result<(), String> {
    checkpoint(source, generation, deadline)?;
    if events.len() >= LIMIT {
        return Err("XKB pending FIFO exceeds coherent queue budget".into());
    }
    for event in events {
        if source.invalidation(event).is_some() {
            return Err("XKB invalidation already pending before coherent observation".into());
        }
    }
    checkpoint(source, generation, deadline)
}

/// `used` is aggregate poll attempts, including None, never reset by this drain.
/// Capture charges prefetched events once; initial handoff reuses capture's exact
/// counter. No polling once exhausted, even to guess that the queue is empty.
pub(super) fn drain<S: Source>(
    source: &mut S,
    pending: &mut VecDeque<S::Event>,
    used: &mut usize,
    generation: usize,
    deadline: Instant,
) -> Result<(), String> {
    while *used < LIMIT {
        checkpoint(source, generation, deadline)?;
        *used += 1;
        match source.queued(deadline)? {
            None => return checkpoint(source, generation, deadline),
            Some(event) => {
                if source.invalidation(&event).is_some() {
                    return Err("XKB invalidation/companion during coherent observation".into());
                }
                if pending.len() >= LIMIT {
                    return Err("XKB coherent FIFO capacity exceeded".into());
                }
                pending.push_back(event);
            }
        }
    }
    Err("XKB aggregate coherent queue poll budget exhausted".into())
}
