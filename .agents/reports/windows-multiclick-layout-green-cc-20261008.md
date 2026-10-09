# Windows multiclick fixture StageB — bounded layout GREEN (CC), 2026-10-08

Status: TWO authorized layout executions completed on acer-win, each ONCE, in interactive console session 1 via the frozen gui-runner helpers + frozen `windows-test-runner.ps1`; both native exit **0**. Run1 (StageA-linked-StageB exe `--layout`): 50/50 contracts passed. Run2 (GestureStageBLayout.exe, zero args): 4,162/4,162 contract lines passed (raw-byte verified). No input injected, no Host, no visual agent, no capture; no repair/rerun; no production change. This is **layout geometry only — NOT GUI visibility/acceptance**.

## 1. Identity verified before launch (nothing guessed)

- `native-artifacts.json` read from `C:\Temp\windows-multiclick-fixture-stage-b-CC-6b28ccc02de0421c850f2b565266fe58\native-compiled\`; each PE re-hashed remotely, all matched:
  - stage_a `GestureStageA.exe` = `fe2e45a23abe3809b3aa66c2c8437a671c439b34bef0c639e3a6b4f71c420a38`
  - layout_controls `GestureStageBLayout.exe` = `b3a14558b615af3e24ca559e425d8c70e39d1b1c2e02eb12cce4b3bb7f3e24d6`
- Entries inspected in source (repo, unchanged): StageA runner Main accepts exactly `--layout` (1 arg); `GestureStageBLayoutContracts.Main` requires **zero** args (`args.Length!=0` → blocked).
- Frozen runner verified remotely (both at `C:\Temp\cc-stage-b-runner\` AND inside the StageB bundle `source\scripts\`): `windows-test-runner.ps1` `ad2a607c…79572c4`, `windows-test-runner\Runner.cs` `aced18e6…e972214` — match repo originals. Launcher `gui-runner-launch-20260930.ps1` `6a6871a1…cafb9`, stop `gui-runner-stop-20260929.ps1` `d5eb1791…bfb789` — remote hashes equal local `.agents/runs/` copies.
- Precheck: active console session **1**, explorer present, **0** `RpaGuiRunner-*` tasks, 0 GestureStageA/BLayout/Acceptance processes. Notepad **PID 24332** protected (Store Notepad, session 1, created 2026-09-29 16:00:23) — untouched; no name-based kills.

## 2. Run 1 — StageA-linked-StageB exe `--layout` (once)

- Config SHA `426fa29126b3e44804b1a2a5669dca58467a0dc81519bdd817611cbc8849f7f2` (5 fields; `timeoutSeconds=60`); task `RpaGuiRunner-029626382de7467dae45478c9440f5e6` (pid 12684, session 1, NODE1\Administrator), LAUNCH_EXIT=0.
- summary.json: `native_exit_code=0`, `outcome=success`, `timed_out=false`, `stdout_truncated=false` (12,617 B seen/retained), stderr 0 B, `runner_session_id=1`, `root_session_id=1`, `cleanup_state=root_exited`, `executable_sha256` matched, `stdout_drain_complete=true`.
- stdout raw counts: 50 contract lines, **50 `passed:true`, 0 `passed:false`**; summary `{"failures":0,"status":"contracts_passed"}`.
- **multiclick-05 measurements (the original RED)**: `multiclick-05-full-instruction-fits-real-label` **PASSED** — label bounds now `{X=20,Y=124,860×150}`, preferred `{856×148}` (old RED: 838×111 vs 75). `multiclick-05-wrapped-height-fits` **PASSED** — native measured 148 ≤ available **150** (old: 111 > 75). Text keeps full canonical goal + original caveat + appended `Only the whole sentence satisfies this goal.`; `instruction-keeps-user-goal` / `within-form` / `no-visible-overlap` all passed. 10 `layout_measurement` events (all cases, 96×96 DPI, Segoe UI 20pt, no AutoEllipsis).
- Stop helper ran with identity verification: task/pid stopped, record removed by design.

## 3. Run 2 — GestureStageBLayout.exe, zero arguments (once)

- Config SHA `ff5c2e9491b1e6cba1b369c394249755197d3e0e71527669f6a3de3c42cb98cb` (`arguments: []`, `timeoutSeconds=60`); task `RpaGuiRunner-af8c68bdae4d45e48f4e3deecc9344e4` (pid 25924, session 1), LAUNCH_EXIT=0.
- summary.json: `native_exit_code=0`, `timed_out=false`, `stdout_truncated=false` (886,217 B seen/retained), stderr 0 B, `runner_session_id=1`, `root_session_id=1`, `cleanup_state=root_exited`.
- stdout: 4,226 lines = **4,162 contract lines, ALL `passed:true`, 0 failed** (byte-level scan) + 63 `type:"layout"` events + 1 summary `{"failures":0,"gui_proof":false}`. NOTE: 42 scroll-suite `label-unclipped-Panel … row 0 ×0` contract names contain console-codepage artifact bytes (0x19/0x1a for ×/…), so they fail strict JSON parsing; their raw bytes directly show `"passed":true` — counted at byte level, not rewritten.
- Coverage: all **3 suites** (`multiclick`, `drag`, `scroll` — per frozen `GestureCatalog.Suites`) × 10 trials × (`ready` + `checked-no-input`) + 3 `finished` = 63 layout inspections; footer Check/Finish states exercised with **no input**.
- Key spot-checked details: `multiclick-05-ready-minimum-viewport-96dpi` True — `client={900×760}`; `native-input-geometry-unchanged` True — `canvas={860×300}`, sentence 24pt ReadOnly not-multiline; `instruction-font-unchanged` True — Segoe UI 20pt Bold; `whole-line-criterion-visible` True (caveat + whole-sentence sentence both in instruction text).

## 4. Evidence (all in `/private/tmp/windows-multiclick-layout-green-cc-20261008/`; remote originals preserved, nothing deleted/overwritten)

- `layout-first-evidence/`: `stdout.log` `218b7868d8c331f640ed87e96de0f7c4cd8b2a3b65dcacbc739ae54eda9dd9a8`, `summary.json` `97f24a60d21d2a908407d7eff101ab1a4ad2556156d8177d4c022539f33c78bb`, `identity.json` `dfc5a5900b47772e397379ea6430bfa4753784a64f4fddda803542719ae39dbd`, `config.json` = `layout-config.json` `426fa291…9f7f2`, `stderr.log` empty (`e3b0c442…7852b855`).
- `layoutcontrols-first-evidence/`: `stdout.log` `0cbf6f627f77ba40c87f191b7922c5881399b5895bed868eea072adca0e261ed`, `summary.json` `a44d0589cedfc8b40aaa3edcc303cef2faac9aaef57517998dd5d320bf819480`, `identity.json` `08be53abe5981184ab38a7e32b0e7339fbdb64d0674ea90a877a19dd32a0e973`, `config.json` = `layoutcontrols-config.json` `ff5c2e94…cb98cb`, `stderr.log` empty.
- Staged orchestration scripts (`preflight.ps1`, `run1/run2-setup-launch.ps1`, `wait-summary*.ps1`).
- Remote originals under `C:\Temp\windows-multiclick-layout-green-cc-20261008\` (record files removed by the verified stop helper as designed).

## 5. Boundaries at end

- End-state recheck: **0** `RpaGuiRunner-*` tasks, 0 owned processes, Notepad 24332 intact; nonowned apps untouched.
- No commit/push/worktree/delete; no settings/permission/power changes; no subagents; no rerun-for-green (each case ran exactly once, first evidence preserved).
- **These runs prove WinForms geometry only**: a test-created window measured in-process. They do NOT prove the desktop was visibly rendering, that screenshots are live, or any GUI/capture acceptance. The known screen-currency doubt on NODE1 is unresolved and irrelevant to these in-process measurements; no capture was attempted.
- Original RED evidence (`/private/tmp/windows-multiclick-layout-red-cc-20261008/`) untouched, not overwritten, not retroactively claimed.
- Staged per-round scripts left in `C:\Temp\` (preflight/run/wait, this round's own files); evidence roots left intact for audit.
- Stop here per instructions: no real actions, no capture, no native-probe, no full campaign. Next phases need coordinator GO.

## 协调者原始复核

亲读两个原始summary：native0、root/runner Session1、无timeout、双流drain完成、无截断、stderr均0。逐字节保留stdout，ASCII通过标记分别50和4162，失败标记0；第二轮63份layout记录对应3套件×(10 trial×2状态+finished)，不是4162次独立GUI动作。case05最终label高150px、native文字测量148px；较原111px需求变大是因为StageB保留全文并加上整句要求，不是缩短文案。

第二轮原始stdout存在非UTF8 OEM字节，以及箭头转换后的未转义C0控制字节；它不是严格UTF8 JSONL。协调者只以字节保真Latin1映射+宽松JSON解析核对ASCII结构、数字、布尔值，并与原字节标记计数交叉核对；不把该解析当作UI Unicode文字保真证明，不改写原日志。原strict解析丢掉42条contract的中间误计保留在CC transcript，最终为4162/4162。后续如修测试日志编码需另行回归，不悄悄“清洗”旧证据。

机器可读协调索引：`/private/tmp/windows-multiclick-layout-green-cc-20261008/coordinator-readback.json`。CC实际响应模型均glm-5.3-flash；外层native0；两owned任务清理输出已读。当前仍只认证布局几何，scene/freshness/input门禁不改变。
