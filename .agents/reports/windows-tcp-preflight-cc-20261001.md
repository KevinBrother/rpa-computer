# Windows TCP Disconnect Test — Pre-Implementation Review (CC)

Date: 2026-09-30 (UTC; paths retain the existing `20261001` batch label). Scope: read-only review of EXISTING production transport (`src/mcp/remote/{mod,host,pump,tls}.rs`, `src/bin/computer-host.rs`, `src/mcp/stdio.rs`, `src/feedback/config.rs`) against the plan `docs/superpowers/plans/2026-10-01-windows-tcp-disconnect.md`. No test code inspected (not yet written), no builds/runs/network/SSH. No runtime evidence.

**PREIMPLEMENTATION_REVIEW_ONLY — no runtime evidence. STOP.**

## Q1 — Test-only exe + self re-exec feasibility

**Verdict: feasible with the public API, no product modification required, with the traps below.**

- `remote::host::run(RemoteArgs, Arc<AtomicBool>)` is public (`src/mcp/remote/mod.rs:20-23`, `src/mcp/mod.rs:29`; `host.rs:79`). All `RemoteArgs` fields are public (`host.rs:60-68`), so a test executable (example/bin) can call it directly with `mock_backend=true`, `feedback: FeedbackConfig::default()` (disabled).
- Child spawn uses `std::env::current_exe()` and forwards EXACTLY: `--mock-backend` (only when `RemoteArgs.mock_backend`) plus `feedback.append_args` (`host.rs:217-240`). With `FeedbackConfig::default()`, `append_args` appends nothing (`src/feedback/config.rs:35-45`). **No other args are forwarded** — the test exe's "am I the stdio child?" routing can only key on the presence of `--mock-backend` (argv) or inherited env (env is inherited by default). This matches the plan's intent.
- The test exe must therefore implement its own stdio-mode wiring: `Worker::start_with_feedback_shutdown(BackendFactory::Mock, …)` + `stdio::run` (both public; used at `src/bin/computer-host.rs:335-341, 405`). In mock mode the production binary skips DesktopLock and hotkey (`computer-host.rs:307-327, 359-375`); the test must replicate that skip — it is test code's job, not something `remote::host` does.
- **Trap 1 — re-entry loop:** `current_exe()` re-exec means the test exe MUST branch on `--mock-backend` BEFORE entering supervisor mode, else it forks itself infinitely. Related: a `#[test]` harness binary would re-enter the test harness, not stdio mode; the plan's choice of a standalone `examples/` executable is required, not optional.
- **Trap 2 — Cargo/test-harness current_exe:** the frozen runner must execute the exe directly (not via `cargo test`), so `current_exe()` resolves to the fixture exe itself.
- **No required product capability found:** no feature flag, no private API, no global state in `remote::host` blocks the mock-only path. `computer-host.exe`, DesktopBackend, native desktop, GUI/feedback renderer are never started in this design.

## Q2 — Loopback enforcement & security boundary

- `RemoteArgs.listen` is a raw `SocketAddr`; `host.rs:83` binds whatever it is given. Unlike the `--listen` path (`computer-host.rs:114` calls `tcp::ensure_loopback`), the `--remote-listen` parse (`computer-host.rs:118-127`) does NOT enforce loopback. **Loopback-only must be asserted in the test code** (parse/build `127.0.0.1:0`-style addrs, or reuse public `tcp::ensure_loopback`) — this is test-side, not a product change.
- Do NOT claim loopback universally prevents a Windows Firewall prompt. Loopback traffic typically does not trigger one, but flags/path choices are not authorization. Any actual security prompt (firewall, consent, accessibility) encountered during execution remains a STOP condition.
- The test cert/token reuse must stay read-only; this standalone mock transport integration is NOT an alternative path to restore desktop Host access, and no test result may be cited as evidence that real Host network authorization is resolved.

## Q3 — Existing teardown behavior and concrete traps

Sources: `src/mcp/remote/pump.rs`, `src/mcp/remote/host.rs:328-517`, `src/mcp/stdio.rs`.

