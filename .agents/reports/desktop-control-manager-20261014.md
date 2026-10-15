# Desktop control manager — implementation / verification log

Status: phase-one implementation and Windows non-GUI regression complete.
Native desktop ownership lifecycle passed on October14 after user login; see the follow-up below.
Full-width IME fixture and DingTalk input/send acceptance subsequently completed; see
control-followup-20261014.md. All8 originally ignored cases now have explicit
passing evidence across follow-ups (last case: windows-registry-20261015.md).
Replacement of the original deployed host remains outside this test closeout.

## Approved scope

Persistent multi-client TLS connections; one centralized task authority; isolated
execution process only for an admitted task; open/close ownership; verified
retirement, quarantine, stale request isolation and machine-readable status.
No queue, new UI, force takeover, multi-machine or VM scheduling.

Design: docs/superpowers/specs/2026-10-14-desktop-control-manager-design.md.
Plan: docs/superpowers/plans/2026-10-14-desktop-control-manager.md.

## Baseline and workspace

Initial HEAD871e93c and existing uncommitted clipboard feature were preserved.
During work HEAD changed externally to595c9bd (`fix: windows text input`), with
those prior changes committed by another actor. Coordinator/agents performed no
commit, reset, push or worktree operation. Old desktop-lock proposal is historical
and superseded for remote TLS by the approved manager design.

Frozen pre-manager Windows binaries under ignored evidence baseline/:
- host SHA256 D3DE3A154161AA676A9F813242EAD4A9B9D31717795ED8A9EB50FACF9AC0451A
- client SHA256 D94FDB0BEEFA29215EBAC04F2B2352E3B67FFF7FD6A2DCA9A1B699A9F593D805

Evidence root: `.agents/runs/session-control-20261014/`.
Remote test root: `C:\Temp\rpa-session-control-20261014`.

## Environment / actual model

CC invocation uses `--model sonnet`; actual response model tags verified as
`glm-5.3-flash` (model-probe-v2.jsonl). No settings changes. Initial model probe
failed CLI parsing because variadic MCP option consumed the prompt; fixed with
`--` delimiter. No test or model response occurred in that first probe.

No original host was running at fresh read-only preflight. Attempt to safely
start the already-owned paste fixture host was refused before launch because
active console Session1 has no explorer.exe. Follow-up quser reports no logged-in
user. No user login/desktop was started or modified, no original process killed.
GUI acceptance of both manager and pending paste is therefore not yet possible.
This is unrelated to the preceding day's desktop-lock contention.

## Preserved unsuccessful verification attempts

- First RED orchestration (cc-red.jsonl) did not run the native test. Missing
  remote evidence parent caused runner validation failure Win32=3; subsequent
  evidence scp reported missing directory. No native_exit_code. Not a product
  failure and not RED evidence. Coordinator created the owned evidence parent,
  retained the failed attempt, and requested new phase red-v2.

## Test policy

All executable test runs via actual CC + GLM on Windows. Integration mock backend
runs are transport/authority evidence, NOT native GUI correctness. Codex agents
perform code authoring/compile checks only. No Mac/Linux GUI or test execution.

## Baseline RED verified

CC + GLM phase red-v2 executed `remote_control_minimal_handoff` against frozen
pre-manager binaries on Windows. Native exit101, expected assertion failure:
second client stdout closed at MCP handshake (0.37s, no timeout). New regression
requires that second authenticated connection remain usable. Transcript actual
assistant model tags: glm-5.3-flash. This is real TLS/mock-backend evidence only.
Evidence: `red-v2/red-v2-0-remote_session_control/result/{summary.json,stdout.log,stderr.log}`.

Runner containment limitation is retained: it bounds/kills only its owned test
root; test fixtures separately own/clean their host/client children. The runner
does not establish arbitrary descendant cleanup. Any timeout needs explicit
identity-based residual verification before rerunning.

## Provisional manager smoke (not final)

CC + GLM ran real built Windows binaries with explicit mock backend:
- remote_session_control:27 passed,0 failed,36.39s,native0.
- remote_transport:25 passed,1 failed,34.03s,native101.
- Failure: remote_raw_tls_wrong_token_gets_no_ack, assertion that rejected peer
  observes connection closure. Sent to implementation worker; not hidden/skipped.
- Transcript text miscounted transport total as27; raw stdout is26, authoritative.
- Exact provisional hashes recorded in smoke-hashes.json; implementation was
  still being hardened, so all suites must rerun against final hashes.

