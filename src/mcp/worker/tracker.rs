//! Unified, mutex-guarded request tracker: ONE shared state viewed by both
//! the in-flight handle ([`ActiveRequest`]) and the transport gate
//! ([`TransportGate`]), so the queued→active transition and every cancel
//! lookup happen under the SAME lock — a cancel can never fall into the gap
//! where a request was already dequeued but not yet marked active.
//!
//! Populations and invariants:
//! - A request id lives in exactly ONE place at a time: `queued`, `active`,
//!   or `tombstoned`. Sets are never cleared wholesale; ids leave only via
//!   the ONE completion point ([`TransportGate::complete`]): a dispatched id
//!   stays `active` until that complete, and a cancelled-still-queued id
//!   stays tombstoned until its refused dispatch completes it. Dispatch
//!   itself NEVER removes state, so an id is reserved end-to-end and no
//!   complete can erase a re-registered id's fresh state.
//! - Capacity is accounted across the WHOLE tracked population
//!   (queued + active + tombstoned), not per-set: a peer cannot grow the
//!   tracker without limit through any single transition.
//! - A `register` refusal (duplicate in-flight id or full capacity) NEVER
//!   touches existing state — the original request keeps its registration.
//! - A queued id cancelled at ingress is TOMBSTONED and stays reserved
//!   until its (still-queued) frame reaches dispatch AND that refused
//!   dispatch completes it; `take_dispatch_decision` reports the refusal
//!   without consuming the tombstone. Until that cleanup the id is still in
//!   flight: a duplicate register is refused, and a replayed cancel matches
//!   the tombstone itself (never a global flag).
//! - Cancelling the ACTIVE id performs the cancellation (cheap atomics on
//!   the caller-provided [`CancelHandle`]) INSIDE the tracker's lock, in the
//!   same critical section as the active-match lookup — there is no
//!   check-then-act split at the caller and no way to target the wrong
//!   request or lose the cancel. A QUEUED-id cancel flips NO global flag
//!   and bumps NO generation — unrelated active work keeps running.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::RequestGeneration;

/// Bound on the transport-visible tracked population, accounted across the
/// WHOLE tracker (queued + active + tombstoned). Larger than any legitimate
/// pending population (the frame queue itself holds at most 64 frames); a
/// flooding peer can never grow the tracker without limit.
pub(crate) const MAX_TRACKED_REQUEST_IDS: usize = 256;

/// Shared state guarded by one mutex. `queued`/`tombstoned` membership is
/// exclusive per id by construction (every transition moves the id).
#[derive(Default)]
struct TrackerInner {
    /// In-flight request id (dispatched, not yet completed). Reserved from
    /// dispatch until the ONE RAII [`TransportGate::complete`] for that id.
    active: Option<Value>,
    /// Accepted request ids not yet dispatched.
    queued: HashSet<Value>,
    /// Ids cancelled while queued: reserved until their still-queued frame
    /// reaches dispatch (refused as `cancelled`) and that path completes
    /// the id. Dispatch reports but does NOT consume the tombstone.
    tombstoned: HashSet<Value>,
}

impl TrackerInner {
    /// Total tracked population (the single bound covers every set).
    fn population(&self) -> usize {
        self.queued.len() + self.tombstoned.len() + usize::from(self.active.is_some())
    }
}

/// Cancellation controller handed to the I/O side. Setting cancellation is
/// cheap (atomics only, never behind the worker), and is idempotent. Every
/// GLOBAL stop also bumps the request generation so open/resume calls queued
/// BEFORE the stop are refused while fresh deliberate ones still work. A
/// per-request cancel of a queued id does NOT go through this handle — it
/// only tombstones that id in the tracker (see [`IngressVerdict`]).
#[derive(Clone)]
pub struct CancelHandle {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) generation: RequestGeneration,
}

