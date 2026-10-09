# feedback::tests_process Windows 集成门禁（fake renderer，无真实 GUI）— CC 报告

日期：2026-09-30。执行者：CC（config sonnet alias，实际 glm-5.3-flash）。
范围：仅 lib 中 5 个 `feedback::tests_process::...` IGNORED 测试，使用
`BackendFactory::Mock` + fake renderer，无窗口/截图/原生输入/网络。未改任何
product/test/fixture 源码；未 commit/push/worktree；Mac 上只做交叉构建，从未执行。

## 1. 源码冻结与产物核对

- `src/feedback/fixtures/fake_renderer.rs` SHA256 =
  `62ba8a0c11886689b076dff3e774ab5e998faaa95075d19177118424b5256453` — 与任务
  冻结值**完全一致**，未改动。
- lib 测试 exe 从上一批 JSON 记录取精确路径
  `.agents/runs/desktop-feedback-host-windows-cc-20260930/target/x86_64-pc-windows-msvc/debug/deps/rpa_computer-3769e6ba7e450727.exe`，
  SHA256 `c92b747f...452a5b` 与该批 `logs/exe-hashes.txt` **完全一致**（未重编
  root，遵守"另一 owner 正在修 TLS fixture"约束）。

## 2. 测试授权审查（只读）

`src/feedback/tests_process.rs`（173 行）5 个测试逐个确认：
- 全部 `#[ignore = "CC Windows fake renderer required"]`，仅本批 5 个被
  `--ignored feedback::tests_process` 精确选中（240 filtered out 证实无越界）。
- Fixture 仅把 `RPA_FEEDBACK_TEST_RENDERER` 指向的文件复制到
  `%TEMP%\rpa-feedback-<testpid>-<counter>\fake_<mode>.exe`（tests_process.rs:21-28），
  Drop 时 remove_dir_all。
- 无窗口创建、无截图、无 SendInput、无网络；`BackendFactory::Mock`；
  fake renderer 是 std-only 管道协议回放器（stdin/stdout 帧），不画 GUI。
- 结论：符合授权范围。

## 3. fake renderer 交叉构建（Mac，仅构建）

沿用上一批 cargo-xwin + rust-lld 环境（build-env.sh 的 eval/DYLD/AR 导出），
standalone rustc（std-only，无新依赖/工具）：

```
cargo xwin env --target x86_64-pc-windows-msvc +
rustc --target x86_64-pc-windows-msvc --edition 2021 \
  -C linker-flavor=lld-link -C linker=rust-lld -C strip=symbols \
  -o fake_renderer.exe src/feedback/fixtures/fake_renderer.rs
```

exit 0，0 诊断（logs/rustc-fake-renderer.stderr 为空）。产物
`fake_renderer.exe`：PE32+ console x86-64，SHA256
`8c10725cf1b4a7b14a11b9a3834fc96d2c8a091f9987cf718cbb33efed024831`。

## 4. Windows 执行（acer-win，ACTIVE console session）

新独立 TEMP：`C:\Users\Administrator\AppData\Local\Temp\dfb-process-cc-20260930-r1`。
lib exe + fake exe 上传后 certutil/Get-FileHash 双端 SHA256 一致。
启动走冻结 helper `gui-runner-launch-20260930.ps1`（未改动，双端 hash 一致）：
Interactive principal、RunLevel Limited、console session 1、explorer-owner
预检由 helper 完成；ScriptArgs VERBATIM、名称均带前导 `-`：

```
-TestExe  ...\rpa_computer-3769e6ba7e450727.exe
-Renderer ...\fake_renderer.exe
-OutDir   ...\dfb-process-cc-20260930-r1
-BudgetSeconds 300
```

自有 runner `test-runner.ps1`（仅设置进程局部 `RPA_FEEDBACK_TEST_RENDERER`）：
先落盘 runner/test 身份（pid、exe、session、creation ticks、双 SHA256、parent pid），
再 `Start-Process -RedirectStandardOutput/-Error`（流式文件，非 Out-String 缓冲），
执行 `--ignored feedback::tests_process --test-threads=1 --nocapture`，
`WaitForExit(300000)` 有界监督；超时则按 pid+exe+creation-ticks+session 四元
身份复核后才 Stop-Process。任务身份：runner powershell pid 16764 / test pid
29684，session 均 = 1（active console，非 SSH Session 0）。Console 未解锁、
未切换桌面、无用户窗口被隐藏。

### 结果（一次运行，无重跑）

```
running 5 tests
  eof_and_heartbeat_loss_revoke_and_reap_bounded .............. ok
  fake_wire_stop_reaches_worker_and_old_open_resume_and_queued_step_are_denied ok
  live_heartbeat_cannot_hide_a_permanently_blocked_snapshot_pipe ok
  stalled_reader_does_not_block_native_fact_publication_and_reaps ok
  startup_rejects_missing_no_ready_unsupported_and_invalid_frames ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 240 filtered out;
finished in 17.95s
```

