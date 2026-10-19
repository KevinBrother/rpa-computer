# NKN + initial-handoff implementation checkpoint (not observed GREEN/live)

2026-10-16. Implements the authorized, independently observed RED contracts.
**Developer ran compile-only, not tests/SSH/GUI/input.** This source checkpoint is
called GREEN implementation, not a declaration that CC tests or real X passed.
No live execution is authorized by this handoff.

## Frozen ancestry / observed RED

Base: immutable386 `hotkey-initial-handoff-integrated-red-1`, archive SHA256
`09a8818af3ea0d66de7cd958f61fb55a81c0447a2132f18a71e73cb4f316daaf`.
Earlier immutable382 NKN base SHA256
`1bc687b6ea6bb8006bd6248308814d2e7eb05d314d898135df10d68295c974e8`.

Read independent raw evidence:
- `cc-hotkey-nkn-revalidation-red-1/probe.log`: compile0;
  revalidation26 =25 assertion FAIL/1PASS; auxiliary8 =8 assertion FAIL.
- `cc-initial-handoff-red-1/probe.log`: compile0; initial-handoff4 =4 assertion
  FAIL/0PASS, rc101, the four positive calls hit the explicit Err scaffold.

These are new scaffold RED contracts, not reproduction of the historical unsafe
GetMap decoding defects or the real default-input NKN failure. Those separate
real-X records and the exact original canonical assertions remain unchanged.

## Actual implementation

- `xkb_aux_wire/decode.rs`: byte-only complete Controls92-byte reply and full
  CompatMap (`firstSI=0`, returnedSI==total, all4 group bindings). Checked declared
  size/count/extent before allocation; all81 control config bytes, ordered SI16
  records and group4 records retained, opaque actions including unknown tags.
  Only transport metadata/protocol padding excluded. No typed pointer/enum casts,
  generated dynamic getter, dependency change or normalization chosen to force a
  default-server positive result.
- `capture.rs`: exactly two full observations from the SAME LiveSource connection,
  each through actual planner + `validate_owned`, then exact full-image/plan/
  modifier-dependency equality, one read-only roundtrip fence, bounded queue drain.
  Zero retries. Any intervening real core/XKB invalidation, including a companion
  or second NKN, revokes even if A==B. No request_minor/source-device inference.
- `queue.rs`: one bounded FIFO, no Press-only filtering; retains Release/Other.
  Each attempted queued read including None is charged. A failed drain never
  returns a proof. Stop/failure/deadline and registration generation are checked
  before work and AFTER the final None. No deadline/lease renewal.
- `revalidation/mod.rs`: NKN known bits/same identity/ranges only permit trying;
  the complete stable image must equal the authorized current baseline. Private
  proof construction follows comparisons and final checks. No active/ready/cancel
  write, callback or grab change. Cache assignment is worker-local and restored
  if its final lifecycle check fails; it cannot revive failed registration.
- `scratch.rs`: only an already nonfatal, single known component/range may attempt
  maintenance. Patch ONLY that allowed region into a copy of the authorized image,
  then demand equality to the entire new image and unchanged grabs/physical
  modifiers/dependency sets. Protected/out-of-range/unknown changes, state-producing
  actions, controls/compat/virtual bindings cannot be adopted. Changed action data
  must remain same-length NoAction, changed behavior must be default/inert. Combined
  or unknown notifications and companions remain conservative failures, not silenced.
- `initial_handoff.rs`: actual post-grab/pre-ready function now uses the capture's
  original deadline and consumed poll counter, does one final fence/drain, and
  checks lifecycle/generation again before return. No second A/B capture and no
  new256 window. Corrupt/exhausted accounting fails closed.
- `x11.rs`: pre-ready `ensure_initializing` now also rejects a published failure,
  not only stop/deadline. Existing candidate queue and lifecycle implementation
  remain unchanged; their contracts are NOT re-labelled as preserving all events.

Operation timing: NKN/scratch use the one absolute200ms passed by the pump and the
existing min-deadline same-socket watchdog. Initialization carries its original2s
budget through capture, grabs, handoff, and readiness. **1MiB auxiliary/GetMap caps
are post-wait only**, not limits on libxcb's preceding allocation or read. Existing
F4 synchronous authority-file I/O blocker remains; shared helper consumer migration
is not part of this lane/checkpoint. No end-to-end filesystem setup bound claimed.

## N2/G1 actual counter transfer and G2 publication boundary evidence

Source locations at this freeze:

1. `capture.rs:14,23–27`: initialize from prefetched FIFO, call the real shared
   drain, store its actual `polls` (including None) into returned baseline.
2. `queue.rs:47–51`: increment BEFORE each actual queued attempt; None immediately
   goes through another lifecycle/generation checkpoint. No hidden reset.
3. `x11.rs:216–220,248–253`: the same returned baseline and pending are passed
   across the checked-grab loop, with the SAME original deadline.
