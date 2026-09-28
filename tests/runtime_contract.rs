//! Independent white-box fault-injection tests for the Computer Runtime.
//!
//! These tests are written ONLY against `.agents/CONTRACT.md` — not against
//! any particular implementation. They drive `Runtime::call` with
//! deterministic mock `Backend` implementations and assert the contractual
//! guarantees. The suite is split by topic under `tests/contract/`:
//!
//!   * `validation`   — complete action validation BEFORE any backend event
//!   * `scaling`      — image→native coordinate scaling from true capture dims
//!   * `freshness`    — stale based_on/input_sequence rejection, geometry faults
//!   * `dedup`        — request_id dedup after side effects, conflict errors
//!   * `cleanup`      — cleanup failure honesty, release on all failure paths,
//!     explicit idempotent close/shutdown state
//!   * `partial`      — partial dispatch distinguishable from not_started
//!   * `capture_fail` — screenshot failure after dispatched input
//!   * `cancel`       — cancellation stops input and triggers cleanup
//!   * `registry`     — bounded request registry, no eviction of live records
//!     (slow; run with --ignored, uses a tiny fixture)
//!   * `semantics`    — event-shape semantics (chords/scroll/hold/text) and
//!     misc protocol hygiene
//!
//! These tests use mocks only. A passing mock-backend test does NOT prove
//! anything about real screenshots or real desktop input.

#[path = "contract/support.rs"]
mod support;

#[path = "contract/cancel.rs"]
mod cancel;
#[path = "contract/capture_fail.rs"]
mod capture_fail;
#[path = "contract/cleanup.rs"]
mod cleanup;
#[path = "contract/dedup.rs"]
mod dedup;
#[path = "contract/freshness.rs"]
mod freshness;
#[path = "contract/partial.rs"]
mod partial;
#[path = "contract/registry.rs"]
mod registry;
#[path = "contract/scaling.rs"]
mod scaling;
#[path = "contract/semantics.rs"]
mod semantics;
#[path = "contract/validation.rs"]
mod validation;
