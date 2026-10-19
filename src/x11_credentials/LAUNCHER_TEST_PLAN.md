# Launcher / containment tests-first plan — 2026-10-15

> Checkpoint update: pure implementation and the next production-connected launcher/tracker RED scaffold are now authored, not executed. See `CHECKPOINT.md`. The original status/evidence statements below describe the pre-takeover checkpoint; launcher/containment requirements remain unchanged.

**Plan only.** No launcher, child leaf, reaper or test execution is implemented
by this document. Implementation follows independent assertion RED and review.
The archived hotkey + helper19 CC run remains separate from this follow-up.
No consumer migration until native credential-helper containment is verified.

## Layer 1: portable parent-policy seams (new RED checkpoint before GREEN)

Use scripted syscall *results* only for ordering/error policy. These assertions
cannot prove that a syscall, raw child path, FD closure, kill or reap occurred.
The eventual production launcher must use the same policy boundary, not a
parallel test-only implementation. Each test records an ordered call trace.

| Proposed assertion | Scripted inputs | Required result |
|---|---|---|
| clone3_success_never_attempts_legacy | valid pidfd + child identity | one clone3 call, same reserved slot, no fallback |
| definitive_denial_tries_legacy_once | clone3 ENOSYS/EPERM/EACCES; legacy valid pidfd | exactly one legacy CLONE_PIDFD|SIGCHLD attempt |
| resource_or_argument_failure_does_not_retry | EAGAIN/ENOMEM/EINVAL | no legacy call, typed failure |
| exhausted_deadline_before_fallback_prevents_spawn | denial then deadline checkpoint expires | Timeout, original budget, no legacy call |
| both_paths_denied_are_bounded_failure | both unavailable/denied | no third attempt, no profile/capability mutation |
| successful_pid_without_pidfd_never_retries | child PID returned, fd sentinel unchanged | no new spawn, no bare-PID signal, startup gate revoked, owned Unknown |
| fallback_reuses_original_reservation | denial then legacy success | one slot, no credential/state duplication |
| capacity_exhaustion_precedes_any_launch | all four slots occupied, including Unknown | CapacityExhausted, no spawn or new reaper thread |
| startup_gate_requires_valid_handle_and_live_deadline | valid/invalid pidfd; deadline before/after check | startup byte only for valid unexpired attempt |
| timeout_wins_after_full_reply_before_final_accept | barrier around final acceptance | no cookie, bytes discarded, original timeout retained |
| timeout_wins_after_parent_parsing_before_accept | barrier after successful parser | no cookie and irreversible Timeout |
| cleanup_outcomes_are_not_promoted | kill error, NotReady, ECHILD, observed reap | Pending/Unknown retained unless positive completion evidence |
| stale_identity_cannot_signal_or_release_new_child | same PID, different ticket/generation/pidfd | StaleIdentity, no signal/slot release |
| tracker_stores_no_credentials_or_grants | metadata-only tracker API | no cookie/path/desktop/session/cancel state in tracker |

Add explicit ordering assertions: reserve capacity -> prepare bounded inputs ->
block invoking-thread signals -> raw spawn -> restore parent signal mask; restore
on success and every failure/fallback path. Child actions begin only after a raw
child result. No FFI callback, libc fork/clone wrapper, exec, dynamic symbol lookup,
allocator, unwinding, destructor, environment parser, formatting or log call in
the raw child leaf. Source/assembly review is a separate gate, not replaced by
portable trace assertions.

## Layer 2: ignored native Linux tests, executed only by independent CC

Run in a disposable subprocess fixture so atfork hooks, signal dispositions,
closed descriptors, process death and helper bookkeeping never poison the
shared test runner. Harness process creation is not the product launcher.
Every wait in the harness has an explicit deadline and owned cleanup; fixtures
use synthetic authority bytes, anonymous pipes/socketpairs and disposable files.
No X server, GUI/input, credential harvesting, profile or capability changes.

### A. Actual launch / fallback

- `native_default_environment_acquires_synthetic_credentials`: run under the
  existing environment unchanged; capture which path actually succeeded and
  assert binary cookie + observed child completion, not merely successful spawn.
- `native_raw_legacy_path_has_identical_containment`: internal test-only backend
  selection chooses raw legacy entry directly (not user configurability). Run
  the same FD/deadline/parent-death assertions as clone3. On an environment where
  clone3 is blocked but legacy is permitted, separately capture the genuine
  clone3 errno and actual fallback success. Scripted errno is not this evidence.
- If both calls are denied, record unsupported launch evidence, not successful
  functional parity; do not alter the container profile to turn the test green.
