# Foreground permit GREEN candidate — 2026-10-16

Partial candidate on top of credentials-expiry-green-1; no tests executed here.
The authority for this repair is the independently observed fourth-caller/fifth-
acquire assertion RED. This is NOT a creator-death ownership repair, nor all-six-
native GREEN. All original 15 test-source files remain byte-identical to RED.

## One capacity domain, two retirement obligations

Production policy::run now reserves a foreground-aware tracker slot BEFORE
preparation. Child completion and foreground retirement are separate facts in
the SAME four-slot pool. A reaped child leaves the slot occupied while its caller
still holds pre-parse replies / adapter context. A returned/failed caller leaves
Pending/Unknown child ownership occupied. Only both retired obligations permit
reuse. Positive Reaped evidence is sticky; no stale generation can retire a
replacement. Reaper skips already-recorded Reaped children, not their permits.

The outer Foreground guard surrounds the whole inner policy operation. Inner
reply/handle locals retire first, then Native closes IPC and releases its owned
Prepared allocation/Handle reference, THEN tracker retires foreground. Normal
completion therefore recovers capacity; this is not permanent quarantine.
Adapter cleanup is idempotent and preserves the previous pidfd-only panic-safety
signal request. No wait/join, new thread, new queue, or fresh deadline is added.

The low-level child-only Tracker::reserve primitive remains for independent
child ownership: its positive-reap retirement is correct when there is no
foreground obligation. Production policy does NOT use that path. Existing
scripted resource-free adapters use the trait default; the real Tracker and
Native implement the actual two-obligation protocol. No cfg(test) production
fallback or alternate legacy launcher is retained.

## Narrow unresolved-launch boundary

Immediately before entering a spawn adapter, policy disarms its NotStarted
reservation destructor. Only an explicit returned NoChild/Unavailable outcome
re-arms it. Thus an unwind from an unreturned spawn cannot free the slot as if
no child existed. No raw-FD inference, ambiguous fallback or replay is added.
An unconfirmed/no-attached-child slot remains bounded even after foreground
retirement; this is ownership uncertainty, NOT successful recovery.

This is NOT the proposed stable raw-attempt record implementation: kernel pidfd
storage and Native resource ownership are still on the existing parent path.
Creator SYS_exit bypasses the guard. In particular, an attached/reaped child of
a dead caller now correctly retains its foreground obligation, but cannot yet
retire that caller's resources. All three creator gates remain open. The next
raw-publication candidate is specified separately in RAW_PUBLICATION_DESIGN.md;
none of it or the ARMED/GO witness has been implemented here.

## Independent commands

Fresh integrated Linux build, unchanged seccomp; original high-FD regressions
need only per-container `--ulimit nofile=8192:8192`. Log each command separately.
No zero-match/fixture-failure/timeout may be counted as assertion acceptance.

```sh
cargo-1.89 test --locked --offline --lib --no-run

# Existing authoritative native admission regression: exactly one outer case.
timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::lifecycle_native_tests::reaped_children_do_not_bypass_foreground_admission_cap \
  -- --exact --ignored --nocapture --test-threads=1

# Six additional real-tracker unit assertions; NOT historical assertion RED.
timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::foreground_tests -- --nocapture --test-threads=1

# Additional normal production reuse: eight real acquisitions in ONE dedicated
# test process. Exactly this filter; no scripted native/completion observations.
timeout 30 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::foreground_native_tests::normal_acquisitions_retire_resources_and_reuse_all_permits \
  -- --exact --ignored --nocapture --test-threads=1
```

Also run both exact expiry cases and all original 26 pure / 12 policy / 9 tracker /
9 native (8 scenarios+dispatcher), using EXPIRY_GREEN.md commands. Original counts
remain unchanged; six new pure assertions and one new native case are additional.
The model witness counterexample and three creator cases are NOT repaired.
Supplemental tests are newly authored coverage, not claimed observed RED/GREEN.

## Compile-only evidence

- macOS root cargo check --locked --offline --all-targets exit0; existing unrelated
  hotkey warning left untouched.
- Actual-source helper cargo check --tests for x86_64-unknown-linux-gnu and
  aarch64-unknown-linux-gnu exit0, including supplemental test sources.
- Logs: .agents/runs/linux-parity-20261015/credentials-permit-green-check-*.log.
- Raw assembly/trampoline/deadline files are byte-identical to expiry candidate;
  previous emission evidence is historical, not a new runtime/ABI approval.
- Complete Linux root build and runtime verification remain independent CC work.

No tests/SSH/GUI/commit, consumer migration, root registration/manifest changes,
or lock edits. Previous frozen archives are preserved; this candidate adds its
own complete helper manifest/archive for independent overlay.
