# feedback::tests_process native exit 证据补采（Windows rerun）— CC 报告

日期：2026-09-30。执行者：CC（config sonnet alias，实际 glm-5.3-flash）。
范围：仅 5 个 `feedback::tests_process::...`，evidence-only rerun，用于补采上一批
缺失的**数值 native exit**。无 product/test 源码改动；未 commit/push/worktree；
Mac 仅文件编排，从未执行。

## 0. 对上一批报告的更正

上一批（`.agents/reports/desktop-feedback-process-windows-cc-20260930.md`，
**原文未改**）已追加更正段：其 `exit_code=null`，summary "ok" 行 ≠ native exit；
本报告提供独立的数值证据。上一批原始记录（stdout/stderr/身份 JSON）原样保留于
`.agents/runs/desktop-feedback-process-windows-cc-20260930/logs/win/`，不覆盖。

## 1. 冻结核对（与上一批一致，未重编）

- lib 测试 exe `rpa_computer-3769e6ba7e450727.exe` SHA256
  `c92b747fec8d400cc3174deed967ddfbd84d712a18a19f05881e4c36c5452a5b`（从上一批
  TEMP 复制到新 TEMP 后 Get-FileHash 复核一致）。
- fake renderer `fake_renderer.exe` SHA256
  `8c10725cf1b4a7b14a11b9a3834fc96d2c8a091f9987cf718cbb33efed024831`（同上复核；
  其源 `fake_renderer.rs` 冻结值上批已核对，本批未触碰）。

## 2. 方法（仅临时编排改动）

- 新独立 TEMP：`C:\Users\Administrator\AppData\Local\Temp\dfb-process-exit-cc-20260930`
  （与另一 CC 的 TLS 纯 lib 测试 TEMP 完全不相交，未触碰其任何任务/进程/目录）。
- 冻结 helper 不变：`gui-runner-launch-20260930.ps1` /
  `gui-runner-stop-20260929.ps1`（双端 SHA256 复核同前批值）。ScheduledTask、
  Interactive principal、Limited、active console session、显式 PowerShell 数组
  wrapper（`launch-wrapper-exit.ps1`）。
- 新自有 runner `test-runner-exit.ps1`：supervisor 先落盘 runner+child 身份
  （pid/exe/session/creation ticks/parent/双 SHA256），子 powershell **直接调用**
  测试 exe（`--ignored feedback::tests_process --test-threads=1 --nocapture`，
  stdout/stderr 流式落盘），随后 `Set-Content native-exit.txt $LASTEXITCODE;
  exit $LASTEXITCODE`。supervisor 有界 `WaitForExit(300000)`，超时按
  pid+exe+creation-ticks+session 四元身份复核后杀；fake 子进程残留审计规则同前批。
  **不信任缺失 marker**：`exit_record_exists=false` 即如实报告（本批即曾如实
  报告过一次缺失，见 §3）。

## 3. 两次启动（第一次失败如实记录，未掩盖）

1. **attempt-1**（task `RpaGuiRunner-a7b167f958d74236a7bb320b80944a3f`，child
   pid 26768，session 1）：测试再次 5/5 ok（17.98s），但 exit marker 写入命令中
   的 `"native_exit="` 字面量经 Start-Process 嵌套引号传递后引号被剥
   （stderr 可见 `native_exit= : The term ... is not recognized`），marker 未生成
   → 该次 native exit **未捕获，按缺失如实处理**，不采信。task 经 stop 脚本按
   身份验证后停止注销（stop 输出存档于本 run 目录）。
2. **attempt-2（采信批次）**：child 写纯数字 marker。task
   `RpaGuiRunner-d182edb59df245cfb4e3e88925f10750`，runner pid 19900、child
   powershell pid 19264，均 session 1（active console），parent_pid 19900。

### attempt-2 结果

- **数值 native exit = 0**（`native-exit.txt` 内容 `0`，由子 powershell 的
  `$LASTEXITCODE` 在直接调用测试 exe 后立即写入；`exit_record_exists=true`）。
- harness 计数：`test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured;
  240 filtered out; finished in 17.92s`（5/5 与上一批一致）。
- supervisor `test-status.json`：outcome=exited（自退出，远小于 300s），无超时
  身份杀，`residual=[]`。supervisor 的 `child_exit_code` 字段仍为 null
  （PowerShell Process 对象 ExitCode 不可靠 —— 这正是改用 exit marker 的原因）；
  **数值 exit 以 marker 为准，Process.ExitCode 不作为证据**。
- stderr 仅两条设计内 revoke 消息，同上一批。
- 一次运行采信，无重跑变绿（attempt-1 是失败原样记录）。

## 4. 回收与残留核查（attempt-2 后）

- stop 脚本身份验证后停止并注销 task d182edb5（输出："stopped GUI runner:
  task 'RpaGuiRunner-d182edb59df245cfb4e3e88925f10750', pid 19900; record
  removed. Evidence ... preserved"）。
- `Get-ScheduledTask -TaskName 'RpaGuiRunner-*'` 为空；无
  `rpa_computer-3769e6ba7e450727.exe` / `fake_renderer.exe` 进程；
  `rpa-feedback-*` 目录 0 个；无 ambiguous 项。
- Notepad pid 24332 运行前后均在，未触碰。无 GUI/网络/安全设置改动。

## 5. 产物索引

- 本 run：`.agents/runs/desktop-feedback-process-exit-cc-20260930/`
  - `test-runner-exit.ps1`、`launch-wrapper-exit.ps1`（CC 自有临时编排）
  - `logs/win/{runner-identity,child-identity,test-status}.json`、
    `logs/win/native-exit.txt`、`logs/win/tests-{stdout,stderr}.log`、
    `logs/win/runner-transcript.txt`、`logs/win-evidence-hashes.txt`
- Windows TEMP：`C:\Users\Administrator\AppData\Local\Temp\dfb-process-exit-cc-20260930`
- 上一批 run/报告：未改动（除其报告文件追加更正段）。

## 6. 结论

**5/5 通过，数值 native exit = 0**（marker 证据，哈希冻结一致，active console
session，有界监督未触发，全部身份匹配回收）。上一批 null-exit 记录作为独立
历史保留，未被本批覆盖或改写。
