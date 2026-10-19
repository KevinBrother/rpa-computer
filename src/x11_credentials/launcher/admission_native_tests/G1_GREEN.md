# G1 fixture wait — minimal GREEN candidate, 2026-10-16

This checkpoint changes only `harness::wait` and adds this document. It is NOT a
product no-GO fix or complete helper acceptance. Independent review and CC13
controls are still required. No new test execution by the author.

## Actual prerequisite RED (historical, immutable)

Coordinator CC55563 ran frozen397. Read actual
`.agents/runs/linux-parity-20261015/cc-credentials-stale-false-red-1/probe.log`:
compile exit0; exact G1 control1 failed at fixture_controls.rs:204 with the target
ADMISSION_CONTROL_ASSERTION_RED, exit101, not SETUP_INFRA. Observation:
calls1/saved23us/ready23us/returned210031us/cutoff200000us; actual wait returned
ordinary false. The frozen397 archive, additive RED source and logs stay intact.

Base helper61 additive RED archive SHA256:
da20e6caad41d9e3738eb72ffec1ec572f2dae797dd5eb2d9402ecf9e6460b85.

## Minimal change: reject an uncertain negative bracket

In the existing `!ready && after >= until` branch, require `before >= until`.
If the invocation started before cutoff but returned false at/after cutoff,
explicitly raise ADMISSION_FIXTURE_INFRA: the actual negative read may precede
a timely state transition. The post-call timestamp cannot establish sample time.

Unchanged behavior:
- Successful samples require both their existing before/after time qualifications.
- A negative wholly before cutoff continues the existing bounded polling loop.
- After a previous qualified in-window negative, an invocation beginning at/after
  cutoff may return false only if its fresh sample is negative. Its log now
  includes BOTH before/after offsets, not just the misleading post-call offset.
- A late positive remains INFRA, never reconstructed as timely success.
- An already-expired first observation remains INFRA.

This selects the review's conservative reject-unknown option. No new sampling
loop, sleep, budget extension, clock abstraction or kernel model is introduced.
A real native run straddling cutoff may now be unqualified/INFRA rather than
product RED. That is intentional honesty, not a guarantee of real-time scheduling.
The helper's predicates still must represent fresh monotonic observation; a
cached arbitrary predicate is not a kernel readiness proof.

All13 existing control bodies, including G1 and the prior12, remain byte-identical.
Original native/noGO/delayed/creator/permit/publication/expiry assertions and all
product/raw/leaf files remain byte-identical. The positive/negative treatment
above is not a claim that new timing controls have independently run. Prior
FIXTURE_GREEN.md's negative-sample claim is superseded by this explicit bracket
requirement; that historical document and all previous frozen artifacts remain.

## Independent review / CC13 only

Apply this two-file overlay on frozen397. Review the exact small wait diff, then:

```sh
cargo-1.89 test --locked --offline --lib --no-run

timeout 45 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::fixture_controls \
  -- --nocapture --test-threads=1
```

Expected, NOT RUN here: **13 PASS / exit0**. Original negative controls and G1
intentionally catch ADMISSION_FIXTURE_INFRA; their panic-hook output under
--nocapture must not be counted as run-level failures. G1 setup qualification is
outside the catch; ADMISSION_CONTROL_SETUP_INFRA is never a PASS. Zero matches,
compile failure, timeout or unrelated panic cannot close this gate.

If a separate exact G1 log is requested, use:

```sh
timeout 30 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::admission_native_tests::fixture_controls::in_window_false_returned_after_cutoff_cannot_establish_timeout \
  -- --exact --nocapture --test-threads=1
```

No native execution is requested by this checkpoint. Product noGO remains
unchanged and unresolved; only after independent review+CC13 should its next
implementation gate be discussed. Actual TID reuse, creator resource recovery,
whole-host death, real clone3-success evidence and ARM64 runtime remain separate.

## Compile-only and delivery

Actual-source helper `cargo check --locked --offline --tests` exit0 on both
x86_64-unknown-linux-gnu and aarch64-unknown-linux-gnu. Logs:
`.agents/runs/linux-parity-20261015/credentials-admission-g1-green-check-<target>.log`.
This is not a full Linux root build or test evidence. No tests/SSH/GUI/commit/push.

Two-file overlay: admission_native_tests/harness.rs and this G1_GREEN.md.
Supplied SHA256 overlay manifest plus complete resulting helper62 manifest.
Freeze pending independent review/CC; do not mutate or relabel frozen397 evidence.
