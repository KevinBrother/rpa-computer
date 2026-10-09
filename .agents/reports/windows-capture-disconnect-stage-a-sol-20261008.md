# Windows capture disconnect — Stage A (SOL), 2026-10-08

## Status and authority boundary

**SOURCE_FROZEN — Windows compile0 only; Windows RED NOT EXECUTED.**

Author inspected local evidence/source and compiled the dedicated regression. No unit/API/GUI/repro tests, executable invocation, SSH, remote transfer, subagents, worktree, commit, push, reset, deletion, or production-source modification. Actual test execution/review remains **CC+GLM**. No Stage B product patch is authorized until the coordinator accepts actual CC Windows RED.

## 1. First evidence and authoritative host log

Original transcript: `.agents/runs/windows-current-multiclick-cc-20261008/inner-run/transcript.jsonl`; its `/private/tmp/windows-current-multiclick-cc-20261008/inner-run/transcript.jsonl` copy has identical SHA256. Both first oracle copies are also identical. Full sizes/hashes are in `evidence-freeze.json` in the owned run directory; original evidence is untouched.

Transcript source inspection identifies:

| Calls | Transcript lines | Evidence |
|---|---|---|
| 2 / 3 / 4 | 17–18 / 47–48 / 59–60 | open `session-31688-1`; native observe `Connection closed`; next observe `session_not_found` |
| 5 / 6 / 7 | 61–62 / 63–64 / 79–80 | same sequence for `session-27588-1` |
| 8 / 9 / 10 | 81–82 / 83–84 / 85–86 | same sequence for `session-2036-1` |
| 11 / 12 / 13 | 87–88 / 89–90 / 91–92 | same sequence for `session-31300-1` |
| 14 / 15 / 16 | 151–152 / 153–154 / 279–280 | bounded open `1024×768`, successful observe, then first input |

Five opens overall; no input precedes the successful bounded observation. Correlation with capture size alone is **not** used as root-cause proof.

CC supplied and remotely verified the authoritative raw log; author only read its existing local copy:

- `/private/tmp/windows-multiclick-diagnosis-20261008/host-log-current-20261008.log`
- 4,812 bytes; SHA256 `89a2a3ae11d965b42189fc39587be3bebd294514e829cd49728a47d38fcb2574`.
- Lines 27/28, 35/36, 43/44, 51/52: four `remote transport failed: tls writer accepted 0 bytes` messages and the matching pump error for each.
- Lines 30/38/46/54: workers exit 0 after each failed transport; subsequent authenticated connections start fresh workers.
- Lines 55–60: final worker stops/exits 0, without the zero-write failure.
- Host-log location manifest exists and is separately frozen. Author did not retrieve any remote log.

Logs have no per-entry timestamp or worker PID, so exact transcript-session-to-TCP-port mapping is not independently proven here.

CC-supplied production binary identities, **not freshly rehashed by author**:

- host: `1aeedcd930528abc29358555a6d7c7abb7b0f8ee8bbf9f6e9d9b39abe8164b9c`.
- client: `2f1f8335160ecd5ee7d3d86d0f88f75be8111c05bb4df600fa6b0ae96d622dc7`.

Current source177 matches all 177 entries in `.agents/runs/windows-root-race-repair-sol-20260930/source-freeze.sha256`, before and after this work.

### Oracle preservation

First oracle: 42,292 bytes / 70 records / SHA256 `eb30833e415959ad8cb1d62725f9351d78d5ddf7fa21578344bdd0248b139e3d`. Later CC authoritative oracle: 42,578 bytes / 71 records / SHA256 `2fe4a01115cce6cdddda1aae133111b598516638638f123682f95835c5061add`. The latter is an **exact byte-prefix extension** of the first: one `session_close` record, timestamp `2026-10-08T03:42:55.2937960Z`. It is not substituted for FIRST evidence, and does not redefine any first-input verdict. `oracle-tail-note.json` records the difference.

## 2. Proven source defect, versus unmeasured triggering conditions

Production source chain:

