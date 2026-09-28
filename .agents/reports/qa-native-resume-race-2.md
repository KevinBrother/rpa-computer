# QA Report: Resume Cancel Race (Native) — Run 2

## Command

```
cargo test --test transport_lifecycle --offline -- --nocapture
```

## Result: RED (failed deterministically, as expected)

```
running 1 test
thread 'resume_cancelled_inside_geometry_keeps_cancel_flag' panicked at tests/transport_lifecycle.rs:143:9:
cancel flag was cleared after a resume cancelled inside geometry()
test resume_cancelled_inside_geometry_keeps_cancel_flag ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; finished in 0.23s
```

Exit code: 101 (test failure, not a compile error). Two harmless `unused_mut`
warnings in the test file; no other diagnostics.

## What the test proves (evidence of the bug)

`tests/transport_lifecycle.rs` drives a `Worker` with a mock `Backend` whose
`geometry()` blocks behind an armed gate (bounded 10 s safety, released on
scope exit / Drop so a panic can never hang the implicit scoped-thread join):

1. `computer_open` + `computer_pause` succeed (setup asserted).
2. A scoped thread calls `computer_resume`; the main thread waits (bounded,
   3 s) until the backend is provably inside `geometry()`, then calls
   `cancel_handle.cancel()` and only then releases the gate and joins.
3. **Failing assertion (line 143):** after the cancelled resume completes,
   `cancel_handle.is_cancelled()` must still be `true`. On the current source
   it is `false` — `Runtime.resume` clears the cancel flag *after* the
   `geometry()` call, clobbering a cancellation that arrived while the backend
   was blocked. This reproduces deterministically (0.23 s, no flakiness, no
   OS capture calls — the mock panics on `capture`/`inject`, and none fired).

Subsequent assertions (not reached due to the first failure, per spec — no
debugging/fixing of `src/` was performed) additionally pin the expected
behavior:

- the resume reply must be `is_error` with `error.code == "cancelled"` (never
  a clean `ready`),
- a following `computer_observe` must report `cancelled` (never
  `capture_error`),
- a fresh deliberate `computer_resume` (gate disarmed) must succeed on the
  first call with `requires_fresh_observation == true`.

## Conclusion

The resume cancel race is confirmed: a cancellation delivered while
`Runtime.resume` is blocked inside backend `geometry()` is silently cleared,
so the worker loses the cancelled state. No source files were modified; only
`tests/transport_lifecycle.rs` was added.
