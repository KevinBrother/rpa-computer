# Windows bounded test runner — CC verification (16-case selftest) — 2026-09-30

- 执行身份：CC（真实 Windows 机器 node1，经既有 SSH `acer-win`，交互用户 console 会话上下文继承；非 Session 0 GUI）。
- 结论：**GREEN。SelfTest 原生 exit 0，PASS 16/16 cases。**四份冻结源码字节未改动；无 GUI/原生输入/Host/renderer/网络/防火墙/系统改动；Notepad24332 及无关进程未触碰。

## 冻结源码 SHA-256（本地与远端均核对一致）

| 文件 | SHA-256 |
| --- | --- |
| `scripts/windows-test-runner.ps1` | `ad2a607ca0af59983b306ba5f6c32611005669a6b7d385834ccceb027a9572c4` |
| `scripts/windows-test-runner/Runner.cs` | `aced18e616afe881845b2ac233e32a190b7186b69492aaa7be416ac41e972214` |
| `tests/windows-test-runner/Child.cs` | `9cb8c32bc2bb7965aa91d20588e3c6c5c98fa9df9cc400dfcf6fccb4d45a5b56` |
| `tests/windows-test-runner/SelfTest.ps1` | `1e4212d7f6f0e4059b476f8d84f58cc49b2f61249653f2ca4794fe61b600cceb` |

Windows 侧 staging 副本经 `Get-FileHash` 核对，四项与冻结表逐字节一致（含 UTF-8/no-BOM 中文 argv 字面量 `中文`；argv case roundtrip 通过证明编码未被静默转换，未做任何 BOM/换行转换）。远端无既有 checkout，按 brief 仅四文件保留 repo 相对 `scripts/`、`tests/` 布局复制到新唯一 TEMP。

## 运行路径

- Staging（保留，未删）：`C:\Users\Administrator\AppData\Local\Temp\windows-test-runner-cc-b5f1b3cbae964adb8976ed8b5ce681bc`
- SelfTest 自建证据 TEMP（保留）：`C:\Users\Administrator\AppData\Local\Temp\windows-runner-12fbb4fdb9bc405e91f37c68349f3f23`
- 本地证据归档：`.agents/runs/windows-test-runner-cc-20260930/`（两个远端目录全量 tar，含全部 config/summary/identity/harness stdout+stderr/case 目录/`selftest-result.json`，83+ 文件）

## 命令与原生退出码

实际自测运行（attempt invocation-4）：远端 on-disk `invoker3.ps1`（`System.Diagnostics.Process` 启动 `C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -ExecutionPolicy Bypass -File ...\SelfTest.ps1`，启动后缓存 `$p.Handle` 再 `ReadToEndAsync` + `WaitForExit(170s)`，持久化真实整数 ExitCode）。

- **native exit = 0**（`invocation-4/native-exit.txt`，真实整数，非 0 coercion，非 parse-time `%ERRORLEVEL%`）。
- stdout 末行：`PASS 16 cases; retained evidence: ...windows-runner-12fbb4fdb9bc405e91f37c68349f3f23`；stderr 空。
- `selftest-result.json`：`{"outcome":"passed","cases":16}`。
- Case 计数核对：结果声明 16 cases；证据 TEMP 内 9 个 case 目录（argv/cwd/existing/launch/missing/nonzero/stress/timeout/zero）+ 7 个 validation-rejection case（nullarg/unknown/bound/duplicate/nonstring/nullarray/malformed）按设计在占用 evidenceDirectory 之前拒绝，证据在各 case `.harness-stdout.log` 内。9+7=16，与声明一致。

## 失败/中断 attempt（全部为 invocation 侧，非产品 red；全部保留未删）

| Attempt | 结果 | 原因 |
| --- | --- | --- |
| scp/sftp 传输 | 失败 | Windows OpenSSH sftp 路径处理问题；改为 base64-over-ssh 管道传输，事后 hash 证明字节一致 |
| 目录首建 | 返工 | ssh→cmd 层吞反斜杠，误建 `scriptswindows-test-runner`；同一 TEMP 内用 `Rename/Move` 修成正确布局（未删原始 TEMP） |
| invocation-1 | 未运行 | 外层调用用 `Join-Path $env:WINDIR Microsoft/WindowsPowerShell/...` 解析失败（CommandNotFound），SelfTest 未启动 |
| invocation-2 | 未运行 | 我的 invoker 用了 PS 5.1 不支持的 `WriteAllText(path, if(...))` 表达式，ParserError，SelfTest 未启动 |
| invocation-3 | 未运行 | invoker2 中 powershell 路径误为 `Microsoft\WindowsPowerShell`（该机实际为 `System32\WindowsPowerShell`），`Process.Start` "file not found"；invoker 写入 `NOT-EXITED`，logs 空。**Native ExitCode null 在此是 harness-调用失败的证据，非 runner red** |
| invocation-4 | **成功** | 修正路径后全绿 |

以上均为只改调用 wrapper（`invoker*.ps1`），四份冻结源码零改动、零重试改源。

## 残留/清理核对（read-only，未执行任何 kill）

用 on-disk `residual-check.ps1`（GUID 不出现在命令行，避免自匹配）按两个唯一目录路径+commandline 匹配 `Win32_Process`：仅命中检查脚本自身那次调用的瞬态 cmd.exe/powershell.exe（随 SSH 会话退出）。**无 runner harness、child fixture、timeout 后代残留；无需任何 kill；未使用名称/全局 kill。**初步一次命令行内联检查误报 2 个 PID（cmd/powershell），经核实为该检查自身进程树自匹配（每次 SSH 新 PID），已在报告中更正，未据此杀进程。

## 明确限制（不隐瞒）

- **无 descendant containment**：runner 无 Job Object、无后代枚举；timeout 只 kill exact owned `Process`（cached handle，Runner.cs:129,159）。本次 timeout case 通过不证明所有子进程被回收；本 runner 不得用于活 GUI 或声明后代全部清理。
- 30s/watchdog 到期、drain incomplete、UTF-8 noBOM 字面量在 PowerShell 5.1 的编码 caveat（本次未触发，已保留原字节）均沿用 SOL 报告声明。

## 准入状态

本验证 GREEN 仅覆盖纯 console 测试 runner；其他测试任务采纳需 coordinator 批准。远端两个 TEMP 目录全部保留，无未解决的 cleanup。