1. `src/mcp/remote/host.rs:339–352` sends child stdout in chunks no larger than `CHUNK_BYTES` (64 KiB).
2. `src/mcp/remote/tls.rs:20` fixes `TLS_BUFFER_LIMIT` at **4 MiB**; `pump.rs:296` applies that bound.
3. `pump.rs:270–285`, especially line 277, treats a real rustls plaintext-writer `Ok(0)` as fatal `Err("tls writer accepted 0 bytes")`, even though `pending` is nonempty. Partial writes update `off` before the next writer call.
4. Pinned **rustls 0.23.45 primary source**: `conn.rs:358–365` wraps the accepted plaintext length as `Ok(len)`; `common_state.rs:320–334` derives accepted length from `sendable_tls.apply_limit`; `vecbuf.rs:65–72` uses remaining bounded capacity, which can be zero. `conn.rs:488–522` documents outgoing-record buffering and `write_tls` draining. Therefore zero acceptance is reachable on a valid established connection with a full bounded TLS output buffer; it is not inherently a broken socket or backend error.
5. `pump.rs:331–337` already has a retry/backpressure path for `Ok(false)` that flushes TLS records and preserves `pending`/`off`. Current `Ok(0)` bypasses it, emits a failed event, and exits the pump.
6. `host.rs:402–405,415–429` treats that event as fatal, drops the child input channel, and logs the same pump failure. The input thread drops worker stdin on channel closure (`host.rs:364–375`), explaining why worker exit 0 is compatible with transport failure, not proof of successful observation.

**Proven:** the four live failures hit this exact zero-write fatal branch; the current helper misclassifies valid rustls bounded-buffer backpressure. The dedicated regression targets that behavior using real authenticated TLS, not a fake writer or forced panic.

**Not measured/proven:** failed screenshots' encoded response sizes, socket queue occupancy/timing, or the precise buffer occupancy at each live failure. No screenshot backend fault, disk-capacity cause, 4 MiB image threshold, or resize-based cure is asserted. Source and host error establish the transport handling defect without those guesses.

## 3. Dedicated regression source

New source: `tests/windows_capture_disconnect/write_pending.rs` (205 lines).

SHA256: `1405ec0803a178af804a9161aa17bdbd56cf798e807f42c047a87a5d9861aef2`.

This is intentionally a dedicated child-module source, not an auto-discovered Cargo integration target. The private helper cannot be accessed through an integration-test sibling module without changing production visibility. The frozen minimal harness:

- Copies production `pump.rs` **byte-for-byte**, then appends only a `#[cfg(test)] #[path = "../../../tests/windows_capture_disconnect/write_pending.rs"] mod windows_capture_disconnect;` declaration.
- Copies `tls.rs`, existing support sources and public throwaway TLS fixtures exactly; never edits their originals.
- Calls the actual private `write_pending`, not a duplicated implementation.
- Uses the original 4 MiB `TLS_BUFFER_LIMIT` and 64 KiB `CHUNK_BYTES` unchanged.
- Uses real rustls client/server connections with fixture CA verification and completed in-memory handshake. No socket, GUI, product process, backend, timer sleeps, or mocked writer.
- Bounds handshake passes at 64, TLS-record passes at 1,024, and saturation writes at `TLS_BUFFER_LIMIT / CHUNK_BYTES + 2`. CC runner provides a 30-second process bound.

Cases:

1. `full_tls_buffer_retains_pending_instead_of_disconnect`: fill real post-handshake output until writer actually returns zero; expect `Ok(false)`, unchanged data/offset, and exact once-only delivery after drain/retry. **Failing-before expectation:** current helper returns `Err("tls writer accepted 0 bytes")` at the `Ok(false)` assertion.
2. `partial_tls_write_retains_offset_and_resumes_without_replay`: leave less than one chunk of TLS capacity; require strictly positive partial progress before saturation, then `Ok(false)` and retained offset; drain and verify only the unsent suffix is retried. **Failing-before expectation:** same erroneous fatal result after advancing the offset. Failure of the partial-progress setup assertion is **not** valid RED for the defect.
3. `writable_tls_buffer_consumes_one_chunk_exactly_once`: unpressured control, clears pending/offset, exact peer bytes, empty retry never replays output. **Expected current behavior:** pass.

The executable also contains pre-existing `tls.rs`/support unit tests because those files are copied unchanged. The exact CC filter selects **only these three new cases**; unrelated tests are not to be run as part of this handoff.

No assertion says merely "an error occurred". Setup, retained plaintext, accepted offset and peer-decrypted output have separate expectations. No limits weakened to make tests pass. These are **expected** outcomes from source analysis, not observed RED/GREEN results.

## 4. Compile-only evidence and freeze

NEW storage root (all large outputs and compiler temporary files):

`/private/tmp/windows-capture-disconnect-stage-a-sol-20261008.rbt5w_hx`

