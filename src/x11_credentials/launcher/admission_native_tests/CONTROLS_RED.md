# Admission fixture controls — RED-only checkpoint, 2026-10-16

Base: `credentials-publication-green-2` helper55 / coordinator publication391.
Review: `.agents/reports/2026-10-16-linux-credentials-admission-red-review-1.md`.
This is an additive fixture checkpoint, NOT a fixture fix, product no-GO fix, or
new publication candidate. Keep the original admission388/publication391 archives
and their independent logs immutable. No consumer migration.

Read-only inspection of laneA `cc-credentials-admission-red-1/probe.log` confirms
the original no-GO assertion failure, pre-prctl PASS, and actual TID reuse absent
after4096 attempts (NOTQUALIFIED, not PASS or behavior RED). The review limits the
qualification of those fixtures; this checkpoint does not rewrite their history.
No new tests were executed by the author.

## Exact delta

Changed:
- `launcher/admission_native_tests.rs`: register two new modules, one dispatcher
  selector, and one ignored real-launcher test. Existing tests/assertions intact.
- `launcher/admission_native_tests/harness.rs`: extract `poll_exited` and
  `creator_gone` into small shared decisions used by the ACTUAL fixture observer
  and creator-exit path. Capture poll result/errno; use `try_exists` then preserve
  the old error-to-false collapse. Both defects are intentionally retained.
  The original predicate-before-deadline `wait` is unchanged.

Added:
- `launcher/admission_native_tests/fixture_controls.rs`
- `launcher/admission_native_tests/startup_budget.rs`
- `launcher/admission_native_tests/CONTROLS_RED.md` (this document)

All production, raw assembly (both ABIs), child hooks, publication supplements,
original creator/permit/expiry/policy/tracker/auth tests remain byte-identical.
Original `no_go.rs` and `creator_reuse.rs` remain byte-identical too. This is NOT
a thread-handle/provider expansion. No fixture error handling has been repaired.

## New controls and exact EXPECTED results (not measured)

`fixture_controls` has11 tests: expected **7 assertion FAIL +4 PASS**.
The seven failures must have `ADMISSION_CONTROL_ASSERTION_RED`, not a setup or
compiler error. Each requires an `ADMISSION_FIXTURE_INFRA` classification; neither
an ordinary true/false return nor an unrelated panic satisfies that requirement.

| Control | Expected current result / reason |
|---|---|
| `expired_observation_with_true_predicate_is_not_timely_exit` | FAIL: expired first sample returns true; no earlier evidence exists |
| `poll_syscall_error_is_infra_not_child_still_running` | FAIL: EIO/-1 returns false |
| `poll_nval_is_infra_not_child_still_running` | FAIL: NVAL returns false |
| `poll_error_event_is_infra_not_child_still_running` | FAIL: POLLERR returns false |
| `poll_nval_with_readiness_cannot_qualify_exit` | FAIL: NVAL plus POLLIN returns true |
| `proc_permission_error_cannot_qualify_creator_death` | FAIL: EACCES after pre-exit marker returns true |
| `proc_io_error_cannot_qualify_creator_death` | FAIL: EIO after pre-exit marker returns true |
| `timely_true_predicate_remains_observable` | PASS: ordinary timely observation remains usable |
| `valid_poll_readiness_and_not_ready_are_distinct` | PASS: valid readiness, timeout and retryable EINTR stay distinct |
| `visible_task_or_missing_preexit_marker_does_not_qualify_death` | PASS: live task / missing marker cannot prove death |
| `delayed_start_schedule_excludes_a_fresh_full_budget` | PASS: real case's constants separate original cutoff and reset budget |

These are deterministic tests of shared fixture decisions, not real creator/TID
reuse, kernel poll-error injection or launcher containment proof. The real call
sites consume the same decisions; there is no parallel mock oracle.

### R1 new native case

`delayed_start_no_go_keeps_original_deadline` uses the existing real raw child
`BeforePrctl` barrier and actual original creator `BeforePermissionWrite` barrier.
Original budget=2000ms; delay startup until at least start+900ms; original
observation cutoff=start+2500ms (500ms grace). Require at least400ms remaining
before releasing child and confirm original creator reaches ARMED before expiry.
ARMED+full2000ms is later than cutoff by more than300ms. This deterministically
separates a reset-full-budget counterexample without changing product code.

Expected current **1 assertion FAIL**, with `exited=false`, no termination event,
parent still held, zero path operations. Readiness/cause, held state, path counts,
product reaps and elapsed release/ARMED/observation/cutoff/reset-earliest times
are recorded BEFORE parent release or fixture cleanup. An unexpected launch or
barrier/setup failure is INFRA, not the expected assertion RED.

The legacy R2 wait/poll decisions intentionally still apply. Therefore a native
PASS does NOT yet qualify original-deadline correctness. The pure schedule test
only proves the chosen windows separate; it does not prove timely observation.
The raw no-GO read still blocks; no product timeout implementation is included.

## Independent CC commands

Run from the immutable integrated source root, separate logs and exit status.
Never treat zero matches, build failure, host timeout or INFRA as assertion RED.
The controls do not need DISPLAY, GUI, credentials or the high-FD resource setup.
Keep per-container nofile8192:8192 for any original high-FD regression reruns.

```sh
cargo-1.89 test --locked --offline --lib --no-run

timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::fixture_controls \
  -- --nocapture --test-threads=1

timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::delayed_start_no_go_keeps_original_deadline \
  -- --exact --ignored --nocapture --test-threads=1
```

Do not run the entire admission module with `--ignored` and report an aggregate
as proof: real TID reuse can remain NOTQUALIFIED. Keep each original native test
separate using `../ADMISSION_RED.md`. Original three creator ownership assertions
and all old regressions remain unchanged; publication391 laneB stays independent.

## Compilation and gates left open

Actual-source helper `cargo check --locked --offline --tests` exit0 for
`x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`. Logs:
`.agents/runs/linux-parity-20261015/credentials-admission-controls-red-check-<target>.log`.
This is compilation only, not tests, full integrated Linux build, or runtime
qualification. Initial shell status-print attempt hit zsh's read-only `status`
name after compile; commands rerun with a different variable and both statuses
captured. No tests/SSH/GUI/commit/push were executed.

Wait for independent control assertion RED before changing fixture semantics or
product no-GO. Future fixture GREEN must still establish pre-exit task visibility,
distinguish genuine target absence from unavailable parent/proc lookup, and handle
deadline sampling/error attribution; those are NOT solved by extracting seams.
The no-GO exit127/EOF cause boundary in ADMISSION_RED.md remains in force. Actual
TID reuse, creator resource recovery, whole-host death, both-ABI native evidence
and overall helper acceptance remain open. No change to supported kernels or
security policy, no model substituted for actual TID reuse.