impl CancelHandle {
    pub fn cancel(&self) {
        // Bump the epoch BEFORE latching the flag. The native epoch guard
        // checks "flag still set?" AFTER reading the generation, so a
        // generation that is already new while the flag is not yet visible
        // is conservatively re-latched by the guard — the opposite order
        // (flag first) could clear-and-miss a stop whose bump lands between
        // the guard's generation read and its flag read. No stop is lost.
        self.generation.bump();
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Identifies the in-flight request so `notifications/cancelled` applies
/// ONLY to the request the client actually cancelled — an unsolicited id
/// must not stop unrelated work.
///
/// This handle is a VIEW over the unified tracker state (shared with the
/// transport gate): the queued→active transition and every cancel lookup
/// happen under ONE lock, so a cancel can never fall into the gap where a
/// request was already dequeued but not yet marked active.
#[derive(Clone)]
pub struct ActiveRequest {
    inner: Arc<Mutex<TrackerInner>>,
}

impl ActiveRequest {
    /// Mark `id` as the in-flight request (None clears).
    pub fn set(&self, id: Option<Value>) {
        if let Ok(mut g) = self.inner.lock() {
            g.active = id;
        }
    }

    /// True if `request_id` matches the in-flight request. This only
    /// *matches*; the actual cancellation for a validated cancel is
    /// performed by [`TransportGate::cancel_request`] under the same lock.
    pub fn matches(&self, request_id: &Value) -> bool {
        self.inner
            .lock()
            .map(|g| g.active.as_ref() == Some(request_id))
            .unwrap_or(false)
    }

    pub fn current(&self) -> Option<Value> {
        self.inner.lock().ok().and_then(|g| g.active.clone())
    }
}

/// Admission result for one ingress frame, travelling with the frame in the
/// typed [`super::super::ingress::Frame`] envelope. NEVER parsed from client
/// bytes: only the reader (via [`TransportGate::register`]) can produce a
/// `Refused` here, so a client cannot forge its own admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Not a cancellable JSON-RPC request (notification/response/malformed)
    /// or an internal/direct frame: no tracking, proceed normally.
    NotTracked,
    /// Registered in the tracker at ingress.
    Accepted,
    /// Registration refused: duplicate in-flight id or full capacity. The
    /// service MUST answer an explicit refusal (never execute, never touch
    /// the original request's state).
    Refused,
}

/// Transport-visible cancellation gate AND in-flight tracker: ONE shared
/// state (this struct and [`ActiveRequest`] are two views of the same
/// lock), so the queued→active transition and every cancel lookup are
/// atomic — a `notifications/cancelled` that races dispatch always finds
/// the request in exactly one place (queued, active, or tombstoned).
///
/// Semantics (validated ids only, never a blanket cancel):
/// - The READER `register`s every structurally valid JSON-RPC REQUEST id it
///   accepts (bounded; capacity and duplicate in-flight ids are REFUSED
///   explicitly so no untracked request ever executes and a rejected
///   duplicate never disturbs the original request's state), and stamps
///   each accepted frame with the CURRENT stop generation.
/// - A `notifications/cancelled` yields a verdict from
///   [`TransportGate::cancel_request`]: `ActiveMatched` (the cancellation
///   itself — cheap atomics on the passed [`CancelHandle`] — was performed
///   INSIDE the tracker's lock, in the same critical section as the
///   lookup) or `Tombstoned` (the still-queued frame is refused as
///   `cancelled` at dispatch; NO global flag, NO generation bump). Unknown
///   or completed ids yield `NoMatch` and change nothing.
/// - `take_dispatch_decision` ATOMICALLY marks a queued id active (or
///   reports it tombstoned) WITHOUT consuming any state: every id stays
///   reserved until the ONE completion point, [`TransportGate::complete`],
///   which removes the queued/tombstone membership AND the active marker
///   only when it matches that id. A refused duplicate owns no registration
///   and completes nothing, so its drop can never erase the original's
///   state; a completed id matches NOTHING, so id reuse after completion is
///   always safe.
#[derive(Clone)]
pub struct TransportGate {
    inner: Arc<Mutex<TrackerInner>>,
}

/// Verdict of a validated cancel lookup (see
/// [`TransportGate::cancel_request`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelVerdict {
    /// The id names nothing tracked: cancel NOTHING.
    NoMatch,
    /// The id names the in-flight request: the cancellation was performed
    /// (cheap atomics) under the tracker's lock before this verdict was
    /// returned. Queued requests are deliberately NOT tombstoned — they
    /// belong to other requests and keep their own fate.
    ActiveMatched,
    /// The id was queued (or already tombstoned): it is tombstoned so its
    /// still-queued frame is refused as `cancelled` at dispatch. No global
    /// flag, no generation bump — unrelated active work keeps running.
    Tombstoned,
}

