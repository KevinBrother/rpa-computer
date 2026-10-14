# Desktop Control Manager Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development. No commits,
> pushes or worktrees; project policy overrides skill defaults.

**Goal:** Multiple persistent TLS clients share one task-scoped desktop executor
with explicit conflicts and verified safe handoff.

**Architecture:** Shared authority state machine in remote host; separate task
executor process using existing stdio runtime. MCP transport lifecycle is decoupled
from native task lifecycle. OS process-level lock remains the final cross-host
exclusion; no idle client creates an executor.

**Tech Stack:** Rust, existing rustls/MCP framing/process infrastructure; Windows
cross compilation; actual Claude CLI + GLM independent Windows validation.

## Ownership and files

- Implementation worker: `src/mcp/remote/**`, narrowly necessary supporting
  `src/mcp/{jsonrpc,server}.rs`, `src/bin/computer-host.rs`; own unit tests.
  Split authority, executor and connection handling into focused new modules.
- Contract worker: `tests/remote_transport.rs`, new
  `tests/remote_session_control.rs`, `tests/support/**` only when needed.
- Coordinator: spec/plan, README, evidence reports, build/test orchestration.
  Existing clipboard implementation stays untouched unless a reviewed integration
  issue specifically requires it.

## Steps

- [x] Capture baseline git state and pre-change Windows host/client artifacts.
- [x] Author regression contract before changing implementation. Connect clients
  A and B; initialize both and list tools; A opens; B open must return
  `error.code=lease_conflict` as a tool error, not EOF; A close succeeds; B can
  open on the SAME connection; stale A/session calls cannot mutate B.
- [x] Cross-compile that contract against baseline and execute on Windows through
  CC + GLM, retaining the expected failure evidence.
- [x] Implement centralized available/acquiring/owned/cleaning/quarantined state
  with unique connection/task identities. Unit tests exercise racing acquisition,
  stale release, cleanup failure and panic/unknown fail-closed transitions.
- [x] Replace TLS single-active-connection gate with a bounded multi-client front
  end. Authentication/framing bounds and shutdown responsiveness remain intact.
- [x] Implement lazy owned child per admitted task and protocol initialization;
  reuse existing native worker/hotkey/feedback in child, preserving OS lock.
- [x] Separate discovery/status handling from control requests. Route requests
  only for owning transport/session; return structured conflict/cleaning/fault
  errors for other clients, never delay execution silently.
- [x] Retire executor on valid close, EOF or lifetime expiry; require clean
  cleanup and confirmed exit before release, otherwise quarantine. No force
  kill/exit success is treated as proof of cleanup. Preserve prioritized cancel.
- [x] Add deterministic fault, repeated reopen, invalid close, pause/timeout,
  slow peer, per-client cancel isolation and admission bound coverage.
- [x] Build with `cargo check --tests --locked --target x86_64-pc-windows-msvc`;
  build optimized test executables (`cargo test --release --no-run --locked
  --target x86_64-pc-windows-msvc --message-format=json`) using existing toolchain.
- [x] Run new and existing Windows tests via bounded remote runner, launched
  by genuine `claude --model sonnet`, verifying actual assistant model tags
  `glm-5.3-flash`. Distinguish mock transport checks from native GUI proof.
- [x] Independent spec/safety review; repair findings and rerun affected tests.
- [ ] Only when old desktop ownership is free, use owned isolated host and
  screenshot-only CC + GLM fixture acceptance; never terminate user processes
  without authorization. Otherwise report GUI gate explicitly pending.
- [x] Document lifecycle/status semantics and exact verification evidence;
  `git diff --check`, scoped formatting, preserve all user changes. No commit.


## Final verification checkpoint

Implementation and independent source review complete. Actual CC+GLM Windows
runs:13 suites,480 passed,0 failed,8 explicitly ignored; source and binary hashes
frozen. Includes real process/pipe/TLS disconnect tests with fake input backends.
See `.agents/reports/desktop-control-manager-20261014.md` for exact evidence,
preserved failed setup attempts, and remaining coverage limitations.

The only unchecked delivery gate above is native desktop fixture acceptance:
acer-win has no logged-in interactive user. Original deployed host is untouched;
new host is staged separately. No Mac/Linux testing, commits, pushes or worktrees.
