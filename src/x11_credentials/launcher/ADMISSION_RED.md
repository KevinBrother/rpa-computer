# Original-creator admission / no-GO RED checkpoint — 2026-10-16

Tests only on credentials-permit-green-1. Production/raw/transport/observation
seam behavior is unchanged. Exactly one existing source file changes: launcher
mod.rs registers the Linux x86-64/ARM64 test module. All previous test sources,
including original actual-creator cases and permit supplements, are unchanged.
No tests, SSH, GUI or commit executed here; no consumer migration.

## Inventory: three real cases + one dispatcher

1. `no_go_exits_at_original_deadline_without_parent_cancellation`
   Intended new assertion RED, not yet independently observed. Actual clone
   creator reaches existing BeforePermissionWrite only after consuming stage1
   (ARMED) from the actual child after prctl+FD closure. Keep creator held there:
   no GO, no parent cancellation. Original deadline=800ms; sample pidfd readiness,
   SIGCHLD cause, path operations and held-parent state by deadline+500ms BEFORE
   releasing parent or fixture cleanup. Require child raw rejection exit127, not
   SIGKILL or merely an eventually failing caller. Missing kernel cause is INFRA.

2. `original_creator_death_before_prctl_never_opens_authority`
   Real baseline, not automatically a RED claim. Hold the actual creator at
   CloneReturned and its child BeforePrctl. Terminate ONLY that actual clone
   thread using the existing non-unwinding fixture seam; confirm thread exit
   before releasing child. Original budget=5s. Require no path operation and
   child fail-closed exit by that deadline+500ms. No promise of retroactive
   PDEATHSIG or immediate death discovery. No creator-ownership recovery claim.

3. `actual_reused_tid_cannot_supply_original_creator_go`
   Same real pre-prctl death, then bounded REAL Linux thread churn. Only an
   actually matching SYS_gettid in the SAME SYS_getpid group qualifies. Keep the
   new thread alive across child admission/observation; it receives no ARMED,
   writes no GO, and never invokes any credential launcher. Child then has the
   original TID numerically live but no original creator stack to send GO.
   Require no path operation and self-rejection by the ORIGINAL 5s deadline.
   No injected boolean/TID syscall substitute or fixture-forged GO is used.

   Churn budget: 2s, at most4096 created threads, at most ONE live replacement/
   churn thread at a time, 128KiB requested stack, one bounded identity channel.
   No next spawn after budget; identity wait is min(300ms, remaining budget),
   and a finishing thread gets at most300ms cleanup grace before INFRA. No
   unbounded join: observe is_finished under a bound first. All cases have an
   outer disposable-host12s limit; blocking OS fixture setup is not product RED.

   If reuse is not obtained: cleanup the observed child without behavioral
   assertions, print ADMISSION_NOTQUALIFIED, inner exit77. Outer test deliberately
   fails with NOTQUALIFIED, never prints CREDENTIAL_ADMISSION_OK, and must NOT be
   counted as a PASS or behavior RED. Exhaustion/spawn/setup errors are INFRA,
   not "reuse absent". No pid_max/ns_last_pid/PID namespace/global/security/kernel
   changes, privileges or environment replacement are authorized.

4. `isolated_admission_scenario` is only the subprocess dispatcher, not another
   behavioral proof; when run without its case selector it returns immediately.

## Who terminated the child? Evidence boundary

Each disposable host installs a parent-only SA_SIGINFO SIGCHLD observer before
launch. It writes lock-free atomic si_pid/si_code/si_status only; it never waits,
reaps, allocates, formats or signals. No SA_NOCLDWAIT. The raw child still inherits
blocked handlers and executes no callback. Normal test exit restores old handler.
There is exactly one product child in the host, avoiding merged multi-child cause
attribution. A missing cause is fixture INFRA, never a guessed timeout success.

Snapshot before any fixture kill or parent release is logged as
ADMISSION_OBSERVATION. CLD_EXITED/status127 distinguishes raw self-rejection from
CLD_KILLED/SIGKILL by parent, PDEATHSIG, reaper, or fixture; only actual product
waitid counts as positive_reaps. Fixture observer uses duplicated actual pidfd;
its later bounded kill/wait cleanup is explicitly excluded from acceptance.

IMPORTANT: an exit127 could also follow revoked IPC/EOF. On this exact frozen
production source, the held creator cannot reach revoke_ipc/cancel, and the sole
reaper in launcher/mod.rs ONLY calls Handle::observe + Tracker::observe. It does
not signal or shutdown IPC on deadlines. That source invariant is part of the
no-GO acceptance evidence; exit status alone is NOT a general cause oracle. If
future reaper code can revoke IPC, add explicit revocation/cause observations or
otherwise isolate that action BEFORE accepting this test as child-owned expiry.
Never silently count a reaper-induced EOF as the child's clock/deadline fix.

The current stage1/0xa5 protocol is structurally close to ARMED/GO; these tests do
not pretend every protocol property is missing. Real sender/receiver identities
must still be audited: only the original actual clone call stack may authorize
GO, never reaper/replacement. This checkpoint adds no runtime sender-ID seam and
claims no complete stable-thread-handle, caller-death or whole-host-death proof.

## Independent CC commands (separate log and exit code for EACH)

Use fresh integrated source build; preserve actual model/tool identity in reports.
No special kernel/security changes. Original high-FD regressions still need only
per-container nofile=8192:8192; these new cases require no FD4096 sentinel.

```sh
cargo-1.89 test --locked --offline --lib --no-run

timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::no_go_exits_at_original_deadline_without_parent_cancellation \
  -- --exact --ignored --nocapture --test-threads=1

timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::original_creator_death_before_prctl_never_opens_authority \
  -- --exact --ignored --nocapture --test-threads=1

timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::actual_reused_tid_cannot_supply_original_creator_go \
  -- --exact --ignored --nocapture --test-threads=1
```

Do not reinterpret zero matches, timeout, unsupported launch, fixture failures or
NOTQUALIFIED as assertion RED. Run ordinary regressions using PERMIT_GREEN.md and
EXPIRY_GREEN.md; their prior snapshots/evidence remain separate and immutable.

## Compilation / remaining gates

macOS root all-targets and actual-source Linux helper --tests on both ABIs are
compilation-only checks. Logs: credentials-admission-red-check-*.log under
.agents/runs/linux-parity-20261015. Initial harness compile found a stray semicolon
making a waitid-result block return (); corrected before freeze, initial error
log retained separately. No runtime result claimed.

Raw release publication remains a design candidate, not implemented here. The
original three creator ownership failures, real qualified TID reuse, whole-host
death and both-ABI native evidence remain mandatory. Do not change no-GO product
behavior before independently observed assertion RED for this checkpoint.
