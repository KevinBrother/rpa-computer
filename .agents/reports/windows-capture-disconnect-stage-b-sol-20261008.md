# Windows capture disconnect — Stage B SOL — 2026-10-08

## Status

**SOURCE_FROZEN. Windows root test compilation = 0; Windows release Host/Client compilation = 0. No test/executable was run by author. CC Windows GREEN and independent CC+GLM review are pending. GUI default-size capture verification is a separate later gate.**

Stage B was explicitly authorized after coordinator acceptance of actual CC Windows RED. Work stayed in the shared repo, without worktree, commit, push, reset, subagents, SSH, deployment, GUI, test/repro execution, deletion, or changes to Cargo/limits/screenshot defaults. Large data is exclusively in a NEW `/private/tmp` directory.

## Accepted RED evidence (read, not re-executed)

- CC report: `.agents/reports/windows-multiclick-stage-a-cc-20261008.md`.
- Raw FIRST result: `/private/tmp/windows-multiclick-stage-a-cc-20261008/first-red/`.
- Read `result.json`, `stdout.log`, `stderr.log`: native exit **101**, `timed_out: false`, **2 failures + 1 control pass**.
- Both failures are specifically actual `Err("tls writer accepted 0 bytes")` versus expected `Ok(false)`, at frozen source lines 130 and 167; partial-progress setup passed before its assertion failure.
- Original regression SHA256: `1405ec0803a178af804a9161aa17bdbd56cf798e807f42c047a87a5d9861aef2` — still identical to accepted RED.
- Author did not repeat Stage A tests or broaden the regression cases.

## Minimal production change

`src/mcp/remote/pump.rs::write_pending` gains one guarded match arm:

```rust
Ok(0) if conn.wants_write() => return Ok(false),
```

The existing unguarded `Ok(0) => Err("tls writer accepted 0 bytes")` remains immediately after it.

Behavioral contract:

- Nonempty retained plaintext plus zero acceptance **with queued TLS output** returns the existing retry/backpressure result.
- `pending` and `off` remain unchanged by this arm. A prefix already accepted by prior `Ok(n)` calls stays accounted for in `off`.
- The existing pump caller flushes TLS output and then continues its normal read/abort/deadline handling; no new inner retry loop is introduced.
- Zero acceptance **without TLS output to drain remains fatal**, preventing a new no-output/no-progress retry spin.
- Genuine writer errors, `WouldBlock`, socket flushing, socket-zero-write handling, cancellation flags, drain deadlines, close/truncation classification, chunk/queue/TLS bounds are **not changed**. The guarded handling is for the rustls plaintext writer, not a blanket reinterpretation of `TcpStream`/`write_tls` zero writes.
- No claim is made that this patch redesigns all pre-existing stalled-socket behavior. Existing socket flush has its own bounded per-pass loop; that code is unchanged.
- Screenshot defaults, scaling, PNG/backend capture code and connection/session policy are untouched.

There is exactly one production helper implementation. `production-change.patch` records the complete diff against the preserved Stage A pump: the guarded arm/comments and a test-only forwarding function, nothing else.

Current pump SHA256: `d360c0e699d71f6a6978716413e746caee97c2bb5fb349c0995266471b60abeb`.

## Normal Cargo integration; original three cases unchanged

New normal integration entry: `tests/windows_capture_disconnect.rs` (SHA256 `2655fc8c4c2bfdf36e4cd6f97b98cb87f1616102da70027f4c24dc262cc207a6`). Cargo discovers it automatically; no Cargo.toml/Cargo.lock changes.

The entry uses `#[path]` to compile the real production `pump.rs` and `tls.rs`, and includes the existing `tests/windows_capture_disconnect/write_pending.rs` **without changing one byte**. Its frozen `crate::tls` import resolves to the real TLS module at the integration crate root.

