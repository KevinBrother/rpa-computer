# B1 minimal GREEN candidate — 2026-10-16

## Observed RED (independent CC, not developer execution)

Immutable398 `hotkey-candidate-gate-integrated-red-1`, CC35616 raw
`.agents/runs/linux-parity-20261015/cc-writer-green-hotkey-gate-red-1/probe.log`,
B-RUN2:3 cases,2PASS/1 assertion FAIL,exit101. Exact failure at tests.rs153:
`location_local=true result=Ok(Some(Cancel)) cancel=true`.
This is the actual executable omission, not a scaffold/compile/infra failure.

## Only implementation change / self-review

Only `candidate_gate/mod.rs` changes. The actual pump's existing call remains
unchanged. After the original XCB validation drain, before stop/decide/cancel,
reject if the local FIFO remainder contains a non-synthetic core KeyPress.

- `validate_queued()?` still runs FIRST, once; its errors retain priority and
  propagate unchanged. Nonempty XCB result retains the same rejection text.
- The local predicate exactly matches the existing XCB classifier: core
  KeyPress with response_type high bit clear. No interpretation of map/action
  events, new validity policy, retries, X requests or queue-budget changes.
- `pending.iter().any(...)` borrows only; no pop/drain/filter/reorder. Synthetic
  presses, releases and ordinary nonpress retain their existing policy/FIFO.
- Scan is O(existing pending length), short-circuits, no allocation or I/O; it
  does not create a new event collector or authorize an unbounded read.
- The original SeqCst stop check and actual `x11_candidate::decide` remain after
  rejection. Local real presses therefore cannot reach its cancel store.
- No claim to close every future concurrent event/publication race; only the
  reviewer-identified already-prefetched candidate boundary is changed.

Original3/40/51/57/3 assertions, canonical/live fixtures, runners and all other
modules are untouched. Historical RED.md/SHA256SUMS intentionally stay frozen;
new GREEN.SHA256SUMS describes this overlay, not historical RED identity.

## Compile-only and packaging

Linux isolated hotkey `rustc --test --emit=metadata`: exit0, no diagnostics.
Log: `target/hotkey-candidate-gate-green-work/compile.log`.
No tests/link/SSH/GUI/input execution. No manifest/dependency/rootlib/helper/native
edits, commit/push/worktree. Compilation is not independent GREEN evidence.

Three-file overlay against immutable398 ONLY:
1. `src/mcp/hotkey/x11/candidate_gate/mod.rs` (one implementation replacement)
2. `src/mcp/hotkey/x11/candidate_gate/GREEN.md` (new)
3. `src/mcp/hotkey/x11/candidate_gate/GREEN.SHA256SUMS` (new)

Full frozen payload =398 baseline files +these replacements/additions =400 files:
397 original files unchanged,1 changed,2 new,0 removed. Original73 hotkey files
have72 identical and only mod.rs changed. Full freeze is constructed from the
verified immutable398 manifest, NOT mutable whole-tree sources. Full400 manifest
and SHA256SUMS are sibling artifacts under `target/hotkey-candidate-gate-green-1-*`.

## Independent CC commands (not executed by developer)

```sh
cargo-1.89 test --locked --offline --lib --no-run
cargo-1.89 test --locked --offline --lib 'mcp::hotkey::x11::candidate_gate::tests::' -- --nocapture --test-threads=1
```

Expect3 cases; actual result remains pending. Existing40/51/57/3 regression
selection remains with coordinator; no ignored/live execution is authorized by
this delivery. Await independent source review + CC; do NOT call this live/full
hotkey GREEN or cancellation qualification.
