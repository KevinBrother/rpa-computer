# Child expiry-only GREEN candidate — 2026-10-16

This checkpoint addresses ONLY the two independently observed expired-startup
assertion failures. It does not fix creator ownership/witness or foreground
admission. All previous test sources and assertions remain byte-identical.
No consumers, lock, root manifests or registration changes. No tests, SSH, GUI,
commit or push executed by the implementation worker.

## Production change

- Prepare one absolute CLOCK_MONOTONIC deadline before clone. Sample the kernel
  clock before sampling Instant's remaining duration: intervening scheduling can
  shorten the budget, never renew/extend it. Check arithmetic and clock errors.
- Copy that value into the private COW Child context; clone fallback reuses it.
- Both raw assembly leaves check now < deadline immediately after consuming each
  permission and again before openat (after any existing pre-open test barrier).
  Clock failure or equality/expiry raw-exits; no Rust child callbacks/allocations.
- ARM64 permission saves/restores x30 unconditionally because freshness is now a
  production local subroutine; stack remains 16-byte aligned. x86-64 freshness
  uses 16 private stack bytes. Neither helper changes the established context,
  file, payload-length or status registers.
- Parent checks, cancellation/cleanup policy, syscall clone methods, FD closure,
  signal masks, startup protocol and test barriers remain otherwise unchanged.

Boundary: a clock check and openat are separate syscalls. This closes consumption
of already-expired permissions and adds a path-entry freshness check; it is NOT
an atomic kernel deadline+open operation or a guarantee against descheduling
between those syscalls. Cancellation still does not synchronously wait for a
credential-only helper. No new kernel-version/procfs/witness prerequisite.

## Independent commands (implementation worker must not run)

Use a fresh integrated Linux source build, default seccomp, per-container
`--ulimit nofile=8192:8192` for the original high-FD cases. Preserve log/exit code
for each command; zero matches, fixture failures and timeouts are not acceptance.

```sh
cargo-1.89 test --locked --offline --lib --no-run

timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::lifecycle_native_tests::expired_permission_is_rejected_at_child_consumption \
  -- --exact --ignored --nocapture --test-threads=1

timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::lifecycle_native_tests::permission_write_race_cannot_authorize_expired_startup \
  -- --exact --ignored --nocapture --test-threads=1
```

Run each original group separately, including normal successful acquisition:

```sh
# Capture each invocation's status separately; do not stop after the first group.
for group in api_tests auth_tests ipc_tests display_tests \
             launcher::policy_tests launcher::tracker_tests; do
  timeout 120 cargo-1.89 test --locked --offline --lib \
    "x11_credentials::$group" -- --nocapture --test-threads=1
  printf 'GROUP=%s EXIT=%s\n' "$group" "$?"
done
timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::native_tests -- --ignored --nocapture --test-threads=1
```

The full lifecycle-native filter still includes three unfixed creator cases and
one unfixed admission case; the model's missing-witness assertion is also unfixed.
Do not label this checkpoint six-native-GREEN. Actual TID reuse, whole-host death,
both-ABI raw review and remaining native containment gates are still required.

## Compile-only evidence

- macOS root `cargo check --locked --offline --all-targets`: exit0; pre-existing
  other-owner hotkey dead-code warning, not changed.
- Actual-source helper `cargo check --tests` on x86_64-unknown-linux-gnu and
  aarch64-unknown-linux-gnu: exit0.
- Actual-source helper `cargo rustc --lib -- --emit=asm,obj` AND
  `cargo rustc --lib -- --cfg test --emit=asm,obj` on both ABIs: exit0. The latter
  emits 13 unused test-import warnings because this is not a running test harness.
- No complete Linux root build or runtime/test success claimed.

Logs under `.agents/runs/linux-parity-20261015/credentials-expiry-green-*.log`.
Generated child-only slices: `credentials-expiry-green-1-child-*-*.s` in that
same directory. Local inspection found only local assembly subroutine calls;
this is not independent ABI approval or runtime evidence.

## Next ownership state decision (not implemented here)

A stable slot must be reserved and own kernel output storage before clone.
Preparing -> Launching does not authorize retirement. A release-published syscall
result may transition to Published(child) or NoChild; only confirmed NoChild may
release as unstarted. A negative failed first clone result permits at most the
existing one documented fallback, never an ambiguous replay.

If creator disappears while Launching, a nonnegative output FD alone is NOT a
completed launch. That slot must remain bounded/owned until creator-stopped and
kernel-result recovery are proven. Witness research remains separate; no TID or
timestamp guesses. No "recovered" claim for an unresolved publication window.
Foreground permit will be a separate obligation from positive child reap, with
normal completion returning capacity only after both obligations retire. These
changes are not part of this expiry-only checkpoint.