A small `#[cfg(test)]` forwarding function in production pump, `write_pending_for_regression`, calls the private `write_pending` directly. It contains **no copied algorithm**, is absent from release builds, and does not widen release visibility. This avoids modifying `lib.rs` or rewriting the accepted RED tests just to adapt their import path.

The three regression cases retain real in-memory rustls handshake, unchanged **4 MiB** TLS limit, unchanged **64 KiB** chunk size, exact pending/offset and peer-plaintext checks. No new cases, mocks, sockets or GUI behavior were added. The old source's Stage A comments remain historical text because its bytes are frozen.

Normal Cargo execution command for **CC only**, in a matching Windows source tree:

```powershell
cargo test --locked --offline --target x86_64-pc-windows-msvc --test windows_capture_disconnect windows_capture_disconnect:: -- --nocapture --test-threads=1
```

This is no longer an orphan ad-hoc harness. The ordinary root Cargo integration target compiled successfully below. The standalone historical Stage A harness remains untouched as RED evidence.

## Compile-only work

Fresh storage/target/temp root:

`/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y`

Author ran only these compile commands (known xwin environment, fresh `CARGO_TARGET_DIR`, `CARGO_INCREMENTAL=0`, and `TMPDIR` beneath the new root):

```sh
cargo test --lib --test windows_capture_disconnect --test remote_transport --locked --offline --no-run --target x86_64-pc-windows-msvc --message-format=json
cargo build --release --bins --locked --offline --target x86_64-pc-windows-msvc --message-format=json
```

- Root tests compile exit **0**, 19.99 s: root library unit-test executable, normal `windows_capture_disconnect` integration executable, existing `remote_transport` integration executable, and debug Host/Client artifacts.
- Release bins compile exit **0**, 45.62 s: release Host and Client.
- Compiler diagnostics: **0 warnings, 0 errors**.
- Rust `1.98.1 (48a229cea 2026-09-01)`; target `x86_64-pc-windows-msvc`; all listed executable PE headers statically checked as AMD64 (`0x8664`).
- Root Windows-filtered dependency metadata also exited 0 (`--locked --offline --filter-platform x86_64-pc-windows-msvc`); no dependency edits/download workaround.
- **No executable was invoked, including no `--list` invocation. No runtime GREEN is claimed.**

### Frozen primary artifacts

| Artifact | SHA256 |
|---|---|
| Normal Cargo regression executable | `d7d6a85749755664873a32957c8da7d3df9d6c95f535624a12739b791f099b73` |
| Release computer-host.exe | `c8c1348f9cca5d8a95d08ff09b2d1be67eec0657b1639267a2959d835d258d1e` |
| Release computer-client.exe | `445675c3db815460b8e8f83ffec6ceb546194d3e098f444fbdeb3864da516ddc` |
| Existing remote_transport test executable | `7eecd49004ff5ed346b187ab30cea2f940cb8d74452e88778350e66ebfe738d6` |
| Root library unit-test executable | `5bafecfa2ccb962b4c4b6036ca0c8b698fde6b822eb23305c11e0f3dcfa40600` |

Exact paths:

```text
/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y/target/x86_64-pc-windows-msvc/debug/deps/windows_capture_disconnect-017e802864c1ae8d.exe
/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y/target/x86_64-pc-windows-msvc/release/computer-host.exe
/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y/target/x86_64-pc-windows-msvc/release/computer-client.exe
/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y/target/x86_64-pc-windows-msvc/debug/deps/remote_transport-fef3145630261550.exe
/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y/target/x86_64-pc-windows-msvc/debug/deps/rpa_computer-d743cec7e10c1ea5.exe
```

Every compiled executable, including debug bins, is enumerated in `compiled-artifacts.json` with size/hash/PE identity. Compiler JSON/stderr/exit files and all binaries remain on the Data volume, not `/Volumes/doc`.

## Freeze/provenance and intentional baseline drift

Owned run directory: `.agents/runs/windows-capture-disconnect-stage-b-sol-20261008/`.

