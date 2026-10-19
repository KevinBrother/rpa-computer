# R1/R2 process/EOF regression RED checkpoint — 2026-10-16

## Authorized slice / plan

This is a **new RED candidate**, not a repair or a replacement for the queued native GREEN
archive `827ab712d19e98390ace86e24d94272221c85b18b92ad1ef1a1efe65132a36e8`.
No tests, executable, SSH, GUI or Docker run is authorized here. CC must observe assertion
RED before behavior changes. Original 43+36 tests/support, core source, manifest and lock
stay unchanged. All new artifacts are under `desktop-feedback/linux/`.

1. Add new regressions first, separately from the frozen suites.
2. Extract production-called worker turn/completion and connect/start boundaries without
   repairing their decisions. Native/runtime/codec behavior is otherwise unchanged.
3. Compile only; inspect the diff against the old source and freeze new files separately.
4. Hand off the new assertion suite and unchanged suites for independent execution. Stop
   before GREEN implementation, regardless of source confidence.

## Design / proof boundaries

- R1 uses the actual bounded bridge/Outbox and extracted worker turn with deterministic time
  and partial/EAGAIN/EPIPE writers. The final process classification consumes a completion
  receipt (done, fault and remaining/unknown queue), but intentionally retains its old
  exit-reason-only decision. The RAII worker exit flag and completion sampling are shared
  with production, not a test-only replacement policy. Assertions demand failure for
  fault, timeout, non-completion and unknown drain; bounded happy drain remains clean.
- R2 calls the production connect/start orchestrator and Runtime against a real bridge with
  an observable native adapter. It injects clean EOF before connect and after each startup
  stage, requires a distinct clean closed outcome, no later native startup calls or Ready,
  and allows destruction of partial owned resources. Malformed/truncated input and output
  failures must remain failures. Already entered synchronous native calls cannot be
  preempted by these unit tests; the existing watchdog/live-stall qualification is separate.
- The startup outcome has a closed variant for the required cancellation contract. This
  extraction deliberately does **not** yet produce it: current startup still connects,
  creates UI and mistakes clean EOF for failure. The new process branch for that variant
  cannot change behavior until the separately authorized repair.
- These tests do not execute OS worker threads, launch X, establish scheduling bounds,
  prove successful kernel/parent delivery, or qualify live UI/capture exclusion.

Capture exclusion remains `unsupported`; no Host enablement or compositor/provider change.
Full genuine clean-frame capture remains an independent mandatory unresolved gate.

## Implemented seam mapping / behavior retention

Existing files changed only:

- `src/native_ui/mod.rs`: exports the portable `process_boundary` module.
- `src/native_ui/process.rs`: delegates worker turn/RAII completion, connect/start, and
  final bounded wait/exit classification to that module. OS descriptor handling, worker
  sleep cadence, lock budget, watchdog, UI loop and native adapter are unchanged.

New production file: `src/native_ui/process_boundary.rs`.

- `PipeState::turn` is called by the real OS worker. Tests use its real Outbox with an
  authorized-session Stop, partial writes and EAGAIN/EPIPE, not an alternative shutdown
  model. `WorkerExit` is the same RAII done/fault path used by the worker.
- `Completion::snapshot` samples done/fault/queue without blocking (try_lock failure is
  unknown, not empty). The new observation does not change the old result: `process_exit_code`
  still ignores this receipt. `finish_process` is main's existing 300ms wait, with a clock
  and pause callback; the unfinished-worker regression drives this exact wait to its bound.
- `start_renderer` contains the original connect/error-drain/start ordering and is called
  by the real binary path. Its test native boundary injects input into a **real** bridge.
  Runtime/lifecycle is byte-for-byte unchanged. The Closed outcome is deliberately unreachable
  until the R2 fix; no test-only branch or `unimplemented!` manufactures the assertion failures.

New tests (28 total):

- `tests/native_ui_process_completion.rs` — 12: queued and partial Stop at 200ms, EPIPE,
  unacknowledged/unfinished/lost worker, unknown queue receipt, plus normal drain,
  pre-deadline continuation, preserved UI failure and partial-then-success controls.
- `tests/native_ui_startup_eof.rs` — 16: seven cancellation points (before connect; after
  connect/facts/create/configure/paint/present), six malformed/truncated/output failure
  controls, normal unsupported Ready ordering, pending snapshot preservation and native
  connect failure/error-drain callback.

