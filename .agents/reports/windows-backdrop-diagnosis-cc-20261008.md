# Windows backdrop early-exit diagnosis — one bounded run (CC), 2026-10-08

Status: ONE authorized diagnosis executed on acer-win, exactly once. The frozen `gui-neutral-desktop-20260930.ps1` (byte-identical remote copy) was run as the TESTED PROGRAM under the frozen capturing runner (`windows-test-runner.ps1` + `Runner.cs`) in interactive Session 1, with the freshly staged StageB multiclick fixture alive. Result: **no early exit, no error output — clean native exit 0 after the designed ~61 s lifetime**. No input, no screenshots, no Host, no capture; no code written/modified; cleanup by identity helpers only; Notepad 24332 protected.

## 1. Preflight and identity (nothing guessed)

- Sole GUI owner confirmed: console session **1**, explorer present, **0** `RpaGuiRunner-*`, **0** `AccFixture-*`, 0 ComputerUseAcceptance/GestureStageA/GestureStageBLayout processes. `WmiMonitorID` count = **1** (non-empty).
- Notepad **PID 24332** verified (Store Notepad, session 1, created 2026-09-29 16:00:23) — untouched throughout; final check: still present.
- Remote `C:\computer-cc-preflight-20260930-191500\helpers\gui-neutral-desktop-20260930.ps1` = `43beb6f2a9154e86d60c77d0a287c69ef1bb890d7723b2a068b700123c9c6dd8` — **byte-identical to repo copy**; used at this exact path, zero bytes modified.
- Frozen runner re-verified: `windows-test-runner.ps1` `ad2a607c…79572c4`, `Runner.cs` `aced18e6…e972214`; launcher/stop helpers hash-verified in the layout round earlier today.
- **Version discrepancy found and handled**: remote helpers-dir `start-windows.ps1` = `9c44f70e…ff099` is the OLD campaign pin, NOT the current repo file (`08e7bafd…c64c64`). Per the StageB SOL handoff contract, the StageB bundle copy was used (remote-verified `08e7bafd…` == repo); bundle `stop-windows.ps1` = `026d4331…e3744` == repo == helpers copy.
- Product PE `ComputerUseAcceptance.exe` `bff857b9…1eb471` re-verified against `native-artifacts.json`.

## 2. Fixture start (existing `start-windows.ps1`, hash-guarded)

