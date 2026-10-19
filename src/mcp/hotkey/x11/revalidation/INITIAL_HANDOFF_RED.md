# N2 initial handoff: four production-boundary RED assertions

2026-10-16. **RED only, no accepted handoff implementation, no live eligibility.**
Base: frozen382 `hotkey-nkn-revalidation-integrated-red-1`, archive SHA256
`1bc687b6ea6bb8006bd6248308814d2e7eb05d314d898135df10d68295c974e8`.
That base and its historical manifests are not edited/repacked by developer.

## Exact boundary and contract

`serve` now calls `finish_initial` **after all checked passive grabs, before
ready.send**, replacing the old `capture_pending.extend(validate_queue(...))`.
It passes the same registration-owned baseline, existing pending FIFO, same
LiveSource connection/Control and original init deadline. Any Err goes through
the existing failed-cleanup path before readiness can be sent. No source changes
to the candidate queue, callbacks, lifecycle primitives, watchdog or runners.

The newly registered function is an explicit Err scaffold. Even if the already
observed34-test protocol is later made GREEN, this checkpoint will not arm until
N2 is implemented after independent observed4 RED and authorization.

The baseline carries private `initial_queue_polls`. The future capture
implementation must carry forward all its queued-poll attempts, **including
empty results**, and finish_initial must consume only the remainder of the
**single256 total**. No budget reset at grabs; no second A/B image capture. A
single read-only final fence can bound arrival of notifications during the grabs;
its event drain shares the original init deadline and aggregate poll budget.
All retained ordinary events, including Release/Other, remain FIFO across the
handoff. Late fatal events, expiry, stop/failure, generation replacement or
fence/queue failure reject; they never publish readiness.

This is not a claim that the unchanged candidate `validate_queue` preserves all
event classes or participates in the init budget. That separate physical-key
candidate path is untouched. It also does not infer a transaction against future
server changes after the last fence; later events remain subject to the pump.

## Four new tests, no edits to existing tests

`revalidation::initial_handoff_tests::` directly invokes the actual newly wired
production `finish_initial` entry with the existing Script adapter:

1. Completed-capture FIFO + new Press/Release/Other retained in order, no A/B
   reread, exact consumed-poll accumulation.
2. Positive near-limit case; capture-used255/256/257 budget rejects rather than
   starting another256 window. Includes empty-result poll accounting and bounded
   number of further queued calls.
3. Quiet handoff positive control, then all five late invalidation kinds plus
   prefetched fatal event rejected before handoff success.
4. Same caller-supplied absolute deadline across final fence/drain; already expired
   or stopped means no further requests; expiry/stop/failure/generation change at
   fence and read errors cannot return success.

Fixtures manually provide captured-state inputs (count of retained events plus
one empty poll); they do NOT implement drain/comparison logic, call real X, or
pretend capture's unfinished implementation has produced that state. Future GREEN
must connect capture's actual poll count to this field; source review must check
that transfer, not merely these direct-boundary test results.

**Expected ONLY:** four assertion FAIL at positive calls into explicit Err.
No tests were executed by developer. This is neither the old NKN runtime failure
reproduction nor proof that negative assertions behind the positive controls ran.

## Independent CC exact commands

From the new frozen Linux overlay with existing locked/offline dependency setup:

```sh
cargo-1.89 test --locked --offline --lib --no-run
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::revalidation::initial_handoff_tests::' \
  -- --nocapture --test-threads=1
```

Require actual **4 cases**, record exit/assertions, reject zero/compile/infra as RED.
No ignored tests, live registration, canonical run, SSH/GUI/input by developer.
Do not append this run to already finishing CC53081; coordinator owns scheduling.
Existing34 and51/57/3 tests/canonical/Stage2 sources are byte-identical and remain
required future gates. The old382 snapshot's results stay separate.

## Compile-only / freeze

Final developer Linux `rustc --test --emit=metadata` exit0 (no link/run). Expected
scaffold dead-code warnings. Scope only6 overlay files:

- edited `src/mcp/hotkey/x11.rs`: exact post-grab/pre-ready call replacement;
- edited `src/mcp/hotkey/x11/revalidation/mod.rs`: registration/reexport and private
  carried poll field initialized0 (acceptance protocol still Err);
- new `initial_handoff.rs`, `initial_handoff_tests.rs`, this README and
  `INITIAL_HANDOFF_RED.SHA256SUMS`, all under that revalidation directory.

Original `RED.md`/`SHA256SUMS` preserve their older checkpoint identities and are
not rewritten. Neither existing34 positive controls nor any old assertion is
weakened/removed. Root/Cargo/helper/native/Mac/Windows sources untouched. Await
independent4 RED before implementing this new boundary's accepting path.
