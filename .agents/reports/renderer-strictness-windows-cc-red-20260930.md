# Windows renderer strictness — CC red gate（阶段 A 验证）

状态：**RED_CONFIRMED**。首个静态预期失败在 acer-win 原生 Framework64 构建的 exe 上实测复现。终态：交回 Codex 修复；未修实现、未跳过 red、未报告 green。

## 执行环境与合规

- 目标机：`acer-win`（SSH）。仅执行无 GUI 的 `--self-test`；未启动普通 renderer UI、未截图、未触碰 Notepad24332、未启动 Host、未创建计划任务。
- 源码 stage A 冻结：本次零源码改动。所有文件为复制；`JsonValue.cs`、`Protocol.cs` 未被 SOL 修改（哈希与上次 CC 验证报告一致）。
- 无 Mac/Linux 测试；Mac 侧仍为 pending，本报告不覆盖。
- 无 subagent、无 commit/push/worktree/settings 变更。

## 产物与目标路径

- 全新 acer-win TEMP 目录：`C:\Users\Administrator\AppData\Local\Temp\renderer-strictness-cc-red-20260930-190311-265262435`
- exe：`<上述目录>\build\desktop-feedback-windows.exe`
- 原始日志目录（Mac 侧）：`.agents/runs/renderer-strictness-windows-cc-red-20260930/`
  - `source-hashes.txt`（Mac 源 SHA-256）、`tempdir.txt`、`upload-listing.txt`（远端文件清单 + 远端 SelfTests/Protocol 哈希）、`build-windows.stdout/.stderr`、`self-test-windows.output`、`self-test-windows.ssh.stderr`、`self-test-windows.runner.ps1`

## 源 SHA-256（Mac 源，见 source-hashes.txt）

| 文件 | SHA-256 |
|---|---|
| IPC.cs | f24d5b1029da08c74d99aba6ad6fbe9b2f58a9a656a4af23fbaf2156a0170ba3 |
| JsonValue.cs | 8d23af10506240d9f72be513d3d846a73363a6691013ca885675ec5e9a03ab4a |
| Model.cs | 74e158d8111adb5ebcdfdf6f5cf83f901e8cc064310070d05187a66cca869b2c |
| Native.cs | 74e33605d0f82909bf21c3568ccf453c4bf79213ed792184598d60cef1c2b911 |
| Program.cs | 3afb0375d09bf523131a24e6f921cfe095fedb5148a9f2f3e9f551d3ff7ba675 |
| Protocol.cs | 4d9c9d8280670093ffdb1ce5de4144f56225381de1c1f0d03e29fbc415e49edc |
| SelfTests.cs | e7430d8980c3d4b7b756621d98b4088c4bc1a7c41466bc067d42081a4b377cfb（stage A 严格版；旧 67-check 版为 e16080…） |
| Windows.cs | 9cacb3069ff4fe9426a9a04ba01aa8b8acc8bac855042140ca84a4c8733343ab |
| app.manifest | 4f5f2c276e99b0ada7cbd8ec34324717029642aeec86a3f1ab1eb7fe669d0089 |
| desktop-feedback-windows.exe.config | 9d49e908618fe07944a81351c5c8e4495b9320028543d8bc7fdfdf9cb44a5c7c |
| build-windows.ps1 | f2e113e490d8be515053a91a78109530b16a9b04ed2fdcc03e96198d7f86d19e |

远端复核：上传后 acer-win 侧 `SelfTests.cs` = `E7430D89…77CFB`、`Protocol.cs` = `4D9C9D82…E9EDC`，与源一致（大小写差异为 PowerShell Get-FileHash 输出格式）。

## 构建（原生 Framework64 csc）

- 命令：`powershell -NoProfile -ExecutionPolicy Bypass -File .\build-windows.ps1 -Output <temp>\build\desktop-feedback-windows.exe`
- csc：`%WINDIR%\Microsoft.NET\Framework64\v4.0.30319\csc.exe`，`/langversion:5 /target:winexe /platform:x64 /optimize+ /warnaserror+`
- **build exit = 0**；输出 `Built …desktop-feedback-windows.exe (not launched; not acceptance)`（`build-windows.stdout`）。

## --self-test 短路确认

`Program.cs:63-68`：`Main` 在任何 WinForms/DPI/`Native.Initialize()`/`Application.Run` 之前分支 `args[0] == "--self-test"`，仅调用 `SelfTests.Run()` 并经 `Console.Error` 输出失败码后 `return 1`。纯 self-test 不创建任何 HWND，不进入 `RendererContext.Tick`（`Program.cs:30` 注释亦确认）。实测进程正常随 `Start-Process -Wait` 退出并写出 stderr，无 GUI 痕迹。

## 测试结果（red 证据）

- 命令：`Start-Process -FilePath <exe> -ArgumentList '--self-test' -Wait -PassThru -RedirectStandardOutput/-RedirectStandardError`（`self-test-windows.runner.ps1`）
- **test exitcode = 1**
- **stderr（完整原文）：`self_test_strict_session_id_reject_129_bytes`**
- stdout：空

这与 SOL 报告静态预测的首个预期失败完全一致：`Protocol.Nonempty`（Protocol.cs:63）仅检查非空，session id 无 129 字节（或任何）长度上限，因此合法接受该帧而非抛 `WireFailure`，`RejectNamed` 判定为"非法帧被接受"→ 抛含 case 名的失败 → `Main` 输出该码并以退出码 1 终止。

## Fail-fast 覆盖限制

self-test 为 fail-fast：**仅此 1 条断言实际执行**。SOL 报告统计的 80 个参数化断言中，其余 79 个（含全部 strict_* accept 正例、sequence/generation/坐标 JSON 负例、pointer 用例等）**均未执行、未验证**。修复 `129_bytes` 后后续断言可能暴露更多 red，这属于下一轮验证，不得据本次宣称"80 项中仅 1 项失败"或"全部已检查"。

## 协议独立简评（spec → quality）

- **Windows 前导零**：实测源码 `JsonValue.Parser`（JsonValue.cs:79-88）零分支只消费单个 `'0'`；`"01"` 消费 `'0'` 后残留 `'1'`，`Parse` 末尾 `!parser.End → invalid_json`。即 **Windows 解析器本就拒绝前导零/`-01`**。确认并采纳 SOL 报告的静态观察，纠正任何早前"Windows 会接受前导零"的推测性说法：不会。新增的 `*_json_leading_zero` 等 case 是回归防线，不是当前 red 来源。
- spec 层：id/version 语义约束（非空、逗号、长度）当前仅靠测试期望，`Protocol.Nonempty` 只有非空检查 —— 129 字节 red 即该 spec-实现差距的直接证据。
- quality 层：`RejectNamed` 不吞异常、错误信息含 case 名、失败码可定位，实现质量良好；fail-fast 语义明确。

## 结论

RED gate 成立且由原生退出码/输出证明：exit=1、stderr=`self_test_strict_session_id_reject_129_bytes`、stdout 空。未出现意外 green 或其他失败。**交回 Codex 修复**（预期修复点：session/surface id 及 version 的严格性约束，遵循阶段 B 计划）。修复前不进入任何后续阶段。