4. `initial_handoff.rs:17–34`: enforce remaining budget, pass
   `&mut baseline.initial_queue_polls` directly to the shared drain, final check.
5. `x11.rs:254–264,186–193`: FIFO moves intact; deadline/stop/failure checked before
   ready.send and while awaiting acceptance. `x11_lifecycle.rs:25–51` is unchanged:
   caller's readiness acceptance checks deadline/stop/failure/phase under its
   existing phase mutex before publishing Active. New proof code never publishes it.

`integration_tests.rs` adds exactly TWO supplemental tests, keeping original34/4
byte-identical: real capture→handoff with the same Script/deadline, aggregate256
and full FIFO, plus final-None-return injection of stop/failure/generation/expiry.
They were written before implementing handoff acceptance. **No separate historical
RED run is claimed for these2**; coordinator explicitly approved running them with
GREEN regressions after observing the original4 RED.

G2 is a finite last-check regression plus source evidence for the existing locked
readiness transition, NOT proof of every concurrent scheduling window, nor a
server-wide transaction preventing arbitrary future map changes. Subsequent
notifications still go through the normal pump; no automatic rearm exemption.

## Runtime comparison evidence hook (test binary only)

New `comparison_diagnostic.rs` emits at most ONE `HOTKEY_NKN_COMPARE` line per
process after actual coherent A/B/fence succeeds, before baseline acceptance:
concrete map device IDs, charged queue budget and direct per-component equality
(core, all8 map fields, controls, compat, plan/modifier dependencies). A/B equality
and completed fence are successful capture postconditions, not inferred from NKN
metadata. No X request, full map/text, cookie, raw address or input-source claim.
I/O errors do not change authorization; synchronous stderr exists only under
cfg(test), not the production binary or its timing guarantee. The old one-shot
invalidation diagnostic is byte-identical and remains independent.

Actual default-server complete images/equality and real cancellation remain
UNKNOWN until coordinator-authorized canonical qualification. Even a comparison
line with alltrue is config evidence only, not proof of cancel/replay delivery.
Do not prewarm, modify the default map, rewrite runners or use Armed as delivery.

## Compile-only and exact file preservation

Final developer isolated Linux hotkey harness: `rustc --test --emit=metadata`
and non-test `--crate-type=lib --emit=metadata`, both exit0. No binary linked/run,
no full-root/native build or test outcome claimed. Test harness has no warnings;
non-test harness has five unused/dead-code warnings (including proof accessor).
Commands/logs live in `target/hotkey-nkn-green-work/` outside the source overlay.

Compared directly to frozen386's61 hotkey files: **57 byte-identical,4 changed,
0 deleted**. This is archive-to-worktree evidence, not the earlier inventory that
included an unarchived DESIGN.md. Original34, initial4, old51/57/3, all canonical/
Stage2 fixtures/runners and old diagnostic are within the unchanged set. Historical
RED docs/manifests remain untouched, intentionally retaining old hashes.

Changed4:
- `src/mcp/hotkey/x11.rs`
- `src/mcp/hotkey/x11/revalidation/mod.rs`
- `src/mcp/hotkey/x11/revalidation/initial_handoff.rs`
- `src/mcp/hotkey/x11/xkb_aux_wire/mod.rs`

New6 Rust:
- `src/mcp/hotkey/x11/revalidation/{capture,queue,scratch,comparison_diagnostic,integration_tests}.rs`
- `src/mcp/hotkey/x11/xkb_aux_wire/decode.rs`

Plus this GREEN.md and GREEN.SHA256SUMS =12-file overlay. No Cargo/rootlib/helper/
native/other-platform source edit, no commit/push/worktree. Coordinator should
apply only this overlay to frozen386, not package unrelated mutable-tree changes.

## Independent CC commands (developer did NOT execute)

Run each filter independently, retain actual counts/exit/assertions, reject zero
cases. Existing locked/offline Linux setup, no DISPLAY/input/GUI. New groups total
**40 =26+8+4+2**. Original38 assertions retained; only2 integration tests are new.

```sh
cargo-1.89 test --locked --offline --lib --no-run
cargo-1.89 test --locked --offline --lib 'mcp::hotkey::x11::revalidation::tests::' -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib 'mcp::hotkey::x11::xkb_aux_wire::tests::' -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib 'mcp::hotkey::x11::revalidation::initial_handoff_tests::' -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib 'mcp::hotkey::x11::revalidation::integration_tests::' -- --nocapture --test-threads=1
```

Also retain old scoped51 (wire21/review19/owned-map11), old57 and ONLY the original3
ignored Unix-socket tests, plus root/binary link gates as coordinator schedules.
Do not run all ignored tests or any canonical/live fixture from these commands.
No live qualification is conferred by this implementation/compile checkpoint.
