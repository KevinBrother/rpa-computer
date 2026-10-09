# Windows capture disconnect — Stage B GREEN + default-size GUI verification — CC+GLM — 2026-10-08

Role: CC+GLM orchestrator (outer orchestration only; screenshot interpretation is the inner remote GLM's, final visual review is the coordinator's). No subagents; no source/product/test/helper edits; no commit/push/worktree/reset; no settings/secrets/firewall/global changes; no deletions/moves. All raw artifacts in `/private/tmp/windows-capture-disconnect-green-cc-20261008/` (Data volume); only this report in the repo.

## 0. Coordinator interruption (recorded, not hidden)

Mid-task the coordinator terminated outer PID 21616 (exec 80691, exit 143) during read-only prechecks. Confirmed afterward: no owned setup had started (no bins replaced, no tasks/records, 8399 free). Same Claude session resumed (`45c1a60e-66fc-475d-ab0e-8bf7abe7b9ec`, `--model sonnet`, actual GLM).

## 1. Phase 1 — independent review + Windows pure GREEN

**Code review PASS.** `src/mcp/remote/pump.rs` SHA `d360c0e6…`; one guarded arm `Ok(0) if conn.wants_write() => return Ok(false)` (pump.rs:281) exactly per `production-change.patch`; original fatal arm retained (line 282). The arm never touches `pending`/`*off`; zero-acceptance without TLS output stays fatal (no retry spin). No limits raised; socket errors/WouldBlock/cancel/drain paths byte-unchanged; cfg(test) bridge is a pure forwarder (absent from release). Integration entry `tests/windows_capture_disconnect.rs` SHA `2655fc8c…` includes real production pump/tls via `#[path]`; frozen regression `write_pending.rs` SHA `1405ec08…` byte-identical to accepted RED. Drift record: 177 historical refs, 176 unchanged, pump only intentional change. Bundle SHA `12a0498a…` verified locally and after transfer.

**Windows runs** (all raw in `phase1/`; frozen bundle staged at `C:\Temp\windows-capture-disconnect-stage-b-sol-20261008-lfnu54y`, remote SHA verified):

1. Required 3-case regression via author's `cc-green-runner.ps1`: exactly `windows_capture_disconnect::` 3 passed, 0 failed, native exit **0**, no timeout, exe SHA `d7d6a857…` = frozen (`first-green/`).
2. Author-documented pure transport (bundle SHAs gated; env host/client = bundled release bins):
   - `remote_large_observe_frames_pass_unchanged` — **see retest below**.
   - `remote_sequential_reconnect_gets_fresh_session` — PASS, native exit **0** (10.51s).
   - `remote_eof_closes_client_and_host_stays_up` — PASS, native exit **0** (10.32s).
3. **Coordinator-mandated retest** of case 1: my first ad-hoc attempt printed libtest PASS but captured an EMPTY `Process.ExitCode` (PowerShell handle bug) — not authoritative, Phase 1 was NOT claimed complete. One explicitly approved repeat with the **unchanged frozen `scripts/windows-test-runner.ps1` + Runner.cs** (staged byte-identical, SHAs `ad2a607c…`/`aced18e6…`; config = exactly the 5 allowed fields, 120s): `native_exit_code=0`, `outcome=success`, exe SHA `7eecd490…` (same frozen exe), drained, `retest-large-observe/`. FIRST (uncaptured-exit) output preserved untouched; no relabeling. (First retest attempt failed validation — evidence parent dir absent, Win32=3; parent created, leaf not reserved, same config rerun once.)

**Phase 1 = pure GREEN.** No full-library run, no live-backend tests. No claim of total product green.

## 2. Phase 2 — deployment (frozen Stage B bins)

