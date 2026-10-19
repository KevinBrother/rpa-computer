//! Exact production pre-decision boundary after QueryKeymap, before cancel or
//! replay. Extra physical presses reject regardless of queue placement.
#[cfg(test)]
mod tests;
use super::super::x11_candidate::{self, Candidate, Decision};
use super::super::x11_plan::HotkeyPlan;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn check_then_decide(
    pending: &VecDeque<xcb::Event>,
    validate_queued: impl FnOnce() -> Result<Vec<xcb::Event>, String>,
    stop: &AtomicBool,
    plan: &HotkeyPlan,
    cancel: &AtomicBool,
    candidate: Candidate<'_>,
) -> Result<Option<Decision>, String> {
    // Preserve the XCB drain/error ordering, then inspect the prefetched
    // remainder with the same real-KeyPress predicate before decide/cancel.
    // Borrow only: synthetic and ordinary nonpress events stay in FIFO order.
    if !validate_queued()?.is_empty()
        || pending.iter().any(|event| {
            matches!(event, xcb::Event::X(xcb::x::Event::KeyPress(key))
                if key.response_type() & 0x80 == 0)
        })
    {
        return Err("unexpected concurrent physical press while synchronous grab is frozen".into());
    }
    if stop.load(Ordering::SeqCst) {
        return Ok(None);
    }
    Ok(Some(x11_candidate::decide(plan, cancel, candidate)))
}
