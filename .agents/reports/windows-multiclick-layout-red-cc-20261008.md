# Windows multiclick fixture StageA — bounded layout RED (CC), 2026-10-08

Status: ONE authorized `--layout` execution completed on acer-win in interactive console session 1 via the frozen gui-runner helpers; native exit **1** (contract RED), no timeout, no precondition/blocked exit 2. No repair, no rerun, no production change. This was UI layout measurement of the actual production labels — NOT GUI computer-use success and no clicks/input were sent.

## 1. Pre-execution review and identity verification

- Scope re-confirmed from frozen `GestureStageALayout.cs` (SHA `ed60bfd205a02c722639dbc6f55e93e2795f426e6a07719e204835b0f897fbb6`): constructs the real `GestureForm`, iterates all 10 multiclick trials through the production `StartTrial`, measures the actual `instruction` Label (`GetPreferredSize` + `TextRenderer.MeasureText` WordBreak/TextBoxControl) in the `Shown` handler, checks fits/wrapped-height/goal-preservation/within-form/no-visible-overlap, then closes itself in `finally`. No input injection, no fake measurements, no layout constants copied.
- Frozen test runner verified on remote before launch: `windows-test-runner.ps1` SHA `ad2a607ca0af59983b306ba5f6c32611005669a6b7d385834ccceb027a9572c4`, `windows-test-runner/Runner.cs` SHA `aced18e616afe881845b2ac233e32a190b7186b69492aaa7be416ac41e972214` (match originals in `scripts/`). Runner Param inspected, not guessed: exactly `-ConfigPath` (mandatory, single).
- Frozen exe verified at the exact authorized path: `C:\Temp\windows-multiclick-fixture-stage-a-sol-20261008-CC-7baf7efa133f41b88d403ff410516a15\compiled\GestureStageA.exe` SHA `82d32ac8331442b14015a21068e1016bfa866aa613d8895d7fd85920fd368700`.
- Helpers used unchanged: `C:\computer-cc-preflight-20260930-191500\helpers\gui-runner-launch-20260930.ps1` (params: `-ScriptPath/-EvidenceDir/-ScriptArgs/-ExpiryMinutes/-RecordDir`; verbatim pass-through, Interactive principal, Limited RunLevel, Session-0 refusal, GUID-unique task, strict process identity) and `gui-runner-stop-20260929.ps1` (param `-RecordPath`; full identity revalidation before any stop). Local copies hashed at `6a6871a10d26e37b401f5fdb28a5c9669025579043cbd3b3237e79306f0cafb9` / `d5eb179105dc5576add87e8cfa225878bdc61d42e28349f62411d55b9bbfb789`.
- Pre-check: active console session **1** with explorer.exe present, **0** `RpaGuiRunner-*` tasks, **0** GestureStageA processes — no conflicting GUI tasks. Notepad24332 and all nonowned apps untouched; no name-based kills.

## 2. Execution (once, bounded)

- Config (fresh, UTF-8 no BOM, SHA `54cde087c174487c6bae6069a00bebb2620a5c746b5648187400ec4ecfa103fd`): `executablePath` = frozen exe above; `workingDirectory` = the StageA CC stage dir; `evidenceDirectory` = `C:\Temp\windows-multiclick-layout-red-cc-20261008\layout-first-evidence` (NEW); `arguments` = `["--layout"]`; `timeoutSeconds` = 30.
- Launch (verbatim terminal evidence): `GUI runner task 'RpaGuiRunner-abf9b1028b4e40409547458bf374a077' started (pid 5024, session 1, user NODE1\Administrator); record: C:\Temp\windows-multiclick-layout-red-cc-20261008\runner-instance-RpaGuiRunner-abf9b1028b4e40409547458bf374a077.json; evidence: C:\Temp\windows-multiclick-layout-red-cc-20261008; expiry: 30 min.` `LAUNCH_EXIT=0`.
- Runner result (`summary.json`): `native_exit_code=1`, `outcome=native_failure`, `timed_out=false`, `cleanup_state=root_exited`, **runner_session_id=1** (interactive, not Session 0), root pid 1916, `root_session_id=1`, `root_executable_identity_source=exact_absolute_ProcessStartInfo_FileName`, stdout 12,567 bytes seen/retained (`stdout_truncated=false`), stderr 0 bytes, harness exit 1. Run finished in ~0.7 s (root start 04:32:43.2Z → finished 04:32:43.9Z). Full short-lived root identity WAS captured (no identity limitation to report).
- Cleanup (verbatim terminal evidence): `stopped GUI runner: task 'RpaGuiRunner-abf9b1028b4e40409547458bf374a077', pid 5024; record removed. Evidence, transcripts and all other files preserved.` Stop helper exit 0 (identity-verified; the instance record is removed by the helper by design, so it is not part of the fetched evidence).

