# Control takeover checkpoints — 2026-10-15

Only `src/x11_credentials/**` changed. Root lib registration and consumers are
unchanged; the eight-file control socket GREEN remains frozen. No tests, SSH,
GUI, process-launch fixtures or syscall probes executed by this implementation
worker. Compilation is not test evidence.

## 1. Pure GREEN candidate (separately archived)

`credentials-pure-green-1.tar.gz` / `.sha256` under
`.agents/runs/linux-parity-20261015/` preserves the pure checkpoint BEFORE launcher
RED registration. Archive SHA256:
`d3295f323909782b52cc549c3848e5ba90a38d01231f3229cdb43cfa4624741b`.

Independent original26RED: cc-continuity-helper-red-1/probe.log reports
1PASS/25 assertion FAIL, exit101. All26 original API/auth/IPC/display assertions
are unchanged. Implementation adds c_int-safe decimal normalization (including
>100000), bounded complete-stream binary auth parsing, sticky deadline/framing
IPC policy, and pure API validation/no-path success. Valid pathname requests
remain LauncherUnavailable/NotStarted: no synchronous read, thread, process,
socket, environment lookup or native constructor is used to make them succeed.

## 2. Production-connected launcher/tracker RED candidate

`acquire(Some(path))` now enters `launcher::acquire` -> `policy::run` with the
native adapter and a single process-lifetime `Tracker`. The SAME policy::run is
called by scripted policy tests. It deliberately returns LauncherUnavailable;
tracker methods deliberately return Unimplemented. The native adapter makes NO
raw syscall and reports `Spawn::Unavailable`, not an invented kernel errno.
No partial raw launcher, child leaf or reaper GREEN is hidden in this checkpoint.

- 12 portable policy tests: fallback allowlist/exactly-once/original reservation,
  mask restoration, no retry for resource failure, deadline before fallback and
  startup, missing/invalid atomic identity, capacity-before-launch, late reply /
  parse rejection, kill-vs-reap outcomes, stale-identity rejection.
- 9 real-tracker boundary tests: capped starting/quarantined slots, Pending /
  Unknown retention, ECHILD not proof, generation identity, no-handle uncertainty,
  positive completion releasing exactly one slot, no Unknown downgrade.
- Linux ignored fixtures: 8 scenarios plus1dispatcher helper. Fresh disposable
  test process per scenario (harness spawn is NOT product launch). Public default
  acquire; internal raw-legacy selector; low/high/stdin/stdout/stderr closure at
  pre-read gate (including fd3 already occupied); blocked atfork prepare;
  shared-memory inherited-handler marker + exact parent-mask restoration;
  syscall-only before-read/before-frame holds with bounded return and independent
  pidfd-exit observation. A signal-handler pipe could falsely pass after FD
  closure, so the handler oracle uses shared memory instead.

Native fixture configuration is test-only, enters the same policy/native boundary,
and remains unimplemented. Future child checkpoints must be raw syscall-only
precomputed markers, never Rust closures. The observer callback runs in the parent
only. No environmental syscall denial is injected; explicit LegacyOnly tests and
genuine automatic fallback evidence must be reported separately. Unsupported
launch is assertion RED, not a skip or successful containment claim.

## Required later work before consumer migration (not covered/approved here)

After independent new assertion RED, implement/review raw clone3 and exactly one
raw CLONE_PIDFD fallback for definitive ENOSYS/EPERM/EACCES with a valid atomic
pidfd and original budget. No libc fork/callback clone, exec, fork+pidfd_open,
bare-PID signals, flag stripping or seccomp changes. Both architecture-specific
raw child paths require source/assembly review (x86-64 and ARM64 separately).

Additional native release gates remain beyond this first executable fixture set:
deterministically ordered pre-prctl / post-prctl-parent-check death races,
creator-thread death, failed child setup/close_range before file open, post-stat
file growth, native crash/partial-frame faults, real mask restoration on denied
launch, and actual reaper/thread/FD/process-count bounds with competing reapers.
PDEATHSIG+parent-PID race handling is mandatory, not optional future polish.
Native tests here do not simulate uninterruptible D-state or prove immediate
kernel disappearance; quarantined outcomes remain owned until observed completion.
No consumer may migrate based only on pure or scripted-policy tests.
