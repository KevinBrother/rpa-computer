# Windows renderer Desktop StageB — CC independent GREEN verification (2026-09-30)

状态：**GREEN_CONFIRMED（一次调用成功）**。acer-win 原生 Framework64 csc `build_exit=0`；经协调者批准的 bounded test runner 执行纯 `--self-test`：**native exit=0，stderr 原文 `self-test: 235 checks passed (pure; no GUI/ready/input)`，stdout 空，未超时**。与 SOURCE_FROZEN 报告 `windows-renderer-desktop-sol-20260930.md` §5 的预测（235 = 147+16+72、exit=0、stderr 文本、stdout 空）**逐字一致**。仅验证纯 self-test；**不**验证 GUI Stop/click-through/capture exclusion。

## 冻结源核对

Mac 侧与 acer-win 上传后 `Get-FileHash` 复核（`upload-listing.txt`）均与冻结表逐字节一致：

| 文件 | SHA-256 | vs 冻结报告 |
|---|---|---|
| windows/IPC.cs | f24d5b1029da08c74d99aba6ad6fbe9b2f58a9a656a4af23fbaf2156a0170ba3 | 同 StageB/StageA |
| windows/JsonValue.cs | 8d23af10506240d9f72be513d3d846a73363a6691013ca885675ec5e9a03ab4a | 同 |
| windows/Model.cs | 60fbf87a0ac5519cdf358a31aa56227e763bd601a26fe9525efa085bc1b79f72 | StageB 新值，一致 |
| windows/Native.cs | 74e33605d0f82909bf21c3568ccf453c4bf79213ed792184598d60cef1c2b911 | 同 |
| windows/Program.cs | de88c1a2170a526eabc5a425baad0672c2201298c0144a6dc24f1eedd85a8982 | StageB 新值，一致 |
| windows/Protocol.cs | 265fe9dbaafb08b1a31793e14140221299fac98f0f00b4a0ee4085031cf89b31 | 同 |
| windows/SelfTests.cs | c7f619d8999e658cbed4a95a32b5f741b79fc39581c133f824ec58bcb51bc588 | StageB 新值，一致 |
| windows/Windows.cs | 459e1b19ff9628b47ee14633d9bd75c60595e69aa523e3bda711445b3c10c2bf | StageB 新值，一致 |
| windows/app.manifest | 4f5f2c276e99b0ada7cbd8ec34324717029642aeec86a3f1ab1eb7fe669d0089 | 同 |
| windows/desktop-feedback-windows.exe.config | 9d49e908618fe07944a81351c5c8e4495b9320028543d8bc7fdfdf9cb44a5c7c | 同 |
| build-windows.ps1 | f2e113e490d8be515053a91a78109530b16a9b04ed2fdcc03e96198d7f86d19e | 同 |
| scripts/windows-test-runner.ps1 | ad2a607ca0af59983b306ba5f6c32611005669a6b7d385834ccceb027a9572c4 | 同 runner CC 报告 |
| scripts/windows-test-runner/Runner.cs | aced18e616afe881845b2ac233e32a190b7186b69492aaa7be416ac41e972214 | 同 |

runner 自身 `summary.json` 内 `runner_source_sha256` / `runner_module_sha256` 亦为上述值（runner 运行时自核）。

## 执行环境（全部保留，未删）

- 目标机：acer-win（SSH BatchMode）。全新 TEMP：`C:\Users\Administrator\AppData\Local\Temp\windows-renderer-desktop-green-cc-20260930-210752-83215`，正确 `root\build-windows.ps1` + `root\windows\*.cs` + `root\scripts\windows-test-runner.ps1` 布局。
- Mac 侧证据：`.agents/runs/windows-renderer-desktop-green-cc-20260930/`（tempdir、upload-listing、invocation1.ps1、invocation1.output/ssh.stderr、residual-check、`selftest-evidence/`：summary.json、identity.json、config.json、stdout.log、stderr.log、build.stdout、build.stderr）。
- 旧 RED 证据（`windows-renderer-desktop-red-cc-20260930*`）未触碰。

## 构建（原生数字证据）

- 调用：on-disk `invocation1.ps1` 用子 PowerShell（`System32\WindowsPowerShell\v1.0\powershell.exe -File <TEMP>\build-windows.ps1 -Output <TEMP>\build\desktop-feedback-windows.exe`）实际捕获数字 `$LASTEXITCODE`。
- **build_exit=0**；build.stderr 空；build.stdout：`Built ...\build\desktop-feedback-windows.exe (not launched; not acceptance)`
- exe SHA-256：`AA6550ABB044A988717575BBF7CB5AD1E5328EFFA5C9EAB42B02E366CFCA5F01`（runner `executable_sha256` 独立复核一致，小写同值）

## Self-test（经批准的 bounded runner，非 Start-Process 样例）

- config.json（sha256 `c5dee336…739f5d5`，见证据）：五属性 `{executablePath, workingDirectory, evidenceDirectory(全新保留目录), arguments:["--self-test"], timeoutSeconds:100}`；runner schema 校验通过。
- 外层 Mac 侧监督 120s（实测 5s 完成）；runner 内部 100s 上限未触发。
- **runner harness exit = 0**（`invocation1.output` 数字行 `runner_exit=0`；summary `harness_exit_code=0`）
- **native exit code = 0**（summary `native_exit_code:0`，真实整数；非 null coercion）
- `outcome=success`、`timed_out=false`、`cleanup_state=root_exited`（自然退出，未触发 kill）
- **stderr（完整，57 bytes）：`self-test: 235 checks passed (pure; no GUI/ready/input)`**
- **stdout：0 bytes**（`stdout_bytes_seen=0`）

## 计数核对（实际 vs 预测）

- 实测 stderr 计数 **235** = 预测 147（旧基线）+ 16（StageA）+ 72（StageB 新增，52 名称字面量展开 exact-component×4 + nonfinite×12 + DPI×7 后）。一致。
- exit=0（fail-fast 未触发），故 235 项全部真实执行；无失败、无被遮蔽用例、无首失败项。本报告不修改、不增删任何断言。

## 残留核对（read-only，未 kill，未自匹配误判）

`residual-check.output`：`Win32_Process` 中 **无 `desktop-feedback-windows.exe` 进程**；按 TEMP 目录名匹配 commandline 仅命中检查调用自身的瞬态 cmd.exe(26744)/powershell.exe(21720)——即检查自身进程树自匹配，随 SSH 会话退出，**非 runner/self-test 残留**。runner summary `cleanup_state=root_exited` 亦确认根进程自然退出。未执行任何 kill；Notepad24332 及无关进程未触碰。远端 TEMP 与证据目录全部保留，无需清理。

## Attempt 记录

仅 1 次远端 invocation，一次成功；无失败 attempt、无目录重建、无证据覆盖。

## 边界声明

- 本验证仅覆盖纯几何/协议/layout self-test。实际 DPI 迁移、ShowWithoutActivation、真实 Stop 点击取消、热插拔、Host 故障收尾、capture exclusion、多屏真实 GUI 行为**均未验证**（pending，需另行单独授权）。
- runner 无后代进程 containment（summary 明示 `descendant_containment=none`）；本次不涉及后代进程。
- 未改源/测试/构建脚本/全局配置；零 commit/push/worktree/subagent。
