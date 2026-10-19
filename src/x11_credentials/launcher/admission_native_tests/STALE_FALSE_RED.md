# G1 stale negative observation — single-control RED checkpoint

2026-10-16. Based on frozen helper60 / integrated396, fixture-green-1 overlay
SHA256 c44f923b9197ec8fd0d40659f0687222dbf1d290c939b9400b45f08416c5aa52.
Review: 2026-10-16-linux-credentials-admission-fixture-green-review-1.md (G1).
This is a TEST addition, not a fixture or product fix. Keep frozen396 unchanged;
its GREEN filename is not an acceptance claim. No new runtime result exists here.

## Only delta

- Append one test to `fixture_controls.rs`. All previous12 tests and helpers
  remain byte-identical; no existing assertion is edited.
- Add this document. No registration or seam edit is necessary.

`harness::wait` retains the reviewed faulty false/after>=cutoff return, including
its misleading final-negative comment/log. Original native no-GO/delayed/creator,
permit/expiry, raw leaves, publication and all other production code are unchanged.

## One bounded control against ACTUAL wait

`in_window_false_returned_after_cutoff_cannot_establish_timeout` directly calls
the shared `harness::wait`. One thread performs the following deterministic order
inside its first predicate invocation:

1. Save the current false state and record the sample time.
2. Change that same monotonic state to true and record the transition time.
3. Sleep once until cutoff+10ms, then return the OLD saved false value.

The window is200ms. Any later fresh predicate invocation sees true, never the
saved value. No races, thread churn, environment perturbations, syscalls faked as
kernel evidence, or entire kernel/clock models. This is a test of the fixture's
generic observation contract, not an actual child/poll scheduling run.

After catching the wait result, and OUTSIDE that catch, require both sample and
transition strictly before cutoff and the stale return at/after cutoff. Missing
first invocation or delayed setup is `ADMISSION_CONTROL_SETUP_INFRA`, NOT PASS or
the intended assertion RED. This prevents an already-expired-start INFRA panic
from accidentally satisfying the control. The deliberate sleep may overschedule;
that cannot change the qualified before/after order or create timely evidence.

Once qualified, ordinary false is the G1 error; ordinary true would also invent
timely evidence. Require `ADMISSION_FIXTURE_INFRA`, whether immediately for a
straddling negative or after a deliberate fresh postcutoff sample sees true.
An unrelated panic fails rather than satisfying the control. No predicate is
polled in an unbounded fixture loop.

Print `ADMISSION_STALE_FALSE_CONTROL` with call count and sample/transition/
return/cutoff microseconds. On the CURRENT faulty wait, expected count1, sample
and transition<200000us, return>=200000us, ordinary false followed by exactly
the new `ADMISSION_CONTROL_ASSERTION_RED` assertion. This is an expected result,
not independently observed evidence.

## Exact independent CC command

Apply the two-file overlay only on the frozen396 candidate; do not modify its
historical archive. Compile if needed, then run ONLY this exact control separately:

```sh
cargo-1.89 test --locked --offline --lib --no-run

timeout 30 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::fixture_controls::in_window_false_returned_after_cutoff_cannot_establish_timeout \
  -- --exact --nocapture --test-threads=1
```

Expected **1 test / 1 assertion FAIL / exit101**. Preserve the full observation
and failure message. Zero matches, compile failure, timeout, setup-INFRA, or an
unrelated panic are not the requested RED. An unexpected PASS must be inspected.
Do not infer complete fixture GREEN from this run or execute native cases under
this checkpoint's authorization. Previous12/native assertions remain available
unchanged for separately coordinated later validation.

## Later options — NOT implemented

After independent RED, either classify a false sample whose bracket straddles
cutoff as INFRA, or make one explicit bounded fresh sample starting at/after
cutoff. Only a valid fresh negative can establish timeout; a fresh positive is
still INFRA rather than proof of a timely exit. Do not enlarge the deadline or
relax the existing assertions. Coordinator decides the next implementation gate.

## Compilation only / freeze

Actual-source helper `cargo check --locked --offline --tests` exit0 on both
x86_64-unknown-linux-gnu and aarch64-unknown-linux-gnu. Logs:
`.agents/runs/linux-parity-20261015/credentials-admission-stale-false-red-check-<target>.log`.
No tests/SSH/GUI/commit/push, product no-GO modification, or consumer migration.
Two-file overlay plus complete resulting helper61 manifest provided separately.
Wait for coordinator CC evidence before any fixture repair.
