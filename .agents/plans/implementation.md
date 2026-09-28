# Cross-platform Computer Runtime Implementation Plan

> For agentic workers: implementation is delegated exclusively to Claude Code. Use test-driven-development, isolated write scopes, and review checkpoints. No worktrees, commits, or pushes. The coordinator writes design/task Markdown and reviews, but does not implement product/test code.

**Goal:** A real computer-use Runtime with compiled macOS and Windows Computer Host, driven by source-isolated Claude Code through MCP, verified on local macOS and interactive acer-win Windows.

**Architecture:** Native platform backend beneath a cancellation-aware session Runtime. Thin stdio MCP executable embeds Runtime; Windows remote test uses the same executable with authenticated loopback transport and SSH forwarding. Existing Claude Code supplies the Agent; no custom model API loop.

**Tech stack:** Existing Rust toolchain/crate; OS screenshot/input primitives; stdio MCP JSON-RPC; Python stdlib only for test/deployment bridges where helpful; user-configured Claude Code.

## Task 1: Freeze interface and provision workers
- [x] Inspect existing source, independent Git state, local Claude CLI and toolchain.
- [x] Confirm configured Claude responds without changing model configuration.
- [x] Write `.agents/CONTRACT.md` and file ownership.
- [x] Dispatch core/backend/transport workers with disjoint files, plus QA test/deployment worker.
- [ ] Record live process handles and final status under `.agents/runs/` and reports.

## Task 2: Core Runtime (core worker)
Files: Cargo.toml/Cargo.lock/src/lib.rs, src/runtime/**, tests/protocol.rs, .gitignore; remove obsolete example/API only after migration.
- [ ] Write failing deterministic tests: image actual size, coordinate map, invalid chord has zero events, stale observation, partial input, capture error after dispatch, cleanup failure, request replay/conflict, cancellation during hold.
- [ ] Implement API exactly as CONTRACT; module functions rather than a >1000 line file.
- [ ] Run `cargo test --lib`, `cargo test --test protocol`, formatter; report commands and counts.
- [ ] Integrate dependency requests, keep native deps cfg-correct; do not overwrite peer files.

## Task 3: Native backend (backend worker)
Files: src/backend/**; tests in modules.
- [ ] Fix physical/image/native coordinate distinction; use real PNG dimensions.
- [ ] Correct wheel direction and key chord event semantics on both macOS/Windows.
- [ ] Implement capture/geometry/permission diagnostics, pressed-state cleanup, fail-fast geometry change.
- [ ] Prove compile on macOS; participate in Windows build once toolchain available.
- [ ] Do not perform desktop input during implementation; coordinator schedules actual tests.

## Task 4: MCP and Host (transport worker)
Files: src/mcp/**, src/bin/**, examples/*.json.
- [ ] Implement documented protocol handlers and image results; no source/file/arbitrary-command tools.
- [ ] Add separate I/O reader and runtime worker for cancellation, cleanup on EOF, bounded frames, lifetime watchdog.
- [ ] Enforce cross-process desktop writer exclusion and explicit local stop behavior.
- [ ] Implement loopback-only authenticated transport for interactive Windows Host; no LAN listener or firewall weakening.
- [ ] Supply `computer-host --help` usage; no changes to global Claude configuration.

## Task 5: Independent tests and packaging (QA worker)
Files: tests/runtime_contract.rs, tests/mcp_protocol.py, scripts/**, README.md, test prompts.
- [ ] Add black-box protocol tests and white-box failure-injection tests independent of implementation tests.
- [ ] Plan/build Windows binary; install only scoped build tooling if needed, don't alter unrelated apps.
- [ ] Deploy only release artifacts to Windows test directory; GUI Host starts via interactive scheduled task (same explorer session, not SSH Session 0).
- [ ] Create remote bridge, package manifest/hash, startup/shutdown scripts with explicit test ownership.
- [ ] Create source-free local release directory and negative filesystem isolation probe; no credentials in bundles/logs.
- [ ] No real desktop input by worker; report exact commands for coordinator.

## Task 6: Coordinator review and fix iterations
- [ ] Inspect source, not merely worker summary. Trace side effects and cleanup branches.
- [ ] Run `cargo fmt --check`, `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings`.
- [ ] Run independent protocol/fault tests, evaluate tests for false positives and test coverage gaps.
- [ ] Dispatch all implementation fixes to Claude Code with concrete evidence; coordinator does not patch code.
- [ ] Build release macOS and Windows binaries; record target triples and hashes.

## Task 7: Source-isolated end-to-end testing
- [ ] Local release environment contains no source and test Claude has only computer MCP tools.
- [ ] Verify OS denial of reading original project source; prompt/tool restrictions alone insufficient for claimed file isolation.
- [ ] First controlled test screen: prove image parsing and coordinates, not a text hint carrying answer.
- [ ] macOS calculator 10*20 with actual UI trail and screenshot; native text editing with Unicode.
- [ ] Windows Host process matches explorer interactive session; calculator and text editing through its compiled binary.
- [ ] Cancellation during hold/drag, permission/capture failure, geometry invalidation and disconnect/cleanup behavior verified at appropriate layers.
- [ ] Ten trials each task per target where environment permits; at least 8 real successes; report actual raw counts, never fake missing trials.
- [ ] Zero unreported stuck-input state / false completion. Any block reported as incomplete, not waived.

## Task 8: Deliverables and completion audit
- [ ] All planned public tools and constraints have implementation + direct test evidence.
- [ ] README gives setup, permissions, compilation, local MCP, remote test path, stop controls and known limits.
- [ ] Evidence manifest includes file paths, commands, versions, hashes, frame ids, sanitized traces; no model credentials.
- [ ] `.agents/reports/final-audit.md` maps original user requirements to evidence; unmet requirement keeps goal active.
- [ ] No commit/push or modifications outside agreed project/deployment scope.

## Source of truth and limitations

The existing spec's single-platform-first milestones are superseded for THIS goal: both macOS and Windows binaries/validation are required. Windows transport is an extension justified by the explicit user request, not imported from parent RPA services. Parent RPAD/Executor are not dependencies. Maintain goal until cross-platform and source-isolated Agent evidence is real; TCC/locked desktop/toolchain availability may require reported user action, never fabrication.