- `preserved-history.sha256`: original Stage A report/run files, regression bytes, accepted CC report/FIRST RED logs, historical source177 manifest, old artifact/source bundles, authoritative host log. Revalidated unchanged.
- `source177-before.json`: all **177** historical refs matched before editing.
- `source177-intentional-drift.json`: after editing, **176 unchanged; only `src/mcp/remote/pump.rs` intentionally changed**. New integration entry `tests/windows_capture_disconnect.rs` is separately identified. No baseline manifest was rewritten to conceal drift.
- Old pump retained at `/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y/before/pump.rs` with historical SHA `c0516adb97c3299d41e49ed559e39945f43cf0c08e30979e558b75bac55480ba`.
- `compile-source-freeze.sha256`: **193** direct root build/test inputs frozen before compilation and checked afterward.
- `stage-b-source-freeze.sha256`: current source hashes including historical refs and new test integration; distinct from the untouched Stage A baseline.
- `production-change.patch`: exact narrow production diff.
- `dependency-freeze-index.json`: original production Cargo manifest/lock hashes, no dependency changes, and Data-volume source-byte freeze of **91 compiled registry packages / 7043 files**.
- `compiled-artifacts.json`, `artifact-freeze.json`, `compile-results.json`, `storage-index.json`: exact outputs, logs, exit codes and bundle hashes.
- `compile-only.zsh`: recorded compile procedure.
- `cc-green-runner.ps1`: hash-guarded 30-second CC runner for exactly the three unchanged regression cases.
- `status.json`: SOURCE_FROZEN; CC GREEN/review and subsequent GUI gate remain pending.

Historical Stage A source manifests still correctly describe Stage A. Checking them against the now-authorized Stage B production tree will intentionally report pump drift; use the explicit Stage B drift report rather than altering history.

## Exact CC verification handoff — not run by author

Local bundle for coordinator/CC transfer (author did not transfer/deploy it):

`/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y/cc-windows-stage-b-bundle.zip`

SHA256: `12a0498a8d717f60e46cc33664cfb49e42f5cb4bf63040e588542a5dbcecaedd`.

Contents: normal Cargo regression executable, hash-guarded runner, release Host/Client, existing remote_transport executable, root unit-test artifact and per-file hash manifest. Inclusion of an artifact is **not** blanket authorization to execute all tests or start the GUI/native host.

Once CC transfers that exact zip to this Windows location, run:

```powershell
$Zip = 'C:\Temp\windows-capture-disconnect-stage-b-sol-20261008-lfnu54y.zip'
$Bundle = 'C:\Temp\windows-capture-disconnect-stage-b-sol-20261008-lfnu54y'
if ((Get-FileHash -LiteralPath $Zip -Algorithm SHA256).Hash.ToLowerInvariant() -ne '12a0498a8d717f60e46cc33664cfb49e42f5cb4bf63040e588542a5dbcecaedd') { throw 'Frozen Stage B bundle SHA mismatch' }
if (Test-Path -LiteralPath $Bundle) { throw 'Use a NEW destination; never overwrite FIRST output' }
Expand-Archive -LiteralPath $Zip -DestinationPath $Bundle
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$Bundle\cc-green-runner.ps1" -TestExe "$Bundle\windows_capture_disconnect.exe" -EvidenceDir "$Bundle\first-green"
$LASTEXITCODE
```

Runner arguments are exactly:

```text
windows_capture_disconnect.exe windows_capture_disconnect:: --nocapture --test-threads=1
```

Runner checks the new executable SHA, rejects an existing result directory, drains both output streams, bounds execution to 30 seconds, preserves native exit, and stores FIRST stdout/stderr/result/exit/executable hash. These tests create no child process or GUI; a timeout kills only the owned test executable.