- 实际计数：**5 passed / 0 failed / 0 ignored**； outcome=exited（自退出，
  远小于 300s 预算），未触发身份杀。声明：test-status.json 的 exit_code 字段
  为 null（Process 对象 ExitCode 未取到），以 harness "ok" 汇总行 + 自退出为
  原生退出证据。
- stderr 仅两条设计内 revoke 消息（HeartbeatTimeout、snapshot write deadline
  exceeded），对应 eof/heartbeat_loss 与 stall 测试的预期 fail-stop 路径。
- 无第一失败、无超时；未发生任何 retry。

## 5. 回收与残留核查

- fake 子进程：runner 结束后审计 `%TEMP%\rpa-feedback-*` 下本 run 创建的进程 →
  0 残留（tests 自身 Reap + Fixture Drop 生效）；`rpa-feedback-*` 目录 0 残留；
  无 ambiguous 项（不强制清理任何未证实进程）。
- 调度任务：`gui-runner-stop-20260929.ps1` 按记录四元身份验证后停止并注销
  本 run 任务；另有一个本批次早期失败尝试遗留的任务
  `RpaGuiRunner-1b26c6fc...`（action 指向本 TEMP 的 probe.ps1，身份核实为本批
  所有物）一并注销。之后 `Get-ScheduledTask -TaskName 'RpaGuiRunner-*'` 为空。
- 用户进程：Notepad pid 24332 运行前后均在（MainWindowTitle 未变化），未触碰。
- 未启动真实 Host/默认 renderer/真实截图/输入/防火墙交互；无 GUI、无网络。

## 6. 过程性阻塞与定位（已解决，非产品缺陷）

前 3 次启动失败均为**launcher 调用通道问题**：经 SSH/cmd 调用时
`-ScriptArgs` 逗号列表被 cmd 压扁成单个字符串，runner 收到整串位置参数即
退出（exit 1，无任何文件；未触碰桌面）。用最小 probe 通过同一机制定位后，
改为经 `powershell -File` 的临时 wrapper（本 run 目录 `launch-wrapper.ps1`，
evidence 文件，非产品）传显式数组后一次成功。失败的 task/record 均按 stop
脚本契约回收；中间未向 Session 0 执行过任何测试。

## 7. 局限声明

- fake renderer 的 `capture_exclusion:"requested"` 帧只是协议回放，**不构成**
  真实截图排除证据；真实 stop/overlay/WM affinity/截图排除仍需默认 renderer +
  GUI 门禁（未授权，未做）。
- 本 run 为进程/IPC/生命周期验证；未覆盖真实 pipe DACL 与桌面一致性之外的
  Windows 安全语义。
- exit_code 未捕获（见 §4），如需严格 native exit 可后续重跑补录。

## 8. 产物索引

- 本 run：`.agents/runs/desktop-feedback-process-windows-cc-20260930/`
  - `fake_renderer.exe`（+ `logs/rustc-fake-renderer.stderr`、`logs/local-exe-hashes.txt`）
  - `test-runner.ps1`、`launch-wrapper.ps1`、`probe.ps1`（CC 自有临时脚本）
  - `logs/win/{runner-identity,test-identity,test-status}.json`,
    `logs/win/tests-{stdout,stderr}.log`, `logs/win/runner-transcript.txt`,
    `logs/win-evidence-hashes.txt`, `logs/run-identities.md`
- Windows TEMP：`C:\Users\Administrator\AppData\Local\Temp\dfb-process-cc-20260930-r1`
- 上一批 target / release / 部署基线 / TLS fixture：未触碰。

## 结论

**授权范围内门禁通过**：5/5 `feedback::tests_process` 在 active console
session 以 mock backend + 冻结 fake renderer 通过（17.95s，有界监督未触发），
进程/任务/fake 子进程全部身份匹配回收，用户 Notepad 未受影响，无需返 Codex。

## 更正（2026-09-30 追加，原文保留不改）

§4/§8 中把 harness "ok" 汇总行 + 自退出称为"原生退出证据"是**不严谨的**：
summary 行与进程 native exit code 是两个不同的证据。本批 test-status.json 的
`exit_code` 为 null（PowerShell Process 对象 `ExitCode` 未取到），因此本批
**没有捕获到数值 native exit**，不得声称。原始 stdout/stderr/身份记录按原样
保留（`logs/win/tests-stdout.log` 等），本更正不改动任何原始记录。数值
native exit 由后续独立 evidence-only rerun 单独采集，报告见
`.agents/reports/desktop-feedback-process-exit-cc-20260930.md`。