impl TransportGate {
    pub(crate) fn new() -> Self {
        TransportGate {
            inner: Arc::new(Mutex::new(TrackerInner::default())),
        }
    }

    /// Standalone gate for ingress/service unit tests (not wired to a
    /// worker; the shared-lock semantics are identical).
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::new()
    }

    /// The shared in-flight view (same lock).
    pub(crate) fn active_view(&self) -> ActiveRequest {
        ActiveRequest {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Register an accepted request id at ingress. Returns the admission
    /// decision: `Accepted`, or `Refused` when the tracked population is at
    /// its bound OR the id is already in flight (queued, active, or a
    /// not-yet-cleaned-up tombstone). A refusal NEVER touches existing
    /// state — the original request keeps its registration.
    pub fn register(&self, id: Value) -> Admission {
        let Ok(mut g) = self.inner.lock() else {
            return Admission::Refused;
        };
        if g.queued.contains(&id) || g.active.as_ref() == Some(&id) || g.tombstoned.contains(&id) {
            // Duplicate in-flight id (a cancelled-but-undelivered id is
            // still in flight until its dispatch completes it): refuse,
            // keep the original.
            return Admission::Refused;
        }
        if g.population() >= MAX_TRACKED_REQUEST_IDS {
            return Admission::Refused;
        }
        g.queued.insert(id);
        Admission::Accepted
    }

    /// Validated cancel of one request id. The lookup AND its effect are
    /// performed under the SAME lock that guards the queued→active
    /// transition: there is no check-then-act split at the caller, so a
    /// cancel of the ACTIVE id can neither target the wrong request nor be
    /// lost — the cheap atomic cancel on `cancel` happens inside the
    /// critical section that matched the id. See [`CancelVerdict`] for the
    /// exact effect of each outcome.
    pub fn cancel_request(&self, request_id: &Value, cancel: &CancelHandle) -> CancelVerdict {
        let Ok(mut g) = self.inner.lock() else {
            return CancelVerdict::NoMatch;
        };
        if g.active.as_ref() == Some(request_id) {
            // Perform the cancellation (cheap atomics only) INSIDE the
            // tracker lock, in the same critical section as the match: the
            // id cannot move between lookup and effect.
            cancel.cancel();
            return CancelVerdict::ActiveMatched;
        }
        if g.queued.remove(request_id) {
            // Bounded by the population invariant: this id moved out of
            // `queued`, so inserting into `tombstoned` cannot grow the
            // tracker past the bound.
            g.tombstoned.insert(request_id.clone());
            return CancelVerdict::Tombstoned;
        }
        if g.tombstoned.contains(request_id) {
            // A cancel delivered twice for the same still-undelivered id:
            // it is THAT request's cancel, never anything else's.
            return CancelVerdict::Tombstoned;
        }
        CancelVerdict::NoMatch
    }

    /// Atomically decide a registered request's fate at dispatch and, when
    /// it proceeds, mark it ACTIVE in the same critical section: a cancel
    /// arriving between dequeue and active-marking can no longer disappear
    /// into a gap — it always sees the id as queued, active, or tombstoned.
    /// `true` means the id was tombstoned (cancelled while queued) — the
    /// request must be refused as `cancelled`; the tombstone stays RESERVED
    /// (the id remains in flight) until the refused dispatch's own
    /// [`TransportGate::complete`] releases it. `false` moves a queued id
    /// to active and lets the request proceed as the in-flight request;
    /// unregistered ids (tests, internal callers) always proceed. Dispatch
    /// NEVER removes state — completion is the single release point.
    pub fn take_dispatch_decision(&self, id: &Value) -> bool {
        let Ok(mut g) = self.inner.lock() else {
            return false;
        };
        if g.queued.remove(id) {
            g.active = Some(id.clone());
            false
        } else {
            g.tombstoned.contains(id)
        }
    }

    /// The ONE completion point for an accepted id, called exactly once by
    /// the owning request on EVERY response/error path (RAII): removes the
    /// id's queued/tombstone membership AND clears the active marker — but
    /// only when the active marker IS this id, so completing one request
    /// can never erase a DIFFERENT in-flight request's state, and a refused
    /// duplicate (which owns no registration and never completes) leaves
    /// everything untouched. Because an id stays reserved from register
    /// until this complete, a late complete can never erase a re-registered
    /// id's fresh state: reuse is refused while the original is in flight.
    pub fn complete(&self, id: &Value) {
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        g.queued.remove(id);
        g.tombstoned.remove(id);
        if g.active.as_ref() == Some(id) {
            g.active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gate() -> TransportGate {
        TransportGate::new()
    }

    fn cancel_handle() -> CancelHandle {
        CancelHandle {
            cancel: Arc::new(AtomicBool::new(false)),
            generation: RequestGeneration::new_for_test(),
        }
    }

    /// queued B cancelled while A is active: B is tombstoned, A untouched,
    /// and the tombstone is reported at dispatch yet stays RESERVED until
    /// that refused dispatch completes the id (exactly once).
    #[test]
    fn queued_cancel_tombstones_only_that_id() {
        let g = gate();
        let cancel = cancel_handle();
        let active = g.active_view();
        active.set(Some(json!(7)));
        assert_eq!(g.register(json!(41)), Admission::Accepted);

        assert_eq!(
            g.cancel_request(&json!(41), &cancel),
            CancelVerdict::Tombstoned
        );
        assert!(active.matches(&json!(7)), "active A must be untouched");
        assert!(
            !cancel.is_cancelled(),
            "a queued-id cancel must NOT set the global flag"
        );
        assert!(
            g.take_dispatch_decision(&json!(41)),
            "cancelled queued id must be refused at dispatch"
        );
        // Reserved, not consumed: a replayed cancel still matches THAT id.
        assert_eq!(
            g.cancel_request(&json!(41), &cancel),
            CancelVerdict::Tombstoned
        );
        // The refused dispatch completes the id: now nothing matches.
        g.complete(&json!(41));
        assert_eq!(
            g.cancel_request(&json!(41), &cancel),
            CancelVerdict::NoMatch
        );
    }

    /// A cancel of the ACTIVE id performs the cancellation inside the gate
    /// (same critical section as the match); a cancel of a QUEUED id leaves
    /// the global flag clear and the generation unchanged. A dispatch racing
    /// the cancel sees the id in exactly one place.
    #[test]
    fn dispatch_and_cancel_are_atomic() {
        let g = gate();
        let cancel = cancel_handle();
        let gen_before = cancel.generation.current();
        assert_eq!(g.register(json!(1)), Admission::Accepted);
        assert!(!g.take_dispatch_decision(&json!(1)));
        // Now active: ONE cancel call both matches and cancels (flag set,
        // generation bumped exactly once) — no caller-side split.
        assert_eq!(
            g.cancel_request(&json!(1), &cancel),
            CancelVerdict::ActiveMatched
        );
        assert!(
            cancel.is_cancelled(),
            "active-id cancel must latch the shared flag"
        );
        assert_eq!(
            cancel.generation.current(),
            gen_before + 1,
            "active-id cancel must bump the generation exactly once"
        );
        g.complete(&json!(1));
        // Completed: nothing matches anymore.
        assert_eq!(g.cancel_request(&json!(1), &cancel), CancelVerdict::NoMatch);

        // A QUEUED-id cancel tombstones only that id: no global flag, no
        // generation bump — unrelated active work keeps running.
        assert_eq!(g.register(json!(2)), Admission::Accepted);
        let flag_before = cancel.is_cancelled();
        let gen_before = cancel.generation.current();
        assert_eq!(
            g.cancel_request(&json!(2), &cancel),
            CancelVerdict::Tombstoned
        );
        assert_eq!(cancel.is_cancelled(), flag_before);
        assert_eq!(cancel.generation.current(), gen_before);
        g.complete(&json!(2));
    }

    /// Duplicate registration (including over a cancelled-still-queued
    /// tombstone) is refused and NEVER erases the original state.
    #[test]
    fn duplicate_register_never_disturbs_original() {
        let g = gate();
        let cancel = cancel_handle();
        assert_eq!(g.register(json!(5)), Admission::Accepted);
        assert_eq!(
            g.cancel_request(&json!(5), &cancel),
            CancelVerdict::Tombstoned
        );
        // Reusing the id BEFORE its cancelled frame was delivered: refused.
        assert_eq!(g.register(json!(5)), Admission::Refused);
        // The original tombstone survives the refused duplicate.
        assert!(g.take_dispatch_decision(&json!(5)));
        // Still reserved until the refused dispatch completes it.
        assert_eq!(g.register(json!(5)), Admission::Refused);
        g.complete(&json!(5));
        // After cleanup the id is free again.
        assert_eq!(g.register(json!(5)), Admission::Accepted);
        g.complete(&json!(5));
    }

    /// Capacity is one bound across the whole population; explicit refusal,
    /// never silent acceptance, and released ids free capacity.
    #[test]
    fn capacity_refusal_is_explicit_and_recoverable() {
        let g = gate();
        for i in 0..MAX_TRACKED_REQUEST_IDS {
            assert_eq!(g.register(json!(i)), Admission::Accepted);
        }
        assert_eq!(
            g.register(json!("overflow")),
            Admission::Refused,
            "population bound must refuse explicitly"
        );
        // Completing one id frees exactly one slot.
        g.complete(&json!(0));
        assert_eq!(g.register(json!("overflow")), Admission::Accepted);
    }

    /// Non-dispatch completions release registrations: 300 sequential
    /// ping-like ids never fill the tracker.
    #[test]
    fn sequential_non_tool_requests_do_not_fill_tracker() {
        let g = gate();
        for i in 0..(MAX_TRACKED_REQUEST_IDS + 64) {
            let id = json!(i);
            assert_eq!(g.register(id.clone()), Admission::Accepted);
            g.complete(&id);
        }
        assert_eq!(g.register(json!("still-room")), Admission::Accepted);
    }

    /// A cancel arriving AFTER completion (id already released) matches
    /// nothing, so the request cannot be cancelled retroactively and id
    /// reuse afterwards starts clean.
    #[test]
    fn delayed_cancel_after_completion_matches_nothing() {
        let g = gate();
        let cancel = cancel_handle();
        assert_eq!(g.register(json!(3)), Admission::Accepted);
        assert!(!g.take_dispatch_decision(&json!(3)));
        g.complete(&json!(3));
        // Completed before the cancel arrived: no state remains, no match.
        assert_eq!(g.cancel_request(&json!(3), &cancel), CancelVerdict::NoMatch);
        // Reuse starts clean.
        assert_eq!(g.register(json!(3)), Admission::Accepted);
        g.complete(&json!(3));
    }

    /// ID-reuse ABA: a dispatched id stays ACTIVE and reserved until its
    /// ONE complete — a duplicate register in between is refused and the
    /// old complete can never erase a fresh registration's state.
    #[test]
    fn active_id_is_reserved_until_complete() {
        let g = gate();
        assert_eq!(g.register(json!(9)), Admission::Accepted);
        assert!(!g.take_dispatch_decision(&json!(9)), "proceeds as active");
        // Reserved through the reply path: reuse is refused while active.
        assert_eq!(g.register(json!(9)), Admission::Refused);
        // Completing a DIFFERENT (untracked) id never clears the active one.
        g.complete(&json!(10));
        assert_eq!(g.register(json!(9)), Admission::Refused);
        // The owning request's complete releases it; only then is reuse ok.
        g.complete(&json!(9));
        assert_eq!(g.register(json!(9)), Admission::Accepted);
        g.complete(&json!(9));
    }

    /// A rejected duplicate owns no registration: completing its id (which
    /// a well-behaved service never does — the refusal path disarms the
    /// RAII release) still must not clear the ORIGINAL's active state.
    #[test]
    fn completing_other_id_never_clears_active() {
        let g = gate();
        assert_eq!(g.register(json!(1)), Admission::Accepted);
        assert!(!g.take_dispatch_decision(&json!(1)));
        assert_eq!(g.register(json!(1)), Admission::Refused);
        // Even a stray complete for the same id clears the active marker
        // ONLY because it matches — completing any other id must not.
        g.complete(&json!(2));
        assert_eq!(g.register(json!(1)), Admission::Refused, "still active");
        g.complete(&json!(1));
        assert_eq!(g.register(json!(1)), Admission::Accepted);
        g.complete(&json!(1));
    }
}