Dedicated harness: `/private/tmp/windows-capture-disconnect-stage-a-sol-20261008.rbt5w_hx/harness`.

Windows test executable:

`/private/tmp/windows-capture-disconnect-stage-a-sol-20261008.rbt5w_hx/target/x86_64-pc-windows-msvc/debug/deps/windows_capture_disconnect_stage_a-6156eb2f9d8e50c1.exe`

- SHA256: `af3b04de6582d943a59f5edcdd4c72d03c7cc2e81755025204af1868f7dedb8a`.
- Size: 4,089,856 bytes; statically inspected PE Machine `0x8664` (AMD64).
- Rust: `rustc 1.98.1 (48a229cea 2026-09-01)`; host `aarch64-apple-darwin`, target `x86_64-pc-windows-msvc`.
- Compile command: `cargo test --manifest-path /private/tmp/windows-capture-disconnect-stage-a-sol-20261008.rbt5w_hx/harness/Cargo.toml --lib --locked --offline --no-run --target x86_64-pc-windows-msvc --message-format=json`.
- **Actual compiler exit 0**; Cargo finished the test profile; compiler JSON has no warning/error diagnostic records.
- **Tests/executable NOT EXECUTED.** No claim of Windows RED or regression runtime correctness.

First metadata command (unfiltered platform, offline) exited 101 because uncached `wasi v0.11.1+wasi-snapshot-preview1` would require download. It never reached compilation. FIRST `metadata.stderr.log` and `metadata.exit` are preserved. A separate Windows-filtered metadata command (`--filter-platform x86_64-pc-windows-msvc`) exited 0, followed by the successful compile. No dependency versions/features were changed to recover.

Harness Cargo.lock is seeded from production Cargo.lock with only the root package changed to the dedicated harness; Cargo pruned unrelated packages. **All 25 remaining external lock entries match production versions/checksums.** Compiled dependency source freeze covers **13 packages / 768 files**; package hashes and exact source roots are retained in `/private/tmp`, with its SHA in `dependency-freeze-index.json`. This includes the pinned rustls primary source used for diagnosis.

Owned run directory: `.agents/runs/windows-capture-disconnect-stage-a-sol-20261008/`.

- `storage-index.json`: exact durable /private/tmp paths, executable/bundle hashes.
- `source177-verification.json`, `source-freeze.sha256`: all original 177 refs plus new regression.
- `product-source-before.sha256`, `product-source-after.sha256`: equality verified for direct harness inputs; no production changes.
- `harness-provenance.json`: exact production prefix and sole module-declaration suffix.
- `dependency-freeze-index.json`, `artifact-and-harness-freeze.json`: dependency source and build/artifact hashes.
- `compile-results.json`: actual compile0, first metadata failure, NOT_EXECUTED statuses.
- `compile-only.zsh`, `compile-only-windows.zsh`: preserve first attempt and Windows-filtered compile procedure.
- `evidence-freeze.json`, `oracle-tail-note.json`: original evidence identity/preservation.
- `cc-red-runner.ps1`: CC-only bounded runner, source SHA256 `c4612c9068a544f3b355151f0c80ecf24ee9797dcf1b0c91e5877dd2ac89ec02`.
- `status.json`: SOURCE_FROZEN, awaiting CC Windows RED and coordinator authorization.

No old artifacts, traces, reports, manifests, caches or fixtures were deleted or rewritten. Repository volume free space was checked; no disk cause is inferred.

## 5. Exact CC Windows runner handoff — NOT EXECUTED by author

Executable/runner bundle (local artifact for coordinator/CC to transfer, **not deployed by author**):

`/private/tmp/windows-capture-disconnect-stage-a-sol-20261008.rbt5w_hx/cc-windows-red-bundle.zip`

SHA256: `74560dcb0dce780e15b6da6d09fb0cd258e8f806ca86925bb9815448723a8d49`.

Once **CC** has transferred that exact zip to the following Windows path, run in PowerShell (no GUI needed). Each destination/evidence directory must be new; do not reuse an earlier result directory or overwrite FIRST output.