Independent early review identified partial-spawn boundedness, read-error/EOF
ambiguity, sticky uncertainty, pre-ready lock-refusal classification, and a
pre-existing OS-lock drop-before-process-exit gap. Repairs and deterministic
regression coverage are in progress; review is not yet closed.

## Review hardening after provisional smoke

Additional static-review fixes authored with deterministic actor regressions:
- Known cancellation before dispatch suppresses pipe writes, including a queued
  action preceding a validated pause/close; stale/foreign controls do not mutate
  the active task. Owner EOF is fenced again at actual command dispatch.
- Pending dispatched work with a missing result is reported input_outcome unknown,
  never inferred not_started merely from a clean child exit. JSON-RPC child errors
  and malformed replies complete the request once and preserve bounded cause.
- Clean permanent local revocation has explicit child exit8 (unknown cleanup
  exit7 takes precedence); the old connection cannot reset revocation by opening
  a fresh executor, including failed-open/close races.
- Post-exit pipe-drain grace is measured from observing process exit, separately
  from graceful native shutdown and forced-kill deadlines.

The first full Windows no-run build succeeded (86s) but preceded the last review
repairs. It is compile evidence only; final rebuild/test freeze follows.

## Frozen candidate v1 Windows regression

`final-hashes-v1.json` records the 13 staged artifacts; `green-final/summary.json`
and each suite's raw `result/stdout.log` confirm native exit0 for all11 suites:

| Suite | Passed | Ignored |
|---|---:|---:|
| remote_session_control |27|0|
| remote_transport |26|0|
| runtime_lib |325|7|
| runtime_contract |35|1|
| protocol |12|0|
| transport_lifecycle |3|0|
| text_paste_contract |12|0|
| windows_cancel_contract |9|0|
| windows_capture_disconnect |13|0|
| host_unit |3|0|
| client_unit |0|0|

Total465 passed,0 failed,8 ignored. Ignored tests: real screen capture1,
service-station clipboard mutation1, separately configured fake renderer5,
slow1000-step registry1. They are not counted as accepted/passing.

Actual assistant model in `cc-green-final.jsonl`: `glm-5.3-flash`. The runner
completed all suites successfully. The CC conversation then attempted unnecessary
summary parsing (including one denied command and irrelevant gtest patterns),
exhausted its turn budget and ended with `is_error:true`. This transcript-level
error is preserved and does not replace the independently inspected native
results; no claim that the overall CLI task itself exited successfully.

Final targeted review found one additional product issue after this binary freeze:
host shutdown could return success with retained executor or quarantined authority,
or after a caught shutdown-loop panic. Repair delegated with focused regressions;
v1 is therefore provisional, not the final source verification.

Deep Windows process fixtures are being adapted to the task broker's external
session IDs using passive child-response journaling. Held-key, readonly-ledger,
cleanup and actual process-exit oracles must remain intact.

## Final source review closure

The shutdown repair now requires BOTH no retained executor AND available
(non-quarantined) authority. Caught shutdown-loop panic and unconfirmed retirement
return an error, preserving an earlier failure. The host maps this to nonzero
exit6. Five focused regressions added. Laplace independently reviewed exact
host/broker/test hashes and closed the finding; all of its earlier dispatch,
revocation and EOF findings were also closed at source-review level.

Final Windows compile-only build uses the established xwin environment and
explicit llvm archive wrapper. `source-freeze-v2.json` covers Rust/Cargo inputs.
Deep fixture build IDs are both `session-control-20261014-deep-v1`. Public
`stdio::run_with_reader` change is documentation/visibility only; the fixture taps
bytes passively to record the child's real session ID. Existing held-key and
cleanup oracle source is unchanged. Deep test evidence remains fake backend,
not native input proof.

Newton's final targeted review independently closed the post-exit drain grace
finding and found no important defects in the stdio observation seam, passive
journal or adapted retained-handle + available-status oracle. All reviewed
inputs are covered by the final source freeze.

Final compile-only checks: examples release build36.39s, all-test release no-run
build1m19s, both exit0. Scoped rustfmt check covers21 changed/new Rust files,
exit0; git diff whitespace check clean. No test execution occurred locally.

A final-v2 CC attempt with requested shell output redirection was denied by CC's
local write guard before running any tests (`cc-green-final-v2.jsonl`). No security
setting was changed or bypassed. The next invocation requests ordinary unredirected
runner execution, as previously authorized; the runner itself retains evidence.
The failed invocation's transcript remains preserved separately.

## Final v2 Windows regression results