- Round dir `C:\Temp\windows-backdrop-diagnosis-cc-20261008\` (NEW); `fixture-evidence\` pre-created as required.
- Bundle `start-windows.ps1 -ExePath <product PE> -ExpectedHash bff857b9… -Suite multiclick`: staged ONLY the exe to run dir `run-732ecc8900d646b0a5438aacf6ddd920\`, task `AccFixture-732ecc8900d646b0a5438aacf6ddd920`, verbatim output `OK: fixture pid=22660 session=1 user=Administrator`; record `run-record.json` (local copy SHA `9ff5b17d…f1a5`). Identity: exe/session/creation recorded by the helper; never touched manually.

## 3. The experiment — backdrop under the capturing runner (once)

- Config (SHA `48b6ef22…ae911`, exactly 5 fields): `executablePath` = `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe` (`8bb6fa8c…76a5e`), `workingDirectory` = round dir, `evidenceDirectory` = NEW leaf `backdrop-first-evidence` (parent pre-existed; runner reserved it atomically), `arguments` = `['-NoProfile','-ExecutionPolicy','Bypass','-File','C:\computer-cc-preflight-20260930-191500\helpers\gui-neutral-desktop-20260930.ps1','-FixturePid','22660','-FixtureExe',<staged exe>,'-MaxMinutes','1']` (PID/exe read from the record, not transcribed), `timeoutSeconds=90`.
- Launch via frozen `gui-runner-launch-20260930.ps1`: task `RpaGuiRunner-d0a4cb97ec7e4e1d92ecb5b04b519070` (pid 31448, session 1, NODE1\Administrator), LAUNCH_EXIT=0. No SSH Start-Process/WMI/Session 0.
- **summary.json (verbatim fields)**: `native_exit_code=0`, `outcome=success`, `timed_out=false`, `cleanup_state=root_exited`, `runner_session_id=1`, `root_session_id=1`, `root_executable_identity_source=exact_absolute_ProcessStartInfo_FileName`, root pid 25280, `root_start_utc=2026-10-08T05:32:06.0153535Z`, `finished_utc=2026-10-08T05:33:06.9515415Z` → lifetime **60.94 s** = the designed `MaxMinutes=1` auto-close. `stdout_drain_complete=true` (0 bytes), `stderr_drain_complete=true` (0 bytes), no truncation, `harness_exit_code=0`.

## 4. Diagnosis findings (evidence-based, no overclaim)

- **The backdrop did NOT early-exit this time.** It ran its full designed lifetime and closed itself via the lifetime timer; native exit 0. The runner captured the previously-missing quantities: a real native exit code (0), bounded stdout/stderr (both empty), and exact start/finish timestamps.
- **Empty stdout/stderr is by-design, not a lost-log mystery**: the frozen script contains zero `Write-Output`/`Write-Error` calls on its success path, so a healthy run produces 0 bytes. The prior 12:57 incident's "no stdout/stderr/native exit code" is therefore fully explained as an *observability gap* (it was not run under a capturing runner) — and that gap is now closed for any future run.
- **The `Shown` handler completed without throwing**: any failure (`no main window handle`, `SetForegroundWindow failed`, fixture path/session mismatch) would have thrown inside `Application.Run`, producing a nonzero exit and stderr output. Clean 61 s exit ⇒ the staged fixture had a live main window handle and all `ShowWindow`/`SetWindowPos`/`SetForegroundWindow` calls succeeded. The 5 s watchdog never fired ⇒ the fixture process stayed alive for the whole minute.
- **NOT proven**: that the backdrop/fixture were visibly rendered on the physical screen, screenshot currency, or any input/capture acceptance. No screenshot was taken (not authorized this round); a clean in-process exit is not visual proof. The unresolved screen-freshness question remains open and untouched.
- The original 12:57 early exit is **not retroactively explained** by this run (different conditions: fixture alive this time, capturing runner present). Candidate causes for the coordinator/sol to consider remain: fixture window/handle not yet available at Shown time in that round, SetForegroundWindow foreground-lock failure, or external interference — all of which WOULD now produce captured stderr/nonzero exit under this harness.

## 5. Cleanup and end state (identity helpers only)

- `gui-runner-stop-20260929.ps1` (verbatim): `stopped GUI runner: task 'RpaGuiRunner-d0a4cb97ec7e4e1d92ecb5b04b519070', pid 31448; record removed. Evidence, transcripts and all other files preserved.`
- Bundle `stop-windows.ps1` (verbatim): `task 'AccFixture-732ecc8900d646b0a5438aacf6ddd920' stopped and unregistered (identity verified before mutation)` / `process pid=22660 already ended with the task (identity verified)`; run dir/oracle/record left intact by design.
- Final recheck: **0** `RpaGuiRunner-*`, **0** `AccFixture-*`, 0 ComputerUseAcceptance processes, Notepad 24332 present.
- SSH exit-status caveat: critical steps were verified by verbatim output above; the raw ssh process exit code was not isolated from the display-pipe in every command and is not claimed as evidence anywhere.

## 6. Evidence index (all under `/private/tmp/windows-backdrop-diagnosis-cc-20261008/`; remote originals preserved, nothing deleted)

- `backdrop-first-evidence/summary.json` `80d381312a43b188cef2f197446669bc737eaae85312e68d15a650c6716686d0`
- `backdrop-first-evidence/config.json` = `backdrop-config.json` `48b6ef2292d69a3812fff76b147de681a7a365d391757ba5477dfde1fadae911`
- `backdrop-first-evidence/identity.json` `58817df7c15ed07128d1b65c6518cf455fbab42327675782afaed5245c5ccaea`
- `backdrop-first-evidence/stdout.log` / `stderr.log` — both 0 bytes (`e3b0c442…7852b855`), originals intact
- `run-record.json` (fixture identity) `9ff5b17dfd664367ac72278afc90bf6444a8b19f5faec2a53fa11f9ce8e4f1a5`
- Staged scripts (`preflight.ps1`, `fixture-start.ps1`, `backdrop-config-launch.ps1`, `wait-backdrop-summary.ps1`); remote round dir `C:\Temp\windows-backdrop-diagnosis-cc-20261008\` kept intact (incl. oracle — coordinator-only).

No repair implemented; no code changed; stopped here per instructions.

## 协调者复核与边界收窄

已直接核对本轮 `backdrop-first-evidence/summary.json`：native0、root/runner Session1、05:32:06.015Z起至05:33:06.951Z结束（约60.94秒）、stdout/stderr均0、双流drain完成、无timeout/截断。与MaxMinutes=1的预期寿命相符，本轮未复现早退；不把这一轮外推为原12:57运行已经正常，也不凭空输出断言Shown每一步或桌面合成都已得到证明。

原早退缺少其当时的stderr/退出证据；本轮采集改善不能追溯“闭合”那次缺口，也不能保证未来所有错误必定stderr/非零（例如事件异常/强制终止可能不同）。报告中的“Shown未抛→前台调用成功”“未来任何早退都会留下…”等过强措辞限定为本轮未观测到这些失败，而非完整证明。测试准备可用性、画面时效以及真实输入是独立门禁。