```powershell
$Zip = 'C:\Temp\windows-capture-disconnect-stage-a-sol-20261008-rbt5w_hx.zip'
$Bundle = 'C:\Temp\windows-capture-disconnect-stage-a-sol-20261008-rbt5w_hx'
if ((Get-FileHash -LiteralPath $Zip -Algorithm SHA256).Hash.ToLowerInvariant() -ne '74560dcb0dce780e15b6da6d09fb0cd258e8f806ca86925bb9815448723a8d49') { throw 'Frozen bundle SHA mismatch' }
if (Test-Path -LiteralPath $Bundle) { throw 'Use a NEW bundle destination; preserve FIRST output' }
Expand-Archive -LiteralPath $Zip -DestinationPath $Bundle
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$Bundle\cc-red-runner.ps1" -TestExe "$Bundle\windows_capture_disconnect_stage_a.exe" -EvidenceDir "$Bundle\first-red"
$LASTEXITCODE
```

Runner verifies the executable's exact hash, rejects an existing output directory, executes only:

```text
windows_capture_disconnect_stage_a.exe pump::windows_capture_disconnect:: --nocapture --test-threads=1
```

It drains stdout/stderr asynchronously, preserves native exit code, kills only its owned test executable on the 30-second timeout, and stores `stdout.log`, `stderr.log`, `result.json`, `exit.txt`, `exe.sha256`.

**CC acceptance of RED:** exactly 3 selected tests; full/partial cases fail specifically with actual `Err("tls writer accepted 0 bytes")` versus expected `Ok(false)`; writable control passes; process exit 101; no handshake/setup/fixture/timeout failure. A startup failure, timeout, zero selected tests, or different panic is **not** sufficient RED. CC+GLM must review the raw output and frozen source before requesting coordinator authorization.

Optional CC source rebuild, not needed to run the supplied executable: source-only frozen harness archive `/private/tmp/windows-capture-disconnect-stage-a-sol-20261008.rbt5w_hx/source-frozen-harness.tar.gz` (SHA256 `beb7478de9967e01abee71e32c8c69aa62d63eed95ebfd0f379b3c1021385dc6`). Extract into a NEW Windows directory and use:

```powershell
$env:CARGO_TARGET_DIR = 'C:\Temp\windows-capture-disconnect-stage-a-sol-20261008-source-build\target'
$env:CARGO_INCREMENTAL = '0'
cargo test --manifest-path 'C:\Temp\windows-capture-disconnect-stage-a-sol-20261008-source-build\harness\Cargo.toml' --lib --locked --offline --no-run --target x86_64-pc-windows-msvc --message-format=json
```

Do not use the supplied cross-compiled executable hash to validate a different rebuilt binary. A CC rebuild needs its own compiled-binary hash and FIRST runtime evidence. Never drop `--locked`, relax buffer limits, or update dependencies to pass this regression.

## 6. Smallest proposed Stage B change — NOT APPLIED

After actual Windows RED and coordinator authorization: in `src/mcp/remote/pump.rs::write_pending`, handle a nonempty pending buffer's rustls plaintext-writer `Ok(0)` as **retryable `Ok(false)`**, preserving `pending` and `off`. The existing caller then flushes and retries. Keep genuine I/O/TLS errors fatal, existing `WouldBlock` behavior, all queue/chunk/TLS bounds, close semantics and abort handling unchanged. This is specific to the pinned rustls writer's bounded-capacity semantics, not permission to swallow arbitrary socket `WriteZero` errors.

Subsequent GREEN, broader transport validation, full-size Windows observe/reconnect acceptance and GUI verification are separate CC+GLM/coordinator gates. None has happened here.

## 7. Unresolved evidence/gates

- Actual Windows RED and independent CC+GLM source/output review are pending.
- Exact live TLS buffer occupancy, failed response sizes and timing were not recorded; no payload-threshold claim is made. A follow-up request, if needed after RED, should be bounded to CC collection of pending length/offset, writer acceptance, and socket-flush WouldBlock around ONE existing failure path, without input injection, limit changes or treating resize as a fix.
- Full-size screenshot delivery/reconnect after a future patch is unverified.
- Reported production binary/source association is CC provenance, not a new author-side Windows inspection.
- Regression's setup assumptions must be confirmed by CC runtime; compile0 alone is not RED.

## Changed file paths

- `tests/windows_capture_disconnect/write_pending.rs` (only new regression source; existing tests untouched).
- `.agents/reports/windows-capture-disconnect-stage-a-sol-20261008.md` (this report).
- `.agents/runs/windows-capture-disconnect-stage-a-sol-20261008/` (only new owned freeze/index/procedure/runner files listed above; large raw files remain in NEW /private/tmp storage).

**No product source fix. No tests run by author.**
