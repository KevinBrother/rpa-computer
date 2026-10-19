# Launcher GREEN candidate — 2026-10-16

**Not native acceptance. No consumer migration.** This revision implements the
observed policy/tracker/native RED. Independent reports and raw final logs read:
`2026-10-15-linux-credentials-gates-1.md` (pure26 GREEN, policy12/tracker9
assertion RED, six native assertion RED) and
`2026-10-15-linux-credentials-highfd-1.md` (remaining two actual assertion RED
under the explicit fixture limit). Original test sources/assertions are unchanged.
Earlier CHECKPOINT.md and immutable RED/pure archives remain historical evidence.

## Independent CC commands (not executed by implementation worker)

Use a fresh manifest-verified snapshot and actual source rebuild; copied Cargo
artifacts with newer mtimes previously caused a stale-binary incident. Include
**both native/linux/leaf_*.S files** in source packaging; Rust include_str! needs
them. Run each command separately, retaining its log/exit code/test inventory.

```sh
cargo-1.89 test --locked --offline --lib --no-run
cargo-1.89 test --locked --offline --lib x11_credentials::api_tests -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib x11_credentials::auth_tests -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib x11_credentials::ipc_tests -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib x11_credentials::display_tests -- --nocapture --test-threads=1
timeout 120 cargo-1.89 test --locked --offline --lib x11_credentials::launcher::policy_tests -- --nocapture --test-threads=1
timeout 120 cargo-1.89 test --locked --offline --lib x11_credentials::launcher::tracker_tests -- --nocapture --test-threads=1
timeout 120 cargo-1.89 test --locked --offline --lib x11_credentials::launcher::native_tests -- --ignored --nocapture --test-threads=1
```

Inventories: pure6+5+10+5=26, policy12, tracker9, native8 scenarios +1 dispatcher.
Inventories are not predicted/observed pass results for this candidate.

Native scenarios require an independently owned disposable Linux container with
its original hardened/default seccomp profile, no GUI/X/input, no extra
capabilities, and **per-container `--ulimit nofile=8192:8192`** (record measured
soft/hard values). The fd4096 fixture must not be weakened. Prior default soft1024
caused EMFILE before reaching the product; it is infrastructure, not RED. No
product setrlimit, global/system limit or profile modification is authorized.
Keep the existing outer process/container timeout and final owned cleanup.

Record actual `NATIVE_PATH` and `ACTUAL_CLONE3_ERRNO` from the fd-auto checkpoint.
An explicit LegacyOnly fixture is not evidence of genuine automatic fallback.
If the unmodified environment denies both launch mechanisms or required child
setup, preserve the failure; never skip it or relax the environment to claim GREEN.

## Implementation and review map

- `policy.rs`: original reservation, deadline checks, allowlisted single fallback,
  exact parent-mask restoration, validate before startup, irreversible timeout,
  parent-only parsing, separate positive completion prerequisite and cancellation.
- `tracker.rs`: four total starting/attached/Pending/Unknown slots, generation
  identities; no credential/path/token state. Fixed-size metadata snapshot;
  no syscalls, wait or handle destructor under the metadata lock.
- `mod.rs`: one retained process-lifetime reaper, no per-attempt thread, only
  nonblocking observations of tracked pidfds. It also observes active children;
  the native handle caches our positive wait to coordinate foreground/reaper
  races. A late foreground observation never alters a reused slot.
- `native/linux.rs`: parent-prepared path/buffer/socketpair, raw kernel signal
  mask (including libc-reserved signals), clone3 then one raw CLONE_PIDFD legacy
  fallback on ENOSYS/EPERM/EACCES only; no libc fork/clone callback, exec or
  pidfd_open. Incomplete/invalid handle never permits startup or a signal.
- `native/linux/raw.rs`: separate x86-64/ARM64 syscall register ABI and clone
  trampolines. Child return branches to leaf before any Rust parent frame.
- `native/linux/leaf_{x86_64,aarch64}.S`: assembly-only child in debug AND release;
  no allocator, destructor, handler restoration, TLS, libc or external call.
  PDEATHSIG then parent PID/creating TID liveness check; retain only private fd3
  with full close_range, then startup gate BEFORE path operations; regular-file
  check, cap including post-stat growth, bounded frame, raw exit. All call/bl
  targets within the leaf are private assembly checkpoint/send/permission labels.
  Linux struct offsets/syscall constants come from leaf.rs and target libc, not
  guessed cross-architecture layout.
- `native/linux/{process,transport}.rs`: parent-only pidfd signal/nonblocking
  waitid(P_PIDFD) and absolute-deadline nonblocking private IPC. ECHILD alone is
  Unknown. A kill request never means Reaped. Cancellation revokes IPC first.
- `native/fixture.rs`: existing probe API moved intact; observers run only in
  the parent. Raw child checkpoints send a byte and wait on fd3; no Rust closure.
- `native/unsupported.rs`: other OS/unsupported architectures remain explicitly
  unavailable, with no synchronous file read or libc process fallback.

## Mandatory release gates still OPEN

Independent raw source/assembly review of BOTH ABIs and real Linux containment
are required. Compilation and assembly inspection are not runtime evidence.
The original eight fixtures do not close all required cases: deterministic
pre-prctl / post-prctl-parent-check death, creator-thread death (including TID
reuse and the spawn-to-tracker handoff), setup/close_range failure before reads,
post-stat file growth, partial/crashed native frames, denied-launch parent-mask
restoration, competing reapers, and actual FD/thread/process/capacity bounds.
The TID signal-0 check is a liveness check, NOT a stable identity capability;
only owned pidfds authorize child termination. Missing/uncertain ownership stays
bounded and fail-closed, not claimed retired. No D-state/immediate disappearance
claim. No consumers until coordinator authorizes after those gates.
