# Admission fixture R1–R3 GREEN candidate — 2026-10-16

Fixture only, awaiting independent validation. No product no-GO fix, publication
change, caller-resource recovery, consumer migration or overall helper GREEN.

## Evidence and preserved baseline

Read actual `cc-admission-controls-red-1/probe.log`: compile exit0; controls11
had the seven specified ADMISSION_CONTROL_ASSERTION_RED failures and four PASS;
delayed exact1 had a product assertion at startup_budget.rs:83. Its precleanup
sample was release900/ARMED901/observed2500/cutoff2500/reset-earliest2901ms,
exited=false/event=None/held=true/path=(0,0)/product_reaps=0. These are historical
CC results, not author test execution. Runner nested-test line counting/earlier
staging INFRA is not product RED; use actual assertions and nested/outer results.

Base is helper58 `credentials-admission-controls-red-1` (archive SHA256
1a9ba457ed3b44a735d04754a87dbd61b1d4b0f3b96bb7b1fcb4769dbb2c7e47), integrated394.
All eleven existing control test bodies are byte-identical. Original delayed
assertions remain, with one additive release-based window qualification. Original
no_go.rs, creator_reuse.rs and every product/raw/leaf/publication/creator/permit/
expiry regression source are byte-identical. CONTROLS_RED.md remains historical.

## Limited implementation

**R2 wait:** capture monotonic time before and after every predicate. First sample
must start before cutoff; a successful sample must start before and finish no
later than cutoff. Expired/straddling successful observation is explicit fixture
INFRA, not PASS or product RED. Log the first successful sample bounds relative
to cutoff. After at least one in-window negative observation, a final valid
negative sample at/after cutoff may return false; never infer it from a stale
pre-sleep false. This relies on the fixture's monotonic completion/readiness
predicates. In particular an owned pidfd remaining non-readable in a valid later
poll has not demonstrated an earlier terminal exit. A late positive cannot
retroactively establish timely exit. This is not a real-time scheduling promise;
observation uncertainty is permitted to make a native run INFRA/unqualified.

**R2 poll:** syscall errors other than EINTR, ERR/NVAL, unexpected result/event
combinations explicitly panic with ADMISSION_FIXTURE_INFRA. Actual observer
retries EINTR at most four times, then INFRA; the pure decision's retryable false
is NEVER forwarded to wait as a valid negative sample. Valid POLLIN qualifies;
POLLIN|HUP is accepted for an already reaped pidfd. HUP alone is conservatively
INFRA rather than guessed exit. `poll_exited` remains the actual observer's
decision seam, not an independent test model.

**R3 task visibility:** before setting the actual creator's exit request, confirm
its parent barrier is still held, `/proc/self/task` is a queryable directory, and
the actual target task directory is visible. Record parent dev/inode; verify the
same queryable parent after prequalification and before/after later task lookup.
Only target ENOENT under those conditions returns absent. Parent disappearance,
changed identity, permission/I/O errors, and non-directory objects are explicit
INFRA through the real raw_exit path. Pre-exit marker=false still short-circuits.
`TaskVisibility` is fixture path qualification, NOT a stable thread handle,
proc-based production witness, or numeric-TID admission authorization. No child
release/behavioral assertion follows failed raw_exit qualification.

**R1 window:** preserve the existing 2000ms budget/900ms startup hold/500ms grace
and all assertions. Add a check using the pre-release timestamp as a lower bound
on actual ARMED. `reset_earliest_ms` now reports release+full budget, not the
later controller observation+budget. This changes diagnostic meaning explicitly:
the old value2901 is preserved in frozen history; a new normal value may be2900.
`observed_ms` is still the post-wait caller timestamp; the shared wait log supplies
the actual successful sample bounds. No deadlines or observation limits expand.

**N1:** one additional positive control uses owned synthetic directories with
actual filesystem queries through the same TaskVisibility. It establishes target
visibility, verifies still-present is not death, removes only the owned target,
then requires qualified absence to allow the shared death decision. This prevents
reject-all of successful lookups. Not a real creator death/TID-reuse test, and no
separate historical RED claim for this supplementary control.

## Independent CC commands and expected classifications (NOT RUN here)

Compile the immutable integrated source first. Record each native exact case in
its own log/exit status, including nested precleanup observations.

```sh
cargo-1.89 test --locked --offline --lib --no-run

# 12 controls: original11 + N1. Expected12 PASS / exit0.
timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::fixture_controls \
  -- --nocapture --test-threads=1

# Expected product assertion RED / exit101 (no product no-GO change).
timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::no_go_exits_at_original_deadline_without_parent_cancellation \
  -- --exact --ignored --nocapture --test-threads=1

# Expected product assertion RED / exit101 with delayed-start separation.
timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::delayed_start_no_go_keeps_original_deadline \
  -- --exact --ignored --nocapture --test-threads=1

# Real pre-exit /proc qualification path. Expected1 PASS / exit0 if qualified.
timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::original_creator_death_before_prctl_never_opens_authority \
  -- --exact --ignored --nocapture --test-threads=1
```

Seven original controls intentionally catch ADMISSION_FIXTURE_INFRA panics; with
`--nocapture` their panic-hook output is EXPECTED even on successful controls.
Do not classify their log markers alone as run-level INFRA. Native cases do NOT
catch those qualification failures: any such native failure is INFRA, not the
expected product assertion RED. A no-GO PASS is unexpected and must be inspected,
not accepted as a product fix. Zero tests, compile failures and timeout are not
behavior evidence. Do not count dispatcher success as another native proof.

Optional separately scheduled actual-reuse command remains unchanged:

```sh
timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::actual_reused_tid_cannot_supply_original_creator_go \
  -- --exact --ignored --nocapture --test-threads=1
```

No actual same-group TID match remains NOTQUALIFIED (inner77/outer failure), never
PASS or behavior RED. This fixture change neither increases churn nor changes
kernel/security/environment prerequisites. The synthetic N1 control cannot close
that gate. Original high-FD native tests retain per-container nofile8192:8192;
this limited fixture group does not need that sentinel, DISPLAY or GUI.

## Compilation and delivery

Actual-source helper `cargo check --locked --offline --tests --target <target>`
exit0 on x86_64-unknown-linux-gnu and aarch64-unknown-linux-gnu. Logs under
`.agents/runs/linux-parity-20261015/credentials-admission-fixture-green-check-<target>.log`.
No tests/SSH/GUI/commit/push were performed. This is not full Linux root build,
runtime success, or independent source-review approval.

Six-file overlay: admission_native_tests.rs; admission_native_tests/harness.rs,
fixture_controls.rs, startup_budget.rs; new task_visibility.rs and this document.
Apply only over frozen helper58/integrated394. Supplied overlay SHA256 manifest
and complete resulting helper60 manifest distinguish changed vs preserved files.

The reaper may attach published attempts and observe/reap; it still cannot send
GO or revoke IPC on this frozen product. Held creator cannot cancel. Preserve
the exit127-vs-EOF cause qualification in ADMISSION_RED.md. Actual clone3-success
evidence, ARM64 runtime, creator resource/permit retirement, whole-host death and
overall helper acceptance remain open. Fixture GREEN does not authorize no-GO
implementation; await independent results and coordinator's next decision.
