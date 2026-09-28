# Native cancel checkpoint — 2026-09-28

Writer: native cancel (small scope). Owned files only: `src/mcp/worker/native.rs`,
`src/runtime/runtime.rs`, `src/runtime/runtime/tests.rs`, `tests/transport_lifecycle.rs`.
No GUI/remote/commit/config/model changes. No global formatter run.

## Requirements & fixes

### 1. Resume recovery (Ready + latched stop)
Root cause: `Runtime.resume` had an unconditional `SessionState::Ready` early
return (`"session was not paused"`), even when the cancel flag was latched
(a stop that raced a resume inside `geometry()` and was re-latched by the
native epoch guard). That reply missed `requires_fresh_observation` and left
the stop flag latched, so a later deliberate resume never produced a clean
first-call success.

Fix (`src/runtime/runtime.rs`): the Ready no-op is kept ONLY when no stop is
pending (`resume_was_deliberate`). Ready + latched stop now runs the full
resume path: backend validation (geometry → fault on error/change, preserved
verbatim), `cancel.store(false)`, `invalidate_observations()`, state stays
Ready, reply carries `requires_fresh_observation: true`. The native epoch
guard re-latches any stop that lands DURING this resume (covered by the
existing post-call guard; the flag clear stays inside the runtime).
`CancelHandle::cancel` verified: it bumps the generation BEFORE latching the
flag (`src/mcp/worker.rs:200-209`) — no change needed.

Also added a PRE-native epoch check for `computer_open`/`computer_resume`
(`src/mcp/worker/native.rs`): a stop that lands in the dispatch→native gap
(stamp already stale when the native thread picks the item up) is refused
with the exact `cancelled` error BEFORE the runtime clears the flag — no
backend work, no flag clear, no internal session creation. Control work
(`CONTROL_GEN`) always passes through.

### 2. Observe interrupted inside `backend.capture()`
`enforce_stop_epoch` now also replaces a CLEAN `computer_observe` reply whose
epoch went stale (stop landed inside capture) with the exact `cancelled`
error — never success, never `capture_error`. Error replies are preserved
verbatim. `computer_step` outcomes (partial/dispatched/unknown) are NEVER
rewritten: only the flag is re-latched. The deterministic test uses a gated
mock backend returning a VALID 16x16 PNG, cancels while blocked inside
capture, releases, and asserts: exact `cancelled` + a subsequent deliberate
resume succeeds on the first call (`requires_fresh_observation: true`).

### 3. Open interrupted inside geometry (internal session rollback)
An open cancelled while blocked inside `geometry()` previously substituted
`cancelled` but skipped the watchdog clock, leaking an invisible live session
(the next open got `session_state`). Fix (`src/mcp/worker/native.rs`): when
the epoch guard converts a `computer_open` success into `cancelled`, the
native thread now rolls the session back with an honest
`runtime.shutdown()`:
- cleanup succeeds → session closed, clock NOT armed (final reply is the
  `cancelled` error; `track_session_clock` only arms on verified success), a
  fresh explicit open succeeds on the FIRST call;
- cleanup fails → the reply reports the failure (`cleanup_error` with the
  backend message) instead of a bare `cancelled`, the clock IS armed (state
  uncertain → the possibly-live session stays lifetime-bounded), and the
  runtime's own faulted state is preserved.
Opens that FAILED validation/`session_state` never reach the rollback
(`reply.is_error` guard), so no pre-existing session is shut down. The
cleanup failure is never blindly replaced by the `cancelled` reply.

## Deterministic regressions (tests/transport_lifecycle.rs, mock backend only,
no native GUI; gated via `BackendFactory::Test`)

- `resume_cancelled_inside_geometry_keeps_cancel_flag` (pre-existing, was RED,
  now GREEN without assertion changes): gate blocks inside `geometry()`,
  cancel mid-block → exact `cancelled`, flag stays latched, observe →
  `cancelled` (not `capture_error`), deliberate resume → first-call success
  with `requires_fresh_observation`.
- `observe_cancelled_inside_capture_reports_cancelled_not_capture_error`
  (new): valid-PNG mock, cancel inside `capture()` → exact `cancelled`,
  flag latched, fresh resume succeeds first try.
- `open_cancelled_inside_geometry_rolls_back_new_session` (new): cancel
  inside open's `geometry()` → exact `cancelled`; fresh open succeeds first
  try (no hidden `session_state`) and the new session is fully functional
  (observe ok).

Runtime unit regressions (`src/runtime/runtime/tests.rs`):
- `ready_resume_with_latched_stop_recovers_ready_cleanly`: Ready + latched
  stop → deliberate resume succeeds, `requires_fresh_observation`, no
  "session was not paused" note, flag cleared, old basis now
  `stale_observation`.
- `ready_resume_without_stop_stays_clean_noop`: Ready + no stop → no-op
  note preserved, observation basis still grounds input.
- `closed_session_resume_with_latched_stop_still_reports_not_found`:
  terminal-state errors unchanged with a latched stop.

## Commands & actual exit codes

- `cargo test --test transport_lifecycle` BEFORE fix: 1 failed
  (`resume_cancelled_inside_geometry_keeps_cancel_flag`, "successful resume
  must require fresh observation" — matches
  `.agents/runs/coordinator-native-race-20260928.log`).
- After adding the two new tests, BEFORE fix: 3 failed
  (open-cancel → fresh open hit `session_state`; observe-cancel → returned a
  successful observation instead of `cancelled`).
- `cargo test --test transport_lifecycle` AFTER fix: **3 passed, 0 failed**
  (exit 0), 0.33s.
- `cargo test --lib runtime::` AFTER fix: **71 passed, 0 failed** (exit 0).
- `cargo test --lib mcp::worker`: 12 passed, 0 failed.
- `cargo check`: exit 0, 0 warnings.
- `cargo test --lib` (full, 166+3+1): 3 failures in `mcp::stdio::tests`
  (`direct_cancel_gate_semantics`, `stdio_oversized_overflow_uses_same_one_shot_latch`,
  `stdio_spoofed_and_escaped_ids_never_cancel`). Verified against BASELINE
  (my owned files temporarily reverted): 4 stdio tests fail there —
  **pre-existing, NOT caused by this change** (with the fix, one previously
  failing baseline test `stdio_active_cancel_matches_only_in_flight_id`
  passes; stdio files untouched). Left for the later stdio stage per scope.

Note: no git commits exist in this repo yet (untracked tree), so no commit
was made. Scope frozen: further tracker/stdio work deliberately deferred.