- Review raw register/argument ABI independently on each supported architecture.
  A cross-compile is not native syscall acceptance.

### B. Atfork / signal-handler exclusion

- Install an atfork prepare hook in the isolated harness that writes a marker
  and blocks on a private pipe. Product acquire must complete without invoking
  it; marker pipe stays empty. Harness-owned timeout makes an accidental libc
  fork bounded at the test level. Never install this hook in the shared runner.
- Install an inherited signal handler writing a separate marker. Pause the
  child after startup using a narrowly audited syscall-only fixture checkpoint;
  send the catchable signal. No handler marker may appear. Verify parent signal
  mask restored byte-for-byte on success, denied launch and fallback failure.
- These markers supplement (not replace) child-leaf source/assembly audit.

### C. Full inherited-FD containment before file reads

- Populate low and high descriptor slots with socketpair/pipe sentinels, a
  synthetic authority-file FD, an X-like connected socket, parent IPC endpoint,
  logging pipes and a pidfd. Include fd 3 already occupied before relocation.
- Release parent copies after spawning. Observe EOF/closed-peer behavior while
  the helper is still waiting at its private startup gate, before any credential
  path is opened. This proves actual closure rather than just a planned syscall.
- Require the retained private fd 3 to remain usable; no unrelated peer receives
  payload or output. Do not enumerate a guessed finite set as the product's
  closure implementation; raw close_range must cover the entire other range.
- Inject close/setup syscall failure at the narrow adapter boundary and assert
  no read-stage marker is emitted. This is failure-path policy evidence; the
  native sentinel test establishes actual successful closure.

### D. Blocked acquisition, late bytes and deadline ownership

- Use a harness-controlled syscall-only blocking checkpoint in the child before
  read, and separately before frame send. No Rust synchronization in the child.
  Public acquire returns within the configured bound with Timeout, no cookie,
  parent IPC revoked, pidfd kill request and retained cleanup ownership.
- When testing ordinary kill/reap, wait separately for observed exit and released
  capacity; do not infer it from signal success. SIGSTOP is *not* a simulation of
  uninterruptible kernel D-state. Scripted noncompletion tests establish only
  bookkeeping; genuine blocked-filesystem evidence, if needed, is separately
  authorized and must not be fabricated.
- Hold the startup gate past deadline, then try a late complete response. No
  authority read may begin from an expired/unvalidated startup. Independently
  exercise partial-header, partial-body, EOF, child-crash and oversized-file paths.
- Grow a synthetic regular file after metadata inspection; enforce the 1 MiB cap
  during reads, not only from initial stat. EOF after deadline never succeeds.

### E. Parent death and startup race

- Run a disposable host with an observer process. Kill the host before the
  child's prctl call, between prctl and parent-PID verification, while waiting at
  the startup gate, and while blocked in the read fixture. Use explicit markers
  and barriers, not sleep-only race ordering.
- Pre-prctl death must be detected by the parent-PID comparison before reads;
  post-prctl death uses SIGKILL. Report observed termination separately from
  requested termination. Review creator-thread death while the host process
  remains alive; failure is acceptable, but no successful late credential grant.
- An unavailable/failed prctl must exit before reads. Do not silently omit the
  death-signal/race check. Tests cannot promise immediate D-state disappearance.

### F. Reaper / resource ownership

- Fill four slots with owned Pending/Unknown identities, assert the fifth attempt
  rejects before spawn. Release one through positive completion evidence, then
  verify exactly one slot becomes available. No capacity reset on timeout alone.
- Exercise external SIGCHLD reaping/ECHILD in an isolated harness; preserve stable
  pidfd identity and explicit uncertainty. Never wait(-1) or signal a reused PID.
- Repeated failure/retry must use at most the single explicitly owned reaper;
  no per-request worker/thread or caller join. Inspect independent FD/process/
  thread counts in the fixture; account for deliberate quarantined resources.
- Verify errors and captured stdout/stderr contain no synthetic cookie or raw
  authority data. The process tracker has no credential bytes, desktop grant,
  DISPLAY authorization, global selection, or per-user permission cache.

## Release gate

Need independent CC outcomes for: genuine fallback under an unchanged applicable
environment; both raw child paths' FD/handler containment; blocked-acquisition
caller bound; no late result; owned eventual reap/unknown cases; parent-death
race; and all portable API/auth/IPC/display assertions. No actual X mutation is
possible in the helper by design and code audit; immediate kernel disappearance
is not claimed. Only then migrate hotkey/lock/clipboard consumers in their own
lanes, preserving native Xlib's separate unknown-cleanup contract.
