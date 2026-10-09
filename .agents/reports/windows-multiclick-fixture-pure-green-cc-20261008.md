# Windows multiclick fixture StageB — CC+GLM PURE review/green, 2026-10-08

Status: source review complete (no blockers); ONE native csc compile + TWO pure console modes executed once each on acer-win, both native exit **0**. No GUI, no `--layout`, no `--native-probe`, no `--layout-contracts`, no Host/fixture window, no desktop ops; Session 0 runner sessions are acceptable here because both modes are pure console (confirmed by source inspection, not assumed). No implementation/runner edits, no rerun, no production change beyond the StageB author's already-frozen delta.

## 1. Source review (author gpt-6.1-sol; bundle verified)

- Bundle `/private/tmp/windows-multiclick-fixture-stage-b-sol-20261008/windows-multiclick-fixture-stage-b-sol-20261008.tar.gz` SHA `bf1618e86f467514b9eca6e9336aa94c0dcfb92c1e96675b11ac4c8517a1ea26` (354,306 B) — verified locally AND remotely after scp. Extracted to NEW local review dir and remote stage `C:\Temp\windows-multiclick-fixture-stage-b-CC-6b28ccc02de0421c850f2b565266fe58\` (GUID, new).
- `readiness.json` matches the SOL report (3 compile-only PEs, `changed_existing_files` = GestureJudge.cs + GestureForm.cs, `new_code_files` = GestureFormLayout.cs + GestureStageB/GestureStageBLayoutContracts.cs, tests_executed_by_author=false, nativeprobe deferred, no unsupported classification). Compile inputs use the author's exact names: `compile-inputs.json` with `product`(28)/`stage_a`(32)/`layout_controls`(29) source lists.
- In-archive `source-freeze.sha256` (48 files, manifest SHA `722f2ce43ed2bc847cf8212c8257ee78a193cb7e0c41980cb0f8ecfc8923a2ed`) — `shasum -c` exit 0 locally; the remote `cc-compile-only.ps1` re-verified all 48 before csc.

Review findings (against the required semantics):

- `GestureJudge.cs`: `select_word` now accepts exactly `"alpha"` or `"alpha\u0020"` (comment explicitly calls it case-local, not whitespace normalization; raw data untouched); `select_line` now requires exactly `GestureCatalog.SelectWordSentence` — partial/empty can never pass; reason string uses the shared `WholeLineRequirement` const. All other guards (ValidStream ordering/timing, multi bounds/slop/DBLCLK, area/release, outside, invalid_count, right_between left/right/left, drag/scroll) are byte-identical to the StageA-frozen judge. 04's count/time/slop/button/area/DBLCLK/ordered-release guards all preserved; no trim/normalize/rewrite of `SelectedText`, observed, or raw logs; nothing extended to other cases/platforms.
- `GestureForm.cs`: `LayoutTrial()` invoked ONLY at StartTrial / Check / Next-Finish boundaries (never in `Record`/`WndProc`); canonical goal text unchanged, select_line keeps the original caveat and appends `Only the whole sentence satisfies this goal.`; current-trial `platform_expected` is now `"windows: "+GestureJudge.WholeLineRequirement` (string value only; field names/types/event structure unchanged). Fonts unchanged (Segoe UI 20pt/21pt/30pt Bold, EDIT 24pt); input geometry (860×300 canvas, 425×260 scroll panels) not resized.
- `GestureFormLayout.cs`: measures actual `GetPreferredSize` + `TextRenderer.MeasureText` per label with 2px slack; reserves max-int counter text for the live row to avoid reflow growth; explicit fail-closed `InvalidOperationException` if the working area cannot fit the full readable fixture (no silent off-screen/shrink); minimum client stays 900×760.
- **Unchanged confirmations**: all 4 StageA sources byte-identical to their old freeze hashes (`e132e6ba…`/`ed60bfd2…`/`26e8e654…`/`159c026c…`); `GestureCases.cs` = old canonical pin `456108a7…`; `tasks/multiclick.md` = `8c2dc2bf…`. `GestureSelfTest.cs` `word-selection-native-valid` (exact alpha) remains valid and PASSED. No old self-test assertion was rewritten.
- **Legacy contradictions FLAGGED, not fixed and not treated as product criteria** (as already documented by the author): `GestureCases.cs:107` old Windows note still says "pass = 3 downs + nonempty" (runtime Form now uses the real whole-sentence standard); `scripts/analyze-gesture-gui.py:198` still accepts only exact `alpha`, so a real `alpha ` word selection may be reported absent by the old analyzer pipeline. Also `GestureStageAContracts.cs` still emits the old `capability_diagnostic` line (`pending_native_observation_and_coordinator_approval`) — that is unchanged OLD test code, not a new product classification. No fake selection, no delivery-only success, no new unsupported classification introduced anywhere.
- This report SUPERSEDES the old arbitrary nonempty-selection acceptance semantics for select_line: whole-line success now means the exact full sentence, and 04 allows only the exact word or word+single U+0020.

## 2. Native compile (exact author script; artifacts identity)

`cc-compile-only.ps1` (repo+in-bundle, verified) with explicit Framework64 `csc.exe`, `/main` per mode, product as `winexe`, tests as `exe`. All three modes csc exit 0. Windows PE SHAs (CC-recorded; not required to equal Mono PEs):

| mode | exe | SHA256 |
|---|---|---|
| product | `...\native-compiled\ComputerUseAcceptance.exe` | `bff857b932538ac9d41600a3c02d68ec2bdd8f7e12468d7d8eae929b7c1eb471` |
| stage_a | `...\native-compiled\GestureStageA.exe` | `fe2e45a23abe3809b3aa66c2c8437a671c439b34bef0c639e3a6b4f71c420a38` |
| layout_controls | `...\native-compiled\GestureStageBLayout.exe` | `b3a14558b615af3e24ca559e425d8c70e39d1b1c2e02eb12cce4b3bb7f3e24d6` |

`GestureStageBLayout.exe` is **compile-only in this task** — never executed (live layout contracts are a separate coordinator-gated phase). Authoritative oracle hash `2fe4a01115cce6cdddda1aae133111b598516638638f123682f95835c5061add` verified before any run.

## 3. Pure mode 1 — original StageA runner `--replay` (bounded 30 s)

Source-inspection gate for mode eligibility: StageA runner replay path is pure JSONL judging (verified in the StageA review; unchanged here). Run via EXISTING frozen `windows-test-runner.ps1` + `Runner.cs` (remote hashes re-verified `ad2a607ca0af59983b306ba5f6c32611005669a6b7d385834ccceb027a9572c4` / `aced18e616afe881845b2ac233e32a190b7186b69492aaa7be416ac41e972214`), 5-field config, NEW evidence leaf `replay-first-evidence`.

- **Native exit 0**, runner exit 0, `timed_out=false`, no truncation (stdout 9,698 B seen/retained), stderr 0 B, `root_session_id=0` (pure console — permitted), root pid 31180, PE SHA matched `fe2e45a2…`, runtime ~0.14 s.
- `{"type":"summary","failures":0,"status":"contracts_passed",...}` — **all originally-5-RED contracts now pass**, each inspected individually:
  - `04-recorded-native-alpha-U0020` passes (exact trailing-U+0020 allowance; raw-preserved companion `UTF16=0061 006C 0070 0068 0061 0020` still passes — no trimming).
  - `05-partial-is-not-whole-line-"alpha"` / `"alpha "` / `"beta"` / `" "` now correctly REJECTED (exact whole sentence required) — empty/partial never positive.
- Controls all pass: `04-exact-alpha`; all 8 `04-reject-*` (including NBSP, TAB, double-space, leading-space, wrong word); missing-release/no-DBLCLK/outside-time/outside-slop/release-outside; `05-recorded-empty-not-success` (rejection correct — NOT 05 success); `05-whole-sentence-positive`; `05-whole-line-with-missing-release`; `05-delivery-only-three-presses` (still labelled as isolated delivery evidence, not line success); `08-recorded-final-left-omission-rejected` (rejection correct — NOT 08 success) + `08-native-right-present`. Every `*-raw-preserved` companion passed.

## 4. Pure mode 2 — production `--self-test` (bounded 60 s)

Source-inspection gate: `MainForm.cs` `Program.Main` handles `--self-test` in a branch that runs only `SelfTest.Run()` + Gesture/Basic/Focus/FocusRegression/NativeText/Geometry self-test classes and `return`s BEFORE any WinForms initialization/`Application.Run`/form creation (console modes documented at MainForm.cs:14); grep over all self-test sources found zero `Application.Run`/`ShowDialog`/`Show()`/input APIs. Gate satisfied → run allowed.

- **Native exit 0**, runner exit 0, `timed_out=false`, no truncation (stdout 20,562 B seen/retained), stderr 0 B, root pid 10004, PE SHA matched `bff857b9…`, runtime ~0.25 s.
- **Actual stdout evidence captured (winexe console-attachment caveat addressed: the redirected pipe captured full output — success is NOT inferred from an empty file or exit code alone):** `SELF-TEST PASSED (287 checks)`; line-by-line count: **287 `PASS:` / 0 `FAIL:`**. Includes 46 gesture-suite checks; `gesture-word-selection-native-valid` (exact alpha) passes unchanged. Note: some stdout bytes are legacy console-codepage artifacts around em-dashes (previously documented by the author); PASS/FAIL markers and names are ASCII and were used for counting.

No other mode was executed; no unexpected nonzero occurred (so no later check was stopped); no rerun for green.

## 5. Evidence (Data volume; nothing deleted/moved/overwritten)

`/private/tmp/windows-multiclick-fixture-green-cc-20261008/`:
- `replay-first-evidence/`: `stdout.log` `6a5ca3ff03394e872a73009baefbb427dad0a85b8e3c6bed75e446731b49e634`, `summary.json` `3ac41451edcc525319eec01eb628507e08603f6f27d667ca84d123330b33ed18`, `identity.json` `68cd44201c5d7cb0c07670d67ed9b5240d8df875c822c437929c1253aa989201`, `config.json` `ae4c01db2544bb1b193c2f60a6dc923ce59e8d74ca3107f47035aa613a07f316`, `stderr.log` empty (`e3b0c442…7852b855`).
- `full-self-test-first-evidence/`: `stdout.log` `340cf3b501c328682dc2fbb881e96fd6281a85e4f00193b385f961896fa82328`, `summary.json` `bddbb95ca8f31fb22872d257165c46c868b8de8cf5ed4794d976a1c39620a3cf`, `identity.json` `0c7b75edb8dfb8191cc02e4c55f697c9c826f5eccde682385f766de7fd22630d`, `config.json` `3cf7ea2579b64e5873837ae29e9f642e174a71c18d957e1ce7f9f36802ca9e27`, `stderr.log` empty.
- Local bundle extraction, `compile-driver.out`, `pure-driver.out`, driver scripts.
- Remote originals preserved under `C:\Temp\windows-multiclick-fixture-stage-b-CC-6b28ccc02de0421c850f2b565266fe58\` (`native-compiled\native-artifacts.json` + both evidence leaves). Old StageA RED evidence and all partial outputs untouched.

## 6. Scope distinction and state at end

- These were PURE console checks only: frozen-oracle judge replay + full self-test. They are DISTINCT from and do NOT imply: (a) live `--layout` re-measurement of the new GestureForm layout (StageBLayout compile-only here; separate coordinator gate), (b) real-GUI selection/observation acceptance on the new fixture (default-capture GUI actor owns the acer-win desktop this round; no GUI op performed by this task).
- Production delta remains exactly the StageB author's frozen change set; nothing replaced in existing runtime files/fixtures; only NEW console test stage created. No commit/push/worktree/delete; no settings/permissions changes; no macOS/Linux tests; /Volumes/doc received only this report.
- Stop here per instructions; further GUI/layout/native-probe phases need coordinator GO.
