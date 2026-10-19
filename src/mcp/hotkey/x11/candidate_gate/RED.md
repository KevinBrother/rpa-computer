# B1 candidate predecision RED — 2026-10-16

Scope: one queue-placement regression split into three tests, against immutable
`hotkey-nkn-handoff-integrated-green-1` (394 files). No acceptance fix, lifecycle
expansion, G1/G2 redo, live authorization or changes to previous assertions.

## Production boundary and intentionally preserved defect

`x11.rs` now calls `candidate_gate::check_then_decide` at the original boundary:
after QueryKeymap and before actual `x11_candidate::decide`, cancel store or
replay/ungrab. The function receives the actual prefetched FIFO remainder but
INTENTIONALLY still ignores it. It invokes the original XCB queue check, then
stop check, then actual decision function. This is executable B1, NOT an
unconditional Err scaffold. The obsolete N1 scaffold comment is corrected.

`xkb_events::validate_queue_from` extracts the existing bounded256 drain and
non-synthetic KeyPress classifier without changing policy. The live wrapper
supplies `poll_for_queued_event` and the unchanged map-invalidity interpreter.
Tests supply owned event records to this same drain, not a preclassified result
or a test-only correct decision algorithm. Their validity callback accepts only
ordinary Press/Release/Expose records; no map-invalidity coverage is claimed.

## Three cases (not executed by developer)

Exact filter: `mcp::hotkey::x11::candidate_gate::tests::`

1. `single_legitimate_candidate_reaches_actual_decide_and_sets_cancel`
2. `same_extra_physical_press_in_local_or_xcb_queue_is_rejected_before_decide`
3. `synthetic_press_and_ordinary_nonpress_keep_existing_policy_in_both_queues`

Case2 first proves a legitimate candidate reaches actual decide/cancel; then the
same extra real-tagged press is placed in the XCB queue (existing rejection) and
local FIFO (B1: currently reaches Cancel). Both placements require Err AND cancel
still false. Held keys and all other inputs are identical between placements.
Case3 preserves synthetic/Release/Expose handling: local entries remain intact;
ordinary/synthetic XCB entries retain existing drain behavior. These are owned
records, NOT actual X11 delivery/input proof. Fixed-record constructors plus one
audited initialized response-byte edit model real/synthetic/release tags; no
unsafe variable reply decoder, X connection, XTEST, SendEvent or GUI is invoked.

Source-expected result: **2 PASS / 1 assertion FAIL**; independent CC must observe
actual counts and failure. No current test result is claimed. Positive controls
prevent a reject-all implementation from satisfying this checkpoint.

## Compile / preservation evidence

Developer compile-only Linux isolated hotkey harness, `rustc --test
--emit=metadata`: exit0, no diagnostics. No linking, test execution, SSH or GUI.
The first compile had two module-import errors; corrected before final exit0.
Compile log: `target/hotkey-candidate-gate-red-work/compile.log`.

Compared directly with immutable394:69 old hotkey files, **67 identical,
2 changed,0 deleted**. Original40/51/57/3 tests and canonical/Stage2 files are
byte-identical. The pre-edit hotkey hash inventory independently finds only
these same two existing changes. No Cargo, rootlib, helper, native/input, other
platform, runner or frozen archive edits. No commit/push/worktree.

Six-file overlay (apply ONLY these to immutable394, never mutable whole tree):
- `src/mcp/hotkey/x11.rs` (changed)
- `src/mcp/hotkey/x11/xkb_events.rs` (changed)
- `src/mcp/hotkey/x11/candidate_gate/mod.rs` (new)
- `src/mcp/hotkey/x11/candidate_gate/tests.rs` (new)
- `src/mcp/hotkey/x11/candidate_gate/RED.md` (new)
- `src/mcp/hotkey/x11/candidate_gate/SHA256SUMS` (new; five payload hashes)

## Exact independent CC commands — developer did NOT execute

Headless locked/offline Linux environment; no DISPLAY required. Reject zero-case
or compile/infra failures as assertion RED. Do not run ignored/live fixtures.

```sh
cargo-1.89 test --locked --offline --lib --no-run
cargo-1.89 test --locked --offline --lib 'mcp::hotkey::x11::candidate_gate::tests::' -- --nocapture --test-threads=1
```

Compile-only reproduction used locally:

```sh
printf '#[path="%s/src/mcp"] pub mod mcp { pub mod hotkey; }\n' "$PWD" |
rustc --edition=2021 --crate-name hotkey_compile_only \
  --target x86_64-unknown-linux-gnu --test --emit=metadata \
  -L dependency=target/x86_64-unknown-linux-gnu/debug/deps \
  --extern xcb=target/x86_64-unknown-linux-gnu/debug/deps/libxcb-48d016ae9456751a.rmeta \
  --extern libc=target/x86_64-unknown-linux-gnu/debug/deps/liblibc-d441ec46a247f3a1.rmeta \
  -o target/hotkey-candidate-gate-red-work/compile-only.rmeta -
```

Freeze at RED; no GREEN acceptance repair until coordinator's next gate.
