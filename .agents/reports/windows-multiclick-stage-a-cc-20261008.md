# Windows multiclick Stage A — CC+GLM review and Windows RED, 2026-10-08

Status: both components executed once on acer-win; FIRST outputs preserved; no production patch, no rerun, no GUI/fixture window touched, no nonowned process affected.

## Component A — transport regression review + Windows RED: **ACTUAL RED CONFIRMED**

Review (no blockers):

- Report `.agents/reports/windows-capture-disconnect-stage-a-sol-20261008.md` cross-checked against on-disk evidence.
- Production `src/mcp/remote/pump.rs` SHA `c0516adb97c3299d41e49ed559e39945f43cf0c08e30979e558b75bac55480ba` equals the harness "original pump" identity; `src/mcp/remote/tls.rs` equals `0284fade17ebc8a5060124c7115ba7a280f7b59f2315b4112e2779bc268b09e1`. Regression source `tests/windows_capture_disconnect/write_pending.rs` SHA `1405ec0803a178af804a9161aa17bdbd56cf798e807f42c047a87a5d9861aef2` matches.
- Regression references the original production pump bytes via `#[path]` child-module declaration on a byte-for-byte copy (harness-provenance.json; sole append is the mod declaration); it calls the real private `write_pending`, keeps the production 4 MiB `TLS_BUFFER_LIMIT` and 64 KiB `CHUNK_BYTES`, and uses real rustls client/server with fixture CA. Assertions are exact (`Ok(false)` vs `Err`, retained pending/offset, exact peer-decrypted bytes) — no weak error-only test. Verified against pinned rustls 0.23.45 semantics described in the SOL report (bounded vec_buf capacity zero reachable; not a broken socket).
- Host log `/private/tmp/windows-multiclick-diagnosis-20261008/host-log-current-20261008.log` (4,812 B) actually contains exactly 4 `tls writer accepted 0 bytes` failures (lines 27/28, 35/36, 43/44, 51/52), workers exit 0 after each, final worker (55604) stops clean.
- `shasum -c` of both source freezes (177 refs; owned-run 177+regression) exit 0.

Execution (once, new exclusive remote dirs):

