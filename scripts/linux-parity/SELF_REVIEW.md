# Static self-review / CC handoff — 2026-10-15

No tests, SSH, GUI, input, host execution, build or deployment performed. Only
`py_compile` executed; this is not test/acceptance evidence. Independent review
and all execution remain CC responsibilities. Product builds are not ready.

Reviewed paths:
- RPC stdin/stdout nonblocking, pending request deadlines retain original end,
  total deadlines bound writes/read polling, malformed/oversize/duplicate/unknown
  frames fail without retries. Concurrent control replies routed by id.
- Fixture polling requires current PID/schema/heartbeat and stable predicates;
  callback exceptions become explicit state failures. Key recorder runs before
  app handlers which can stop binding propagation. No injected events.
- Reset observation input-sequence baseline on reopening a new session; retain
  strict fresh observation provenance and map measured fixture root positions.
- Exact native ASCII/Chinese/emoji; native LF/CR/CRLF/Tab normal key semantics;
  explicit paste requires actual multiline receipt, unchanged Enter effects,
  retained focus, no Return/Tab key fallback and repeatable ordinary Ctrl+V.
- Save post-host X11 state even on restoration/held-input failures. Verify exit
  status even if host exited before teardown. Broken transport cannot count as
  confirmed close. Cleanup uncertainty stops later cases.
- Cancellation waits for actual fixture-held input before requesting stop; checks
  release edges, X server idle, no delayed input. EOF missing reply stays unknown.
- Wrapper changes only newly owned authenticated Xvfb settings. Temp auth/runtime
  survive cleanup on failures too; child-only env avoids parent restoration races.
  Private PID/start-tick ledger supports bounded owned-child emergency cleanup.
- Removed control-flow break from finally (compiler warning); all Python modules
  below 1000 lines. File-only screenshots and metadata; raw host stderr only hashed.

Remaining evidence gaps are explicit in run.py DEFERRED and the runbook. Full
XKB native structures, universal image oracle, multimonitor topology, clipboard
fault injection and cross-UID/revocation are not implemented. Do not call the
core projection or a successful native smoke complete Linux parity.

CC should first run harness unit tests (transport doubles only), then use a ready
real binary with isolated smoke, core, cancellation, authority/HiDPI; probe explicit
clipboard separately. Do not deploy to production or touch existing user windows.

## CC first-GUI follow-up (source/evidence review only)

CC supplied separate evidence: original 14 harness units passed; frozen-product
smoke passed; 96-DPI displays failed fixture freshness, so 192 DPI was not run.
These are CC results, not tests executed by this author. See the runbook appendix
for archive/build hashes and exact times; current mutable native fixes were not
used to explain the frozen binary.

Concrete defect addressed: stale bytes/age were discarded by state() and cleanup,
then overwritten by shutdown. Regression tests in test_diagnostics.py were
written before the preservation changes, and are awaiting CC execution. New
read-only owned-PID diagnostics and bounded fixture stderr help isolate a stalled
Tk timer/query/write vs. environment/product contention. Optional periodic stack
dumps never pump Tk or refresh ground truth. Freshness remains age < 1s, stale
matching predicates fail immediately, and shutdown is a separate artifact.

The cause of the actual heartbeat stall is NOT established. No timer recovery,
new acceptance retry, relaxed deadline or product edit was made. Frozen artifacts
were only read. Diagnostic I/O/watchdog threads can perturb timing; a non-repro is
not a fix claim. Independent CC must execute both the added unit tests and new
uniquely named reproduction(s) against the same frozen host before attribution.