**Expected by source inspection, NOT observed:** R1 eight failing assertions + four controls;
R2 seven failing assertions + nine controls. CC should independently classify all outcomes;
an infrastructure/compile error is not assertion RED. Neither these counts nor compile
success establishes a test pass or RED. Do not change assertions just to obtain these counts.

## Compile-only evidence for this new candidate

Local toolchain: macOS `rustc 1.98.1 (48a229cea 2026-09-01)`, Cargo1.98.1.

- `testing/process-eof-red-check.log`: no-default-features **all test targets** check, rc0.
- `testing/process-eof-red-native-check.log`: Linux-target native-feature all-targets check,
  rc0 with command-local `DOCS_RS=1`; **not native linking** or Rust1.89 evidence.
- `testing/process-eof-red-integrity.log`: comparison against the immutable GREEN source.
  Only the two existing files listed above changed. Original 43+36 tests/support, core,
  lifecycle/paint/transport/XCB adapter, manifest, lock and shared protocol stayed identical.
- Formatting check rc0 on the five new/modified Rust files.

Initial default offline resolution failed (`cairo-rs` unavailable in configured rsproxy
index). Retried using command-local source replacement pointing to coordinator's existing
vendor (read-only); no package/dependency/lock/global config change. Reproduction, repo root:

```sh
CARGO_HOME="$PWD/desktop-feedback/linux/target/cargo-home" \
  cargo --config 'source.crates-io.replace-with="vendored-sources"' \
  --config "source.vendored-sources.directory=\"$PWD/.agents/runs/linux-parity-20261015/renderer-core-vendor-1\"" \
  check --offline --locked --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --tests

DOCS_RS=1 CARGO_HOME="$PWD/desktop-feedback/linux/target/cargo-home" \
  cargo --config 'source.crates-io.replace-with="vendored-sources"' \
  --config "source.vendored-sources.directory=\"$PWD/.agents/runs/linux-parity-20261015/renderer-core-vendor-1\"" \
  check --offline --locked --manifest-path desktop-feedback/linux/Cargo.toml \
  --target x86_64-unknown-linux-gnu --features native-ui --all-targets
```

## Independent handoff (commands NOT run here)

New candidate checksum manifest: `testing/process-eof-red.sha256` (repo-relative paths).
Keep the old GREEN archive/manifest intact; its historical source hashes are not the current
mutable-tree hashes. Freeze/package this candidate separately, retaining the unchanged shared
protocol path dependency and the existing full optional graph vendor. No new dependencies.

Run these separately, preserving each exit code and complete bounded log (do not chain the
expected failing RED command with `&&` before regressions or link):

```sh
# New assertions: expected assertion RED, not an infra failure.
cargo-1.89 test --locked --offline \
  --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test native_ui_process_completion --test native_ui_startup_eof \
  -- --nocapture --test-threads=1

# Original43 plus ALL36, explicitly including the three codec_contract tests.
cargo-1.89 test --locked --offline \
  --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport \
  --test codec_contract --test geometry --test ipc_input --test ipc_output --test model \
  -- --nocapture --test-threads=1

# Actual new-snapshot executable link in the approved derived builder, NO DOCS_RS.
cargo-1.89 build --locked --offline \
  --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --bin rpa-desktop-feedback-linux
```

Use CC's existing explicit vendor configuration, not this macOS absolute path. No new image,
package installation, renderer launch, X connection or UI fixture is required for these gates.

## Updated baseline evidence, not transferable to this candidate

Coordinator CC74308 / `.agents/reports/2026-10-16-linux-renderer-native-green-1.md` now records
original frozen43 native contracts PASS, commanded core33 PASS, and the **actual executable
linked on Linux Rust1.89 without DOCS_RS**, never run. `codec_contract`3 was omitted from that
run, so this is not a current36-core rerun. Observer24 units are separate, not renderer tests.
These facts supersede the old handoff's pending-link status for that exact old archive only.
R1/R2 remain source-confirmed despite those pure GREEN tests. This candidate needs its own
independent assertions/regressions/link; no live UI qualification before critical shutdown
classification is resolved. Full capture exclusion remains unsupported and unresolved.
