# Continuation review 4 — coordinator, 2026-09-24

## Scope and accountability
All implementation/tests/scripts remain written by real local Claude Code, configured model unchanged. Coordinator only reviews, writes planning/evidence Markdown, dispatches and executes verification. No worktree, commit/push or parent repository changes.

## Verified this continuation
- qa-lint-1 exec95117 terminal exit0; report reviewed against all five changed test files. Changes are comment indentation and redundant conversions, no weakened assertions.
- Independent `cargo test --test runtime_contract --offline` exec89418 exit0: **35passed, 0failed, 1ignored**,10.55s. Log `coordinator-contract-tests-4.log`. Registry1000 ignored test already separately passed in review3; not claimed as newly run here.
- Same run exposed unfulfilled `expect(dead_code)` in worker.rs; sent sole transport writer. A passing test exit is not a passing warnings-as-errors check.
- Read-only desktop probe `coordinator-lock-state-4.log`: macOS `CGSSessionScreenIsLocked=true`; Windows explorer8648 and LogonUI25084 remain Session1. No input, unlock, native Host, or repeated failing visual Agent attempted.
- Two historical fakeCLI test groups leaked from earlier QA diagnostics were identity-checked against exact paths, PID/PGID/start timestamps and descendant marker, then stopped. Groups87174/96938 have no remaining live members. Evidence `coordinator-owned-old-fixture-cleanup-4.json`. No name-based killing or unrelated process changes.

## Active writers
- transport-fix-3 exec28969 / FIFO `.agents/runs/transport-fix-3.in`: ONLY src/mcp/**,src/bin/**,examples configs, own report. Continuing reader-side cancellation and actual transport regression tests. Do not create overlapping writer.
- qa-runner-lifecycle-4 exec90311: ONLY watchdog-launch.py,run-blackbox-claude.sh,test-release-scripts.sh, own report. Bounded task document `.agents/tasks/qa-runner-lifecycle-4.md`; normal-exit and interrupted-launcher descendant cleanup plus exact sandbox profile evidence. No GUI. Existing QA3 and core2 writers stopped as recorded in review3.

## Concrete transport review feedback already delivered
1. parse_line use now visible, but parser did not validate jsonrpc version/params. Invalid envelope must not trigger control.
2. First overflow implementation emitted unlimited overflow markers into unbounded control queue; stdio remained in infinite refused/drain state. Require one-shot refusal+termination, no forgotten pending clients; TCP reader socket shutdown; oversized queue overflow must not vanish silently.
3. Real production-loop/socket tests required, not only duplicated TestHarness channel tests.
4. Existing scoped tests and 206-pass old full snapshot are not proof new source is final/green. Freeze then rerun full tests+clippy+fmt+protocol, Windows check/release and macOS release.

## Acceptance truth
Source-isolated real Claude Windows read-only preflight already ran (review3, blackbox-readonly-preflight-1 report); eight tools only, 17policy checks, open/observe errors/close. No image, no computer_step, no GUI-success count. Native Windows binary used was diagnostic snapshot, not final deliverable. Real GUI task success remains **0**. Original ten-trial per-task/per-platform gate is retained.

## 20:55 independent snapshot gates (not final freeze)
- exec70430 `cargo test --all-targets --offline` exit0: lib162passed1ignored + protocol12passed + contract35passed1ignored = **209passed,0failed,2ignored**. `coordinator-all-tests-3.log`.
- `cargo clippy --all-targets --offline -- -D warnings` exit0 and `cargo fmt --check` exit0, explicit status file `coordinator-lint-status-3.log`. Unlike older combined commands neither failure is masked.
- exec16758 macOS release build exit0,34.15s: sha256 `094a8f78034411009347b887219dde61808ad3e58faad4f6489263f230923814`.
- exec90656 Windows xwin release build exit0,43.15s: sha256 `fac80c497590f280550e063dbd0dfe8e4f06f903c7c524e4d3a41af470ba02e4`. Three mac-only hotkey helpers still warn under Windows cfg; transport owner notified. Both builds are candidate snapshots while hardening continues, not final release acceptance.
- readme-closeout-1 exec84989 owns ONLY README/report; fixes stale packaging default, migration language and Windows privilege overclaim. No overlap with transport or QA runner.
- exec19355 independent script suite exit0: **81passed0failed** (`coordinator-release-scripts-4.log`), includes newly added normal-exit + TERM/INT descendants cleanup and persisted exact profile/hash. This is fakeCLI/OS-isolation evidence, not GUI.
- **exec18145 release MCP harness exit1:49passed1failed1explicit-skip**, `coordinator-mcp-protocol-3.log`. observe(wait_ms3000)+immediate cancelled notification returned clean image/isError=false. Root review: ActiveRequest only set by main on dispatch, reader can consume request+cancel before that; main processes cancel after clearing active, so both paths lose it. Sent sole transport writer; cannot declare release ready based on 209 unit tests.

## Writer handoff and documentation closure
Transport3 remained in context compaction20:52–21:00 (178k context); coordinator verified exact process identities and absence of children, stopped only CLI83790 and its FIFO keeper83787. exec28969 confirmed143. Replaced with focused transport-closeout-4 exec56365 **only after terminal state**, same local Claude/model, all source preserved, max70turns. Prior messages consolidated in tasks/transport-closeout-4.md. No simultaneous transport writers.
QA runner exec90311 confirmed0 and frozen. README1 exec84989 hit max-turns exit1; its changes preserved but stale temp-root-ban prose remained; readme2 exec75358 bounded documentation-only correction launched after terminal state.