- Bundle zip SHA `74560dcb0dce780e15b6da6d09fb0cd258e8f806ca86925bb9815448723a8d49` verified locally and remotely after scp.
- Remote: `C:\Temp\windows-capture-disconnect-stage-a-sol-20261008-rbt5w_hx.zip` → expanded to NEW `C:\Temp\windows-capture-disconnect-stage-a-sol-20261008-rbt5w_hx\`; frozen `cc-red-runner.ps1` verified PE hash `af3b04de6582d943a59f5edcdd4c72d03c7cc2e81755025204af1868f7dedb8a`, reserved NEW evidence dir, ran once with 30 s bound.
- Result: `running 3 tests` (10 unrelated filtered out); `full_tls_buffer_retains_pending_instead_of_disconnect` FAILED at write_pending.rs:130 with `left: Err("tls writer accepted 0 bytes") right: Ok(false)`; `partial_tls_write_retains_offset_and_resumes_without_replay` FAILED at :167 with the same Err-vs-`Ok(false)` (the strict-partial-progress setup assertion at :162 passed first, so this is valid contract RED, not a setup failure); `writable_tls_buffer_consumes_one_chunk_exactly_once` ok. Native exit **101**, runner exit 101, `timed_out: false`, 0.28 s runtime, no handshake/setup/fixture failure.
- Acceptance criteria from SOL section 5 met exactly. No production patch applied; no rerun for green.

Evidence (Data volume): `/private/tmp/windows-multiclick-stage-a-cc-20261008/first-red/` — `stdout.log` `014127ffe510d866c3195fff50563bdf32382f3f095f99b8480571ca0e2aa91d`, `stderr.log` `731a75c709167b77ed4d1785c4211157c35b59f00fc1f6cea5b7156beff4d45b`, `result.json` `cc57120ffd09f57aee837a5ccbd88effd0ee1e52533071d0ce46c1b78564de22`, `exit.txt` `39b8dc3fc8b44765c8e6f1adee04c5b465e555ab791cc42d0d9e810d5b64297c`, `exe.sha256` `779dd3dfbe1963a8b6d986d25eac86899451074f2e418febfcfc2614772d69ce`. Remote originals preserved at `C:\Temp\windows-capture-disconnect-stage-a-sol-20261008-rbt5w_hx\first-red\`.

## Component B — fixture StageA review + pure replay: **EXPECTED 5 CONTRACT RED CONFIRMED**

Review (no contract/design blockers):

- Bundle `windows-multiclick-fixture-stage-a-sol-20261008.tar.gz` SHA `5dc142023587981012cce59f49a8ed068d89d4ed6b59ade7213525692fa50210` verified locally and remotely.
- All 4 StageA sources reviewed (`GestureStageARunner.cs`, `GestureStageAContracts.cs`, `GestureStageALayout.cs`, `GestureStageANativeProbe.cs`); local 42-file `source-freeze.sha256` `shasum -c` exit 0; `compile-inputs.json` lists 31 explicit sources (27 root + 4 StageA).
- Production judge expectations cross-checked: `GestureJudge.cs` `select_word` requires exact `=="alpha"`; `select_line` accepts any nonempty selection; `right_between` requires left/right/left. 04 tests only the exact recorded trailing U+0020 with raw-preserved companion assertions; no global trimming/normalization anywhere. 05 empty/partial selections must be rejected as whole-line success; 08 must stay rejected for model omission. Capability classification is emitted only as `capability_diagnostic` with `pending_native_observation_and_coordinator_approval` — not treated as a proven input-kernel failure and not counted as semantic pass. `cc-compile-only.ps1` uses explicit `C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe`, verifies the full freeze, refuses non-fresh OutDir; integrity guards throw exit 2 (blocked), never masquerading as RED.

Execution (once, fresh staging):

- Remote stage: NEW `C:\Temp\windows-multiclick-fixture-stage-a-sol-20261008-CC-7baf7efa133f41b88d403ff410516a15\`; tar extract OK; `cc-compile-only.ps1` passed all 42 source-hash checks and native csc compile exit 0 → `GestureStageA.exe` SHA `82d32ac8331442b14015a21068e1016bfa866aa613d8895d7fd85920fd368700` (Windows PE, CC-recorded; not required to equal the Mono artifact). Oracle hash `2fe4a01115cce6cdddda1aae133111b598516638638f123682f95835c5061add` verified before replay.
- ONE pure `--replay` run, bounded 30 s via existing frozen `scripts/windows-test-runner.ps1` + `Runner.cs` copied unmodified (remote hashes `ad2a607ca0af59983b306ba5f6c32611005669a6b7d385834ccceb027a9572c4` / `aced18e616afe881845b2ac233e32a190b7186b69492aaa7be416ac41e972214`). Runner session 0 is acceptable: replay is pure, no GUI. No `--layout`, no `--native-probe`.
- Result: native exit **1** (harness exit 1), `timed_out: false`, 0.16 s, stdout 9,814 bytes retained, stderr empty, `status=contracts_red`, `failures=5`. Each contract inspected individually:

  RED (5, exactly the named expectations):
  1. `04-recorded-native-alpha-U0020` failed — judge rejects recorded `"alpha "` (raw UTF16 `0061 006C 0070 0068 0061 0020` preserved by companion raw-preserved check, which passed).
  2. `05-partial-is-not-whole-line-"alpha"` failed — judge wrongly accepts word as whole line.
  3. `05-partial-is-not-whole-line-"alpha "` failed.
  4. `05-partial-is-not-whole-line-"beta"` failed.
  5. `05-partial-is-not-whole-line-" "` failed — even a single space is misaccepted as whole-line success today.

  Controls (all behaved as required, none assumed):
  - `04-exact-alpha` passes; all 8 `04-reject-*` negatives, missing-release, no-native-dblclk, outside-time, outside-slop, release-outside all rejected as expected.
  - `05-recorded-empty-not-success` stays rejected (empty never accepted as success); `05-whole-sentence-positive` passes; `05-whole-line-with-missing-release` rejected; `05-delivery-only-three-presses` passes only as isolated delivery evidence with explicit "does not fulfill line goal".
  - `08-recorded-final-left-omission-rejected` stays rejected; `08-native-right-present` passes (real WM_RBUTTONDOWN present; no fixture/kernel patch for model omission).
  - Every `*-raw-preserved` companion passed — selections never rewritten.

- No precondition/parse/source-drift exit-2 occurred; nothing was blocked.

Evidence (Data volume): `/private/tmp/windows-multiclick-stage-a-cc-20261008/replay-first-evidence/` — `stdout.log` `f2c1f2be6ac873d0633a86ba24961b91861b069a93f1ae43a5cebedc81602f2a`, `summary.json` `f72594e6657aa26a85f1e80b784ef60eca4ced3481fadc77c0c303256f28f80c`, `identity.json` `48206f6b6754d9d7129ad67616f7200db3a99785d6f1fec684af8e819b0c6b0b`, `config.json` `d3c5fecee19417c298ec079644a642d5125a43ba51ea3af71d4464ec53918f48`, `stderr.log` empty (`e3b0c442…7852b855`). Remote originals preserved under the stage GUID directory above.

## Boundaries and state at end

- Production source unchanged (no edits to any implementation or expected assertion; `pump.rs`/`tls.rs`/judge/fixtures hashes verified before and untouched after).
- No GUI/host/browser/fixture window or input on any platform; only SSH console transfer/execution; nothing running remains (both runs exited; runner `cleanup_state=root_exited`).
- No name-based/global kills; Notepad24332 and all nonowned processes untouched.
- No worktree/commit/push/reset; no permission or global-setting changes; no old artifacts deleted or moved; /Volumes/doc received only this report (volume at 100%, 73 MiB free — no large writes).
- Next gates are coordinator-owned: Stage B pump patch (after RED acceptance), 05 capability-classification policy + bounded native diagnostic, layout/native-probe GO. STOP here per instructions; no repair/retest without coordinator.