## 3. Layout measurements (actual production labels, DPI 96×96, Segoe UI 20pt, label 860×75 at (20,110), client 900×760)

| Case | Full instruction text (as rendered) | Preferred (W×H) | Fits 75? |
|---|---|---|---|
| multiclick-01 | "Click the big TARGET rectangle exactly once (a single left click)." | 826×37 | yes |
| multiclick-02 | "Double-click the big TARGET rectangle (two rapid left clicks)." | 798×37 | yes |
| multiclick-03 | "Triple-click the big TARGET rectangle (three rapid left clicks)." | 796×37 | yes |
| multiclick-04 | "Double-click the word alpha in the sentence below to select it." | 814×37 | yes |
| **multiclick-05** | "Triple-click the sentence below to select the whole line (per this platform's convention). On Windows native EDIT, report the visible selection; whole-line selection is not guaranteed." | **838×111** | **NO — 111 > 75** |
| multiclick-06 | "Click zone A once, then click zone B once (two separate single clicks)." | 811×74 | yes |
| multiclick-07 | "Click once near the LEFT edge of the TARGET, then once near the RIGHT edge (far apart)." | 843×74 | yes |
| multiclick-08 | "Left-click the TARGET once, then RIGHT-click it once, then left-click it once." | 762×74 | yes |
| multiclick-09 | "Click the TARGET once, wait at least 2 seconds, then click it once again." | 842×74 | yes |
| multiclick-10 | "Attempt a click with an INVALID count of 0 using your click tool. The app must receive no clicks at all." | 846×74 | yes |

## 4. Failing contract names (exactly 2; each inspected individually)

- `multiclick-05-full-instruction-fits-real-label` — FAILED: `actual preferred={Width=838, Height=111} bounds={X=20,Y=110,Width=860,Height=75}` (preferred height 111 exceeds the fixed 75px label by 36px).
- `multiclick-05-wrapped-height-fits` — FAILED: `native measured height=111 available=75` (TextRenderer native wrap measurement, WordBreak|TextBoxControl).

All other checks on 05 passed: `instruction-keeps-user-goal` (full text starts with the canonical goal; caveat did not replace it), `instruction-within-form`, `instruction-no-visible-overlap` — i.e. the clipping is INSIDE the label's fixed 75px bounds (text extends below the label's visible area; the pre-existing screenshot evidence of case-05 clipping is independently corroborated by these numbers). No other case failed; cases 06–10 measure 74px, only 1px under the 75px available height.

Summary line: `{"type":"summary","failures":2,"status":"contracts_red",...}`, native exit 1. This is the layout contract RED the SOL author predicted ("layout预计 05 完整 instruction 的高度回归 RED") with actual Windows measurements now recorded; no DPI/font surprises at 96 DPI / Segoe UI 20pt.

## 5. Evidence (Data volume; nothing deleted or moved)

`/private/tmp/windows-multiclick-layout-red-cc-20261008/`:
- `layout-first-evidence/stdout.log` SHA `0fc2506a083ef4582ce084663a7cd36f18752c619f51b73e1acf831481564d70` (12,567 B, byte-preserving)
- `layout-first-evidence/summary.json` SHA `cd5c57945f1c5a5a8aadda3a1063588939612f880cc012e3f16f1093af2b8985`
- `layout-first-evidence/identity.json` SHA `68c86a32891144f3a3c37b42164961ae4cb58dd69b79d7e9a9086b7e3529b056`
- `layout-first-evidence/config.json` SHA `54cde087c174487c6bae6069a00bebb2620a5c746b5648187400ec4ecfa103fd` (matches remotely written config)
- `layout-first-evidence/stderr.log` empty (`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`)
- Staged driver/precheck/wait scripts (`setup-config.ps1`, `launch-driver.ps1`, `precheck.ps1`, `wait-summary.ps1`) and helper copies (hashed above).

Remote originals preserved under `C:\Temp\windows-multiclick-layout-red-cc-20261008\` (record file removed by the verified stop helper as designed).

## 6. Boundaries at end

- Production source unchanged (layout production NOT changed); StageA sources remain frozen; no worktree/commit/push/reset; no settings/firewall/credential changes; no subagents.
- No `--native-probe`, no clicks/keys/input, no Host started, no other test suite executed; only this one bounded run.
- Owned ScheduledTask cleaned through the identity helper; 0 leftover GestureStageA processes expected (root exited on its own before cleanup); nonowned processes untouched.
- No repair/retest; next steps (layout redesign vs capability-split, 05 policy decision) are coordinator gates.