The complete11-suite rerun (`green-final-v2/summary.json`) completed with native
exit0 for every suite: **470 passed,0 failed,8 ignored**. Compared with v1,
runtime_lib is now330 passed (the five shutdown regressions); all other per-suite
counts above are unchanged. Final artifacts are frozen in `final-v2-artifacts/`
with hashes in `final-hashes-v2.json`; original remote candidate v1 retained in
`tests-v1/`. The tested remote v2 bundle is under `tests/`.

`cc-green-final-v2-attempt2.jsonl` actual assistant model is glm-5.3-flash;
CLI also completed successfully, exit0/is_error:false. No source changes since
`source-freeze-v2.json`. Deep process/EOF suite execution is tracked separately.

Deep attempt `deep-green-v1` failed before native execution: Windows rejected the
oversized SSH `-EncodedCommand` with "The command line is too long". No runner
summary/native exit exists, and evidence-copy path was therefore absent. This is
an orchestration setup failure, not a failed product test. QA is changing only
the deep runner's script transport to a short file invocation; Rust/binaries stay
frozen. Retry will use a fresh phase, retaining this attempt.


## Final deep process results and delivery boundary

`deep-green-v2/summary.json` and raw runner stdout confirm:
- windows_stdio_eof:6 passed,0 failed,4.99s.
- windows_tcp_disconnect:4 passed,0 failed,6.31s.
- Both native exit0, no timeout; actual CC assistant model glm-5.3-flash,
  CLI is_error:false. Executed test hashes match frozen-deep-v1.json.

These use actual Windows processes, pipes and TLS with journaled fake input;
they prove the asserted early cancellation, held-key ledger cleanup and retained
process-handle retirement in that fixture, not OS/DingTalk behavior. The TCP
retirement oracle also confirms broker available; no oracle weakening was needed.

**Final total across13 suite executions:480 passed,0 failed,8 ignored.**
Only the8 named ignored cases above are skipped; macOS/Linux testing is pending
by policy. General test executable hashes all match final-hashes-v2.json. Final
read-only remote inventory confirms all13 v2 bundle hashes match and no process
whose executable is under this owned test root remains. This is scoped residual
verification, not proof about arbitrary unrelated descendants. Original deployed
host was not replaced and no user process was killed.

Final remote `quser` still reports `No User exists for *`. Native GUI/IME/paste
acceptance is therefore pending a logged-in interactive desktop. No fake-backend
result is represented as real Chinese-input success. The explicit paste strategy
is retained; native IME injection is not claimed fixed.

Candidate host: `C:\Temp\rpa-session-control-20261014\tests\computer-host.exe`.
Local frozen bundle: `.agents/runs/session-control-20261014/final-v2-artifacts/`.
Final source/Cargo hashes remain identical to source-freeze-v2.json. Scoped
formatting and final git diff whitespace check pass. No commits, pushes,
worktrees, global Claude settings changes, firewall edits or login/session changes.

## October14 interactive follow-up after user login

See `.agents/reports/control-gui-20261014.md` for preserved raw evidence.
Actual CC+GLM native Windows lifecycle run passed conflict-with-live-transport,
pause retaining ownership, resume invalidation, close/release, same-B-transport
handoff and stale-A-close isolation (15 lifecycle calls). No GUI typing was used
for that lifecycle proof.

A real paste attempt found a separate clipboard prewrite-zero bug, repaired with
a minimal locked-baseline policy change while retaining strict postpublication
guards. Fresh Windows regression: library335 passed/0 failed/7 ignored and
paste contract12 passed/0 failed. Four subsequent native GUI paste calls all
delivered exactly the submitted text. Screenshot-only fixture task was1/4,
because three model-submitted payloads misread punctuation/spacing. Do not count
those as fixture passes. Full-width IME was not verified at Check; DingTalk was
not operated. Original deployment unchanged; no commit/push.

## Later October14 completion of previously pending coverage

See `.agents/reports/control-followup-20261014.md`: actual DingTalk self-chat
explicit paste + one Enter send verified; all7 previously ignored library tests
explicitly passed in their required Session0/Session1 contexts. New bounded
fixture trace observed open=true/conversion0x9 before AND after WM_PASTE in all
four cases, with exact submitted-text readback4/4 and no dropped trace samples.
Check-time0x401 was a later state and did not represent paste-time state.
Screenshot-only transcription is still not all green: new run2/4, with the two
remaining model-submitted text errors preserved. Previous failures/history here
are not overwritten. Original deployed host was not replaced.
