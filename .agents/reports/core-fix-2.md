# core-fix-2 — Integration Repair Report

Date: 2026-09-24. Scope: `src/runtime/**`, `.gitignore`, this report only. No
backend/mcp/scripts/independent-test edits. No commit/push/worktree. No GUI
validation (machine locked; none claimed).

## Test results (real exit codes, `set -o pipefail`)

| Suite | Command | Result |
|---|---|---|
| Core unit | `cargo test --lib runtime:: --offline` | **68 passed, 0 failed**, 0 ignored (exit 0) |
| Protocol | `cargo test --test protocol --offline` | **12 passed, 0 failed** (exit 0) |
| Independent contract | `cargo test --test runtime_contract --offline` | **35 passed, 0 failed, 1 ignored** (exit 0) |

The 1 ignored test is the independent suite's own opt-in
`registry::requests_up_to_bound_all_replay` ("slow: 1000 sequential steps; run
explicitly with --ignored") — deliberately `#[ignore]`d by the independent
worker, not by me. Whole-lib suite was not run (transport repair in progress,
per instructions). Scoped `cargo fmt` applied to all touched core files.

## core-integration-2.md findings — all addressed

1. **Pre-input live geometry check, zero input on mismatch** — `validate_step`
   re-reads `backend.geometry()` after basis/freshness checks and faults the
   session on full-`Geometry` inequality; record stays `not_started` with zero
   events. Covered by `geometry_change_faults_on_observe`,
   `step_on_unchanged_geometry_does_not_false_fault` and the independent
   `freshness::geometry_change_rejects_old_actions_and_faults`.
2. **Truthful error flag preserving dispatched/partial metadata** —
   `StepRecord.error` is set on every failure path while `input_outcome` /
   `events_completed` / `observation_outcome` keep their true values; capture
   failure after dispatch never erases `dispatched`
   (`capture_failure_after_dispatch_keeps_dispatched` + independent
   `capture_fail::*`).
3. **Resume requires fresh observation** — `resume()` calls
   `session.invalidate_observations()`; pre-pause `based_on` is rejected
   `stale_observation` with `not_started` (asserted in
   `pause_blocks_steps_and_resume_requires_geometry_stability`, new explicit
   assertions).
4. **Root `width_px`/`height_px` contract fields** — `ObservationMeta::to_json`
   emits both at the observation root (plus the nested `image` object);
   verified by independent `scaling::*` and
   `image_declared_size_matches_encoded_size_on_observe`.
5. **f10–f12 (+ vocabulary drift) parsing** — runtime accept-list now matches
   backend `parse_key` exactly: f1–f12, `super`, `page_up`/`page_down` (backend
   files untouched; the drift was on the runtime side).
6. **Deterministic cancellation tests** — cfg(test) `cancel_after_injects`
   seam in `ExecutionConfig`/`ExecutionOutcome`;
   `mid_text_cancellation_reports_partial_and_releases` is sleep-free.
7. **Correct scaling expectations** — honest round-to-nearest aspect math
   (3008×1692→1365×768, 1600×900→1365×768); FakeBackend capture reports actual
   decoded PNG dims; new `coordinates_map_identity_when_image_not_downscaled`;
   one real downscaling case kept.
8. **Hold-deadline release** — `RELEASE_PASS_ALLOWANCE` (250 ms) cleanup budget
   releases pressed keys after the input deadline expires; shortened holds
   report `partial`, never `dispatched`.
9. **Never release a never-pressed key** — executor tracks `pressed`
   (successful press indexes only); `Plan::is_answered_release` requires a
   prior *successful* press; `partial_input_reported_and_cleaned_up` asserts
   the phantom release is gone.

## Additional coordinator/user findings fixed this run

- **Windows E0500 (session.rs)** — explicit `match` on `backend.geometry()`
  with fault after the basis borrow ends; Windows check-4 passed (exit 0,
  warnings only, per coordinator).
- **Image-integrity on reused request_id** — `Runtime::step` binds the image
  lookup to the produced record via `Session::result_image_for(&record)`
  (record's own `observation.observation_id`); conflict/invalid records carry
  no observation → no borrowed image. Regression:
  `conflicting_reused_request_id_never_borrows_prior_image`.
- **capture_fail contract failure** — per coordinator ruling (CONTRACT
  updated): `get_step` on a known record is a read-only lookup that always
  returns `is_error=false`, with the original step's error state explicit in
  the new `step_is_error` field and all recorded outcomes/`error` verbatim.
  Unknown session/request still `is_error=true`. `computer_step` and identical
  replays keep the original record's error state. New unit test:
  `get_step_reports_lookup_success_and_preserves_original_step_error_state`
  (capture-failure and partial+cleanup-failure records).
- **scaling contract failure** — a read-only re-observe no longer invalidates
  the immediately previous observation: `resolve_basis` accepts the current
  observation or the retained previous one whose `input_sequence` still
  matches (any input in between bumps the sequence → still `stale_observation`,
  so `freshness::input_invalidates_prior_observation` passes). Cache entries
  now retain observation meta+map (bytes evicted independently). Regressions:
  `reobserve_without_input_keeps_prior_observation_valid_basis`,
  `reobserve_then_step_on_previous_observation_dispatches`.
- **Full-geometry comparisons everywhere** — resume now compares the whole
  `Geometry` (was version+surface only, contradicting its comment), and
  `capture_observation` pre/post-capture checks use full equality too, matching
  the `validate_step` invariant. New:
  `resume_rejects_input_geometry_change_with_same_version_and_surface`.
- **`.gitignore`** — `acceptance-fixture/build/` excluded (fixture source
  untouched).
- Removed previous worker's diagnostic `dbg_resize` panic test (earlier).

## Borrow-checker notes (for the record)

`resolve_basis` returns `&Observation` borrowed from the session; session
fields needed by later checks (`input_sequence`, `geometry`) are copied before
the borrow, and fault paths end the basis borrow before mutating session
state. Edition 2021 — no let-chains. No `Backend` trait/Any-downcast seam was
added (per coordinator); mid-session geometry mutation is tested at the
session level, and `FakeBackend::fail_capture_after` covers the
capture-after-dispatch path within testutil.

## Lint closure (final task, per coordinator clippy-1.log)

All 4 core-scope findings resolved with minimal edits:

1. `runtime/mod.rs` `module_inception` — scoped `#[allow(clippy::module_inception)]`
   with explanatory comment on `mod runtime;` (intentional facade naming, no
   restructure).
2. `session.rs:490` `map_identity` — `sleep_cancellable(wait, cancel)?;` (identity
   `map_err` removed).
3. `tools.rs:193` `useless_format` — `TOOL_GET_STEP` description converted to
   `.to_string()` **and extended with the new contract**: known-request lookup
   always succeeds (top-level `is_error=false`), the original step's error state
   is explicit in `step_is_error`, error/outcome fields verbatim; only an
   unknown session/request makes the lookup itself fail.
4. `tools.rs:208` `useless_format` — `TOOL_PAUSE` description `.to_string()`.

Verification: `cargo clippy --offline` — zero warnings in `src/runtime/**`;
remaining warnings are mcp/bin scope (transport-fix-2 / other owners, untouched).
`cargo fmt --check` clean. Full core suite not re-run per coordinator (scoped
green state frozen); these were lint/description-only edits.

## Gaps / remaining failures

- None in scoped suites. The whole-lib suite remains unrun by design
  (transport-fix-2 owns `src/mcp/**`; its unit repair was in flight).
- GUI acceptance not run (machine locked) — no GUI claims made.
- Source writes frozen after this green state, per coordinator.