1. **Raw TCP EOF ≠ clean close.** Socket EOF without `close_notify` is ALWAYS `PumpEvent::Failed("connection truncated…")` (`pump.rs:399-411`). Scenarios 1/2 (bare socket close) take the FATAL bridge path (`host.rs:402-405`), not `ClosedByPeer`. Tests must assert accordingly; only scenario 3 (`close_notify`) produces `ClosedByPeer` (`pump.rs:443-446`).
2. **Partial frame on clean peer close is newline-terminated and DELIVERED, not dropped.** `bridge` flushes a trailing partial frame with an appended `\n` when `clean_peer_close` (`host.rs:421-424`). On FATAL paths the partial frame is silently discarded (no forward). Plan scenario 4 asks whether production "补newline/丢弃" — the answer is BOTH, split by close type. Tests must encode this actual behavior; if the plan assumed pure discard, that assumption must be corrected before coding.
3. **Stdin close → cancel path:** teardown drops `child_tx` (`host.rs:425`) → in-thread drops child stdin (`host.rs:364-376`) → stdio reader sees EOF, cancels immediately and posts control EOF (`stdio.rs:163-167`, EOF rides a separate always-accepted control slot, `stdio.rs:32-34`). In-flight actions converge on the shared cancel flag — this is what the held-key scenarios exercise.
4. **Reap timing:** `reap_child` waits `CHILD_EXIT_GRACE = 5s` after stdin close, then kills and allows 2s to reap (`host.rs:56, 471-506`). Forced kill ⇒ `Quarantine` ("native cleanup unconfirmed", `host.rs:489-497`), and exit code 7 (`shutdown_unknown`) also quarantines (`host.rs:511-517`). Held-key cancellation must complete WITHIN the 5s grace or the scenario becomes quarantine, not clean reuse — promptness assertions need headroom under 5s and scenarios 5 (reconnect/new PID) require a cleanly exited child.
5. **Teardown ordering & latency:** `bridge` → drop child_tx → `pump.finish(PUMP_DRAIN+2s)` (graceful window then abort; `JoinTimeout` never success, `pump.rs:119-136`) → bounded joins of out/in threads (`host.rs:426-436`) → only then `reap_child`. A hung child with open stdout can stretch teardown to roughly 10-12s before the kill; test deadlines must exceed this, and "supervisor reusable" evidence must come AFTER reap (`host.rs:97-116` reaps before accepting).
6. **Inherited pipes:** child stderr is inherited to supervisor stderr (`host.rs:232`); child stdout reader is the only outbound producer and its sender drop triggers our close_notify (`host.rs:339-358`). Child spawn happens strictly after auth; auth failure spawns NO child (`host.rs:190-211`).
7. **No over-claim:** the mock backend never touches the desktop; a mock "held release" proves only the cancel/hold state machine and child lifecycle, NOT that the OS released a real input hold and NOT that production `computer-host` `serve()` main (lock/hotkey/signal wiring) was exercised. Reports must say exactly this.

## Q4 — What final review of the frozen test code must check (preconditions for execution approval)

Approval cannot be granted from the plan alone. When the frozen code exists, verify:

1. **Mock-only isolation:** `--mock-backend` argv branch routes ONLY to `BackendFactory::Mock` + `stdio::run`; no DesktopBackend construction path, no DesktopLock, no hotkey, no native input call reachable in that mode; guard against supervisor re-entry (Trap 1).
2. **Loopback-only:** every listener addr asserted `127.0.0.1` before `run`; no `0.0.0.0`/LAN/public, no firewall/netsh/permission API calls, no system setting changes; explicit "prompt ⇒ stop" handling.
3. **Assertion honesty:** per plan line 27 — outcomes derived from real child exit status, pump `Outcome`, PID/creation-identity deltas; no self-written success flags; raw-EOF vs close_notify vs partial-frame expectations match host.rs:421-424 (Q3.2).
4. **Ownership & safety:** teardown kills only owned child handles; no global/name-based kill; Notepad24332 and user desktop untouched; bounded joins, no unbounded reads, no sleep-only synchronization; failure path preserves first-failure evidence.
5. **Freeze integrity:** no changes to `src/`, `Cargo.toml/lock`, existing tests/certs; only the new test/example files plus this evidence set.
6. **Secrets:** token never logged (host already avoids it; test must too).

Only after all six pass may execution be authorized; any deviation is returned to the author, not approved.

## Risk register (source-referenced)

- Partial-frame newline append on clean close may contradict plan expectations — `host.rs:421-424` (HIGH, decide before coding).
- Loopback not enforced in `--remote-listen` parse — `computer-host.rs:118-127`; test must enforce — `host.rs:83` (HIGH).
- 5s child grace vs held-key cancellation promptness — `host.rs:56, 471-506` (MED).
- Quarantine on exit code 7 blocks scenario-5 reuse — `host.rs:511-517` (MED).
- Teardown latency up to ~12s before kill; test timeouts must account — `host.rs:426-436` (MED).
- Re-exec self-branching / test-harness current_exe — `host.rs:217-241` (MED).
- Raw EOF classified `Failed`, not `ClosedByPeer` — `pump.rs:399-411` (INFO, by design).

**PREIMPLEMENTATION_REVIEW_ONLY — no runtime evidence. STOP.**


## Coordinator scope clarifications (not runtime evidence)

- This batch requires `BackendFactory::Test` with an instrumented fake Backend, not necessarily the built-in `BackendFactory::Mock`. The `--mock-backend` argv is a routing flag on the test executable; it does not force that executable to construct the built-in Mock. The final review must enforce **no native backend/input path**, not a particular enum spelling. Only the instrumented Test Backend can supply the requested dispatch/held diagnostics without product changes.
- The 5s `reap_child` grace starts at entry to `reap_child`, **after** `bridge` returns, not when child stdin initially closes. Promptness must be measured from the peer cutoff to the recorded worker cancellation/release; an eventual reap under its own deadline is not sufficient evidence of prompt cancellation.
- No claim is made about whether Windows will show an authorization prompt for loopback. Preflight establishes the proposed software boundary, not actual OS policy, desktop visibility, or approval to run an unfrozen implementation.
