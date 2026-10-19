# R1/R2 GREEN implementation candidate — 2026-10-16

## Approved bounded plan

Independent CC84952 observed actual assertion RED for all 15 defect cases (R1 eight,
R2 seven); all 13 controls and unchanged original79 passed, and that RED snapshot's
actual Linux Rust1.89 executable linked without execution. Evidence:
`.agents/reports/2026-10-16-linux-renderer-process-eof-red-1.md` and
`.agents/runs/linux-parity-20261015/cc-renderer-process-eof-red-1/remote-logs/`.

Implement only the approved process/EOF repair, keeping original28+79 tests unchanged:

1. Make the 200ms worker drain deadline with pending frames a fault. Keep the queue receipt,
   300ms bounded main wait and fail-closed unknown/noncompletion outcomes; never join forever.
2. Gate connection and every native startup boundary on input status. Retain at most one
   highest-sequence pending snapshot for the first normal UI turn. Input/transport failure
   dominates cancellation. Clean cancellation destroys partial owned resources exactly once
   and never emits Ready or a spurious error.
3. Close the EOF-versus-Ready enqueue race under the actual pipe mailbox lock. Do not infer
   clean EOF from a generic enqueue failure or erase an output fault.
4. Compile/check only, compare original files and freeze separately for independent107
   regressions plus explicitly listed additional boundary regressions. No test/GUI/SSH run.
5. Supply an owned-private-Xvfb fixture plan for a later authorized real UI qualification,
   not a substitute for or permission to change capture exclusion/provider/Host admission.

No new dependencies, global/root edits, compositor/provider work or Host auto-enablement.
Capture exclusion remains unsupported; this is not product parity or UI acceptance.

## Implemented changes and explicit limits

- Worker closing deadline with any retained frame now poisons transport and sets fault before
  done acknowledgment. Process exit0 requires ParentEof AND done AND !fault AND known empty
  queue. Unknown/contended queue or an unfinished worker fails, without extending the300ms
  wait or joining. Core Outbox remains unchanged; dropped-on-error bytes cannot mask fault.
- Connection and all Runtime native startup boundaries inspect input. EOF immediately stops
  later native/Ready/error work and cleans partial ownership once; malformed/truncated input
  and real transport failure remain failure. Native operations already entered cannot be
  preempted by these checks: their original startup watchdog still applies. Cancellation is
  checked when they return, including error returns; no later native work is needed then.
- `startup_input.rs` retains one highest-sequence snapshot (not an extra FIFO) across startup
  checks. First normal turn takes the latest of that snapshot and live input; EOF/failure
  supersedes it. Normal Ready remains after checked painting/presentation.
- `ReadyEnqueue` distinguishes clean input EOF from an enqueue fault. Concrete Pipes commits
  Ready under the SAME mailbox lock as worker reads; PipeBridge returns ParentEof only for
  explicit clean EOF, and failure has priority. Generic Transport's default never guesses
  that an arbitrary enqueue failure was cancellation. No queue is scrubbed to fabricate EOF.
- Runtime/transport cleanup runs before the process completion wait. Existing watchdog
  coverage remains; the1s explicit shutdown arm is not retroactively wrapped around earlier
  cleanup. No scheduling, OS delivery, scan-out, native lifecycle or real-time bound is claimed.

Original28+79 tests/support are unchanged. Added only
`tests/native_ui_startup_races.rs` with **10** separate guards: pending latest snapshots,
first-turn EOF precedence, EOF at Ready commit, EOF during failed native/connection calls,
transport failure precedence, finish-without-EOF, overflow and empty-queue-after-output-fault.
These additional10 have not been run or independently observed RED/GREEN here.

## Verification and exact independent commands

Local compile-only (macOS Rust1.98.1):

- `testing/process-eof-green-check.log`: `cargo check --offline --locked
  --no-default-features --tests`, rc0.
- `testing/process-eof-green-native-check.log`: `cargo check --offline --locked
  --target x86_64-unknown-linux-gnu --features native-ui --all-targets`, rc0 with command-local
  DOCS_RS=1. This skips native probes/link flags: **not actual native linking** or MSRV evidence.
- Both use unchanged vendor with command-local replacement and owned package target/Cargo-home;
  no package/dependency/global changes. `testing/process-eof-green-integrity.log` records the
  byte comparison against the independent EOF-RED frozen source and shared protocol subtree.

Current freeze: `testing/process-eof-green.sha256` (repo-relative complete file list/hashes).
Keep historical RED/GREEN manifests and archives untouched. They describe their own snapshots,
not the current mutable tree. No tests, binary, GUI, SSH or Docker execution in this repair.

CC commands, each independently bounded/logged, using the existing vendor configuration:

```sh
# Unchanged observed-RED28 plus unchanged original79 =107; includes codec_contract3.
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test native_ui_process_completion --test native_ui_startup_eof \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport \
  --test codec_contract --test geometry --test ipc_input --test ipc_output --test model \
  -- --nocapture --test-threads=1

# Additional10, reported separately from the frozen107.
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --test native_ui_startup_races -- --nocapture --test-threads=1

# Link THIS snapshot's actual executable, in approved derived builder, NO DOCS_RS.
cargo-1.89 build --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --bin rpa-desktop-feedback-linux
```

Expected117 passes is **not** an observed result. Prior snapshot links do not qualify this
changed binary. The next independent live phase must use actual native windows/IPC/input;
minimal ownership, cases and evidence are specified in `testing/NATIVE-UI-FIXTURE-PLAN.md`.
That fixture is a plan only, for separate implementation/run after this handoff; no waiting
on it or on an exclusion provider is needed to validate this repair's regressions/link.
Capture exclusion stays unsupported; Host remains disabled; genuine clean-frame parity pending.
