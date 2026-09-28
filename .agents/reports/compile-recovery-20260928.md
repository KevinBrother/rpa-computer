# Compile Recovery Report — 2026-09-28

## Scope
Recover compilation of `src/mcp/worker.rs` after the previous writer stopped
mid-integration on a 429. Fix only constructors / call wiring; no redesign.
Source of truth: `.agents/runs/coordinator-resume-check-20260928.log` (7 errors).

## Changes made

### `src/mcp/worker.rs`
1. **`Worker::start` (~L483)**: build the `TransportGate` first, then construct
   `ActiveRequest` with `inner: Arc::clone(&transport_gate.inner)` so both are
   views of the SAME `Arc<Mutex<RequestTrackerInner>>`. Previously created a
   separate `current` field and an unrelated gate.
2. **`Worker::active_request` (~L523)**: now returns `self.active.clone()`
   (the old `current` field no longer exists).
3. **`coordinator_main` (~L892)**: added `native_generation = generation.clone()`
   and passed it as the 7th argument to `native::native_main(...)`; the
   coordinator's `generation` remains usable after the move closure.
4. **WorkItem initializers** (struct gained `gen: u64`):
   - `__lifetime_close` (~L1057): `gen: native::CONTROL_GEN`
   - `__shutdown` (~L1151): `gen: native::CONTROL_GEN`
   - `dispatch_command` (~L1225): `gen` from the destructured `Command::Call`
     (accepted generation, not re-stamped).

### `tests/transport_lifecycle.rs`
- Removed two `unused_mut` only: `let mut worker` (L88) → `let worker`, and
  `let (mut worker, resume_reply)` (L116) → `let (worker, resume_reply)`.

## Verification (exact exit codes)

- `cargo check --all-targets --offline` → **exit 0** (lib, lib test, all
  targets compile). One pre-existing dead-code warning in
  `src/mcp/stdio/tests.rs` (`read_line_bounded`), not touched per scope.
- `cargo test --test transport_lifecycle --offline` → the harness itself
  exited 0 (build ok), but the single test
  `resume_cancelled_inside_geometry_keeps_cancel_flag` **FAILED**:

  ```
  assertion `left == right` failed: successful resume must require fresh
  observation: Object {"note": "session was not paused", "session_id": ...,
  "state": "ready"}
    left: Null
   right: Bool(true)
  ```
  at `tests/transport_lifecycle.rs:190`. This is a behavioral failure in the
  resume-after-cancel flow (post-cancel resume returned a non-error "ready"
  reply instead of the expected error/null), NOT a compilation issue. Left
  for the next checkpoint per instructions.

## Constraints honored
- No commits/push, no worktrees, no GUI/remote, no config/auth changes.
- Touched only the three permitted files (this report being the third).
- No cargo fmt, no whole-suite run.