Pre-state verified: installed host `1AEEDCD9…` / client `2F1F8335…` (expected prepatch), fixture `3A5FA228…`, 0 owned tasks, 8399 free, Notepad 24332 alive. Exclusive backup `bin-backup-pre-stageb-cc-20261008\` (both bins + `host.log.pre-stageb`, hash-verified); pre-replacement host.log SHA `89A2A3AE…` (4812 B). Frozen `computer-host.exe c8c1348f…` / `computer-client.exe 445675c3…` staged, hash-verified, installed into the same owned `bin\` paths, re-verified. No source copy, no credential edits, no rollback performed.

## 3. GUI run — one session, describe/open/observe×2/close

Launch (all Interactive ScheduledTask, session 1, direct TLS 100.200.20.168:8399, DHCP re-confirmed same IP):

| Component | pid | task |
|---|---|---|
| Host (Stage B) | 30612 | `RpaComputerRemoteHost-f8482e545f934e6aa035d9fee8452fe9` |
| Fixture (multiclick) | 31576 | `AccFixture-18f708386f624ab0a829cfa3e12cbe88`, run dir `run-18f70838…` |
| Backdrop | 29036 | `RpaGuiRunner-1e494c36f2fd47e7a1fcd9cd16dc0cd3` |
| Inner agent | 26468 | `RpaGuiRunner-d825e37bc94b40ccbe14449ca01d5b24` |

Evidence dir `evidence-20261008-green-cc-01\`; inner run `d2b6d7df-6853-4f70-a209-5314bcdc9e7c\`. Task file SHA `b4499770…` (1394 B): describe/open(default, no size overrides)/observe×2/close, zero input. MaxTurns 12 / MaxSeconds 180, one attempt.

**Inner result: exit_code 0, not timed out, stderr empty.** Actual model per transcript message: all 9 = `glm-5.3-flash` (alias sonnet). Tool calls exactly 5: `computer_describe` → `computer_open` (empty input — default primary display, NO max_width/max_height) → `computer_observe` ×2 (same session `session-2712-1`) → `computer_close` (`cleanup_outcome: released`). Zero step/get_step/pause/resume/input. Windows audit script (existing, hash `d4c22dcd…` both sides, separate verifier dir `C:\Temp\verifier-green-cc-20261008`): **AUDIT OK (17 checks), tools=8, calls=5, turns=6, duration 46.5s**.

**Transport verification (the Stage B target): PASS.** Two default-size observations in the SAME session both succeeded — no disconnect, no `tls writer accepted 0 bytes`, no truncation. PNGs extracted unchanged from tool_result: `phase2/observe-1.png` and `observe-2.png`, both **43871 bytes, SHA256 `ad19b5e4…`, 1365×768** (PNG IHDR), byte-identical. Topology metadata: native display 1920×1080, capture `image_rect` 1365×768 — the tool's default behavior downscales; no override was passed by me or the inner agent. host.log new segment (602 B, prefix byte-identical to backup): clean single session — authenticated → worker exit 0 → stopped; the historical 4 errors are in the pre-4812 prefix, none new.

**Scene verification: FAILED — honestly stopped.** The inner GLM reports (from the screenshots) that **no "Computer Use Acceptance" window is visible** — terminal/PowerShell windows and a blank center window instead; no suite/case/trial/nonce/controls visible; no security/firewall dialog. Per the stop rule it called `computer_close` and stopped — no reopen, no resize, no fallback. Read-only diagnosis: fixture pid 31576 still alive (empty cross-session MainWindowTitle — inconclusive); **backdrop pid 29036 was already GONE** at diagnosis (died early; no backdrop visible in captures is consistent). Notepad 24332 untouched throughout. **Do not treat this run as a fixture-scene pass; coordinator must visually review the PNGs.**

## 4. Cleanup (identity-checked helpers only)

- `stop-windows.ps1`: `AccFixture-18f70838…` stopped+unregistered (identity verified); "pid 31576 already ended with the task" = known normal post-stop path.
- `gui-runner-stop` ×2: backdrop + inner tasks/records removed, evidence preserved.
- `windows-remote-stop.ps1`: host task + pid 30612 stopped, record removed, logs/secrets in place.
- Final: 0 owned tasks, 8399 = 0 listeners, Notepad 24332 alive. No claim that process exit proves held-input cleanup (zero input this run). No screenshot/input data sent anywhere.

## 5. Limits / boundary

- Phase 1 proves the frozen regression + documented pure transport cases on this Windows host — not the whole test suite, not the product.
- GUI run proves: repeated **default-size** capture transfer in one session works on the current desktop (Stage B pump fix behaviorally effective for the observed path). It does NOT prove all screen sizes, backpressure, network conditions, or any input behavior.
- The controlled fixture scene was NOT established (window missing, backdrop dead) — this is a scene failure, recorded, not retried (one-attempt rule).
- Capture at 1365×768 vs native 1920×1080 is the tool's default behavior with no overrides; whether that default is intended is an owner question, outside this run.
- source-free = tool-restricted, not OS isolation proof. Oracle snapshot: pre-stop SHA `FE00E559…` (811 B, live read), post-stop `70829C1D…` (1096 B, fetched) — coordinator-only, never given to inner.

## 6. Evidence locations

- Local Data: `/private/tmp/windows-capture-disconnect-green-cc-20261008/` — `phase1/` (first-green, remote-transport-first ×3, retest-large-observe, runner scripts+configs+logs), `phase2/` (deploy scripts+logs, inner-run/{transcript.jsonl,run-summary.json,stderr.log}, observe-1.png, observe-2.png, host-start/fixture-start/backdrop-start/inner-start logs, audit-transcript.log, cleanup logs, hostlog/{pre-stageb, final, new-segment.bin}, oracle-evidence.post-stop.jsonl).
- Remote (preserved): `C:\computer-cc-preflight-20260930-191500\evidence-20261008-green-cc-01\` (run dir + oracle + inner run dir), `bin-backup-pre-stageb-cc-20261008\`, `bin-stage-stageb-cc-20261008\`, `host.log`; `C:\Temp\windows-capture-disconnect-stage-b-sol-20261008-lfnu54y\` (bundle + all test evidence); `C:\Temp\windows-capture-disconnect-green-cc-20261008\retest-large-observe\`; `C:\Temp\windows-test-runner-cc-20261008\`; `C:\Temp\verifier-green-cc-20261008\`.
- Coordinator visual review targets: `/private/tmp/windows-capture-disconnect-green-cc-20261008/phase2/observe-1.png` and `observe-2.png` (identical bytes).

## 协调者最终原始核对及边界收窄

已亲读3回归native0/3pass、large-observe冻结runner重验native0与完整输出、重连/EOF两项exit0；首个大帧测试缺失exit证据及协调中断不抹除。已逐项核对内层恰好1open({})+2observe同会话+close、实际GLM，两PNG与原始tool image字节一致；新host.log是旧4812字节前缀之后追加的干净602字节，无新的writer0错误。

已实际查看PNG：测试夹具不在画面中，只有终端与前景空白窗口，因此**scene_verified=false**。截图任务栏可见日期2026/10/3、时间11:33，与本轮执行日志2026-10-08不一致，两帧又逐字节相同；这些是待核查的画面时效/主机时钟/桌面状态疑点，不能凭此直接认定后端缓存或陈旧帧，也不能宣称已证明图像实时性。回压处理的代码回归已绿，默认参数的图片传输已成功；不把它扩大成当前正确桌面的完整截图验收。

此前“backdrop died early explains scene”没有确切创建/退出时间及因果链，只能称检查时未见该进程、场景不成立。报告“无截图数据发送任何地方”应仅理解为无额外对外发送；远端GLM观察本身必须接收截图，不能否认模型调用含图像。

综合核对：`/private/tmp/windows-multiclick-diagnosis-20261008/coordinator-final-green-readback.json`。本批actor已终态；没有第三次盲重试或输入。后续先诊断场景/画面时效，并补修后布局实测，再继续动作GUI；不需要据此重设已通过的网络授权。

### 协调者读图勘误（2026-10-08 后续诊断）

对原始 `phase2/observe-1.png` 任务栏区域作无内容修改的裁剪放大后再次目检：日期为 **2026/10/8**，先前读成 `2026/10/3` 有误；时间看起来是 **11:37**，先前 `11:33` 亦不可继续当作精确事实。因此撤回“五天前日期”的疑点，不根据旧错误读数推导缓存故障。原图及原始转录均保留不变。分析派生图在 `/private/tmp/windows-scene-diagnosis-cc-20261008/taskbar-analysis-{crop,enlarged}.png`；裁剪区域 x=1240/y=725/w=125/h=43，放大6倍。首次本地Pillow不可用，随后使用系统sips，仅为读图分析，不是产品/测试实现。

测试窗口未出现在截图中的事实不变；时钟读数与本轮执行时刻仍需远端会话/显示状态证据解释，`scene_verified=false`、`freshness_verified=false` 暂不改变。
