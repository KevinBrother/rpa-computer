# Windows focus StageA — RED CC verification — 2026-09-30

- 执行身份：CC（真实 Windows 机器 acer-win，经既有 SSH，console 会话；非 Session 0）。仅执行纯 `--self-test` console 模式，无 GUI/输入/截图/Host/renderer/网络。
- 结论：**PREDICTED RED 确认。** native exit = **1**，唯一失败为 `focus-args-accepts-focus-suite`（parser `unknown suite: focus`），既有 **143 项全部保持 PASS**，总计 144。无超时、stderr 为空。**未修改任何源码/断言/构建脚本/全局设置**；未 commit/push/worktree；Notepad24332 与无关进程未触碰。

## 1. 冻结源码复制与哈希核对

按 SOL 报告 `.agents/reports/windows-focus-fixture-red-sol-20260930.md` 的 15 文件清单 + coordinator 批准的 runner 2 文件，共 **17 文件**以 base64-over-ssh 管道复制到新唯一 Windows TEMP（保留原相对路径布局）：

- 远端 staging（保留未删）：`C:\Users\Administrator\AppData\Local\Temp\computer-focus-red-cc-69d85709_5316_4706_8747_2148ba53`
- 远端 `Get-FileHash` 全量核对（`remote-sha256.txt`，已归档）：**17/17 与冻结列表逐字节一致**（15 fixture 与 frozen-source-sha256.txt 一致；runner `windows-test-runner.ps1=ad2a607c…`、`Runner.cs=aced18e6…` 与 coordinator 冻结表一致）。本地 repo 侧 shasum 预先核对亦一致。

## 2. 构建与 exe 身份

- 使用远端 staging 内的冻结 `acceptance-fixture/build-windows.ps1`（Framework64 csc v4.0.30319），`-OutDir <staging>/buildout`。
- 构建退出码：**0**（`build-exit.txt`，真实 `$LASTEXITCODE`，非 null）；build stderr 空。
- 新 exe 身份：`buildout\ComputerUseAcceptance.exe`，SHA-256 `85735653ccc11dc9684cd3b223bf3bfc091561e1b2b40bc4a63f26db50a9bcc3`（本地归档重算一致）。未复用任何旧部署/compile-only exe。

## 3. 执行：仅 `--self-test`（非 `--suite focus --self-test`）

Coordinator 批准的 hash-verified runner（`scripts/windows-test-runner.ps1` + `Runner.cs`，staging 副本哈希已核对）以 config JSON 驱动：

```json
{"executablePath":"...\\buildout\\ComputerUseAcceptance.exe","workingDirectory":"<staging>","evidenceDirectory":"...\\selftest-evidence-9455456fa1be4e97a89e0722adbad9f6","arguments":["--self-test"],"timeoutSeconds":120}
```

调用 wrapper 为 on-disk `invoker.ps1`（`System.Diagnostics.Process` 启动 `System32\WindowsPowerShell\v1.0\powershell.exe` 运行 runner，缓存 handle、`ReadToEndAsync` + `WaitForExit(170s)`，持久化真实整数退出码；无 Start-Process nullExitCode=>0）。

### 结果（raw 均已归档）

| 项 | 值 |
| --- | --- |
| runner（harness）exit | `1`（`runner-exit.txt`，真实整数） |
| `summary.json` `native_exit_code` | **`1`**（真实整数） |
| `outcome` | `native_failure`；`timed_out=false`；`cleanup_state=root_exited` |
| stdout | 8673 bytes，drain 完整无截断；**143 行 PASS + 1 行 FAIL = 144** |
| stderr | **0 bytes** |
| FAIL 行 | `FAIL: focus-args-accepts-focus-suite ? required --suite focus; parser rejected with ArgumentException: unknown suite: focus` |
| 末行 | `SELF-TEST FAILED (1/144 checks failed)` |

与 SOL 预测完全一致：既有 143 项结果无回归，新增 1 项因未修改 parser 拒绝 focus 而失败，native exit 1。**这是预期 RED 的确认，不是 green，也不是修改断言/生产的授权。** 所有失败确为 focus parser 断言，无其他失败、数量变化或异常。

## 4. 残留核对（read-only，未执行任何 kill）

按**精确 owned 身份**（`ExecutablePath` 与本次 buildout exe 全路径小写比较，含唯一 staging GUID，命令行中不出现路径避免自匹配）枚举 `Win32_Process`：**无任何残留进程**（`residual-check.txt`）。未使用名称/全局 kill；runner `summary.json` 显示 root 已自行退出（`root_exited`），无需 kill。

## 5. 归档

`.agents/runs/windows-focus-fixture-red-cc-20260930/`：

- `remote-staging.tgz` + 解包全量：17 个冻结源码副本、`remote-sha256.txt`、`build-*`、`buildout/ComputerUseAcceptance.exe`（85,735653… 身份可复核）、`runner-config.json`、runner stdout/stderr/exit、`selftest-evidence-9455456fa1be4e97a89e0722adbad9f6/`（config/identity/stdout/stderr/summary）、`residual-check.txt`、全部 helper（hashcheck/build/invoker/residual .ps1）。
- 远端 TEMP staging 与 evidence 目录**全部保留未删**；未覆盖任何旧 snapshot/输出/报告。
- 过程备注：首次传输脚本因 zsh 未分词整串当一个文件名（0 文件写入，立即重试成功）；一次远端 hash 脚本输出被控制台吞掉，改为落盘文件读取。均为 wrapper 侧返工，冻结源码零改动。

## 6. 限制（不隐瞒）

- Runner 无 descendant containment（`summary.json` 已声明）；本次 root 自行退出、无残留，不证明所有后代回收语义。
- 本轮仅覆盖 pure parser 自测路径：**`--self-test` 的 RED 不证明 focus GUI/窗口/路由行为**；focus suite 仍无任何实现（StageB 未开始）。
- `root_session_id=0`（Session 0 服务会话经 SSH 派生），console GUI 行为仍待授权窗口验证，本轮未涉及。
- 实际模型：glm-5.3-flash（按 brief 记录）。

**STOP：报告完毕。StageB 实现方等待本 RED 证据与明确 GO。**