**CC acceptance:** exactly 3 selected cases, all PASS, native exit 0, no timeout/setup/fixture failure. Review the resumed-prefix/suffix byte-equality assertions and guard/production diff. Merely getting exit 0 with zero selected tests is not GREEN. The compiled executable additionally contains inherited TLS/support tests; the exact filter deliberately excludes them.

### Additional pure transport checks for CC after the three-case GREEN

The existing `remote_transport` suite uses explicit mock-backend workers, not real desktop screenshots/input. Its sibling-binary resolver can use the bundled release Host/Client. After verifying bundled file hashes and CC review, these exact existing cases exercise framing, fresh-session reconnect and EOF without GUI acceptance claims:

```powershell
$Expected = Get-Content -LiteralPath "$Bundle\bundle-sha256.json" -Raw | ConvertFrom-Json
foreach ($Name in @('remote_transport.exe', 'computer-host.exe', 'computer-client.exe')) {
    if ((Get-FileHash -LiteralPath "$Bundle\$Name" -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Expected.$Name) { throw "SHA mismatch: $Name" }
}
$env:RPA_TEST_COMPUTER_HOST = "$Bundle\computer-host.exe"
$env:RPA_TEST_COMPUTER_CLIENT = "$Bundle\computer-client.exe"
& "$Bundle\remote_transport.exe" remote_large_observe_frames_pass_unchanged --exact --nocapture --test-threads=1
$LASTEXITCODE
& "$Bundle\remote_transport.exe" remote_sequential_reconnect_gets_fresh_session --exact --nocapture --test-threads=1
$LASTEXITCODE
& "$Bundle\remote_transport.exe" remote_eof_closes_client_and_host_stays_up --exact --nocapture --test-threads=1
$LASTEXITCODE
```

CC must preserve each FIRST output/native exit using its approved bounded runner/evidence procedure; stop at the first failure rather than rerunning to green. These are optional follow-up commands, not executions performed or claimed here. Do not run the entire root library artifact blindly; native/GUI validation remains separately gated.

Source snapshot for independent CC rebuild: `/private/tmp/windows-capture-disconnect-stage-b-sol-20261008._lfnu54y/source-frozen-stage-b.tar.gz`; SHA256 `f7f66e6138282fcbf3140c9ee295e89709f9f0b47724ef610fd36007d329729b`. Rebuild in NEW Data/Windows temp storage with the same locked compile commands, then record the newly built hashes. The supplied runner hash is specifically for the supplied compiled regression executable; do not bypass it to run an unmatched rebuild.

## Unresolved gates and limits

1. **CC Windows GREEN not yet executed** for this Stage B artifact. Only compile0 is established by author.
2. Independent CC+GLM code/test review pending, including the no-TLS-output fallback and the test-only bridge.
3. No post-fix full-size screenshot, native backend, reconnection GUI or desktop acceptance result. Default-size capture verification starts only after CC pure GREEN and the separate GUI authorization.
4. Four live writer-zero failures and accepted RED establish the transport defect. Exact original failed response sizes and live queue occupancy remain unmeasured; no disk cause, image-size threshold, or resize workaround is claimed.
5. Cancellation/deadline/socket-error implementations are unchanged; this compile-only handoff is not a new runtime certification of those paths.

## Changed paths in this Stage B

- `src/mcp/remote/pump.rs` — one guarded retry arm plus comments and cfg(test)-only forwarding function.
- `tests/windows_capture_disconnect.rs` — new normal Cargo integration entry point.
- `.agents/reports/windows-capture-disconnect-stage-b-sol-20261008.md` — this report.
- `.agents/runs/windows-capture-disconnect-stage-b-sol-20261008/` — new owned small freeze/index/runner/procedure files.

**Unchanged:** `tests/windows_capture_disconnect/write_pending.rs`, Cargo.toml, Cargo.lock, original Stage A artifacts/reports/manifests/RED outputs, all other historical source177 entries. No files or caches deleted. No tests run. Stop at SOURCE_FROZEN.**
