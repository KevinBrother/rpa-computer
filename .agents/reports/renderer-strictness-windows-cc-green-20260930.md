# Windows renderer strictness — CC green retest（阶段 B 验证）

状态：**GREEN_CONFIRMED**。 acer-win 原生 Framework64 构建的 exe 上，`--self-test` 退出码 0，stderr 报告 **147 checks passed**，与预期 old 67 + new 80 = 147 完全一致。仅验证 self-test；overlay/stop/exclusion 真实 GUI 行为仍未验证（pending，本报告不覆盖）。

## 对前一份 red 报告的明确更正

`renderer-strictness-windows-cc-red-20260930.md` 中"仅 1 条断言实际执行、其余 79 个均未执行"的表述不准确。实际执行顺序（SelfTests.cs:90-170）：

1. `Run()` 从 line 92 起先执行全部旧检查（`generation_exact`、17 项 invalid 表、Geometry/Style/BoundedLines/Mailbox 等），这些断言**在 red 运行中全部真实执行并通过**（67 项，含 line 117-119 的 invalid-utf8 手动 `checks++`）。
2. 随后 line 169 进入 `StrictProtocolCases(basic)`；其中 session_id 负例循环里 `empty`（index 0）先于 `129_bytes`（index 1）——即 red 运行时 `strict_session_id_reject_empty` **已通过**，首个失败的是 `129_bytes`。
3. fail-fast 只阻止了**其后**的断言（129_bytes 之后的负例、accept 正例、数字/pointer 用例等），并非"只有 1 条断言运行"。

本更正不覆盖旧报告原文，旧 red 证据原样保留于 `.agents/runs/renderer-strictness-windows-cc-red-20260930/`。

## 合规与执行环境

- 目标机：`acer-win`（SSH，BatchMode）。仅执行无 GUI 的 `--self-test`；未启动 renderer UI、未触碰正在运行的 GUI（属另一 CC 所有）、未启动 Host、未创建计划任务、无截图。
- 零源码/测试/helper/settings 改动；未 commit/push/worktree；无 subagent。
- 全新 acer-win TEMP snapshot，不复用 red 运行的任何产物。
- 仅 Windows 测试；Mac/Linux 不在本报告范围。

## 产物与路径

- acer-win TEMP：`C:\Users\Administrator\AppData\Local\Temp\renderer-strictness-cc-green-20260930-191431-27529`
- exe：`<TEMP>\build\desktop-feedback-windows.exe`
- Mac 侧 raw 日志：`.agents/runs/renderer-strictness-windows-cc-green-20260930/`
  - `source-hashes.txt`（Mac 源 SHA-256）、`tempdir.txt`、`upload-listing.txt`（远端清单 + 远端 SelfTests/Protocol/JsonValue 哈希）、`self-test-windows.output`（build 输出 + exitcode + stderr/stdout + exe hash 原文）、`self-test-windows.ssh.stderr`、`self-test-windows.runner.ps1`

## 源 SHA-256（Mac 源，详见 source-hashes.txt）

| 文件 | SHA-256 | 与 red 运行对比 |
|---|---|---|
| IPC.cs | f24d5b10…70ba3 | 相同 |
| JsonValue.cs | 8d23af10…ab4a | 相同（JSON 解析器未被改，前导零/指数/UInt64 校验保持既有严格结构） |
| Model.cs | 74e158d8…c9b2c | 相同 |
| Native.cs | 74e33605…b911 | 相同 |
| Program.cs | 3afb0375…ba675 | 相同（`--self-test` 在 WinForms/DPI/Native.Initialize/Application.Run 之前分支，纯 self-test 无 HWND） |
| Protocol.cs | **265fe9db…89b31** | **唯一变化**（red 时 4d9c9d82…e9edc） |
| SelfTests.cs | e7430d89…77cfb | 相同（旧 67 项 + 新 80 项全部保留，无删除/弱化） |
| Windows.cs | 9cacb306…43ab | 相同 |
| app.manifest / exe.config / build-windows.ps1 | 见 source-hashes.txt | 相同 |

远端复核：上传后 acer-win 侧 `SelfTests.cs`=`E7430D89…77CFB`、`Protocol.cs`=`265FE9DB…89B31`、`JsonValue.cs`=`8D23AF10…AB4A`，与 Mac 源一致（PowerShell 大写格式差异除外）。

## 独立实现审查（spec → canonical Rust 对照）

- canonical：`crates/desktop-feedback/src/protocol.rs` `validate_ascii_token`（非空、len≤max、每字节 `is_ascii_alphanumeric` 或标点集）。session/surface id：max=128，标点 `b"-_.:"`（`validate_token`，protocol.rs:212-214）；surface.version：max=128，标点 `b"_.:,,-"`… 即 `b"-_.:,"`（`validate_surface_version`，protocol.rs:216-220，逗号仅限 version，注释明确不放宽 session ID）。
- C# `Protocol.Token`（Protocol.cs:64-74）：`s.Length==0 || s.Length>128 → invalid_identifier`；逐字符 ASCII alnum 或 `_ . : -`，仅 `allowComma=true` 时额外允许 `,`。session.id/surface.id 以 `false` 调用，surface.version 以 `true` 调用（Protocol.cs:41,47）。**与 canonical 逐点一致**；接受字符全为 ASCII，字符数即 UTF-8 字节数，与 Rust `value.len()`（字节）语义等价。
- pointer/surface：C# 在解析 pointer 字段前要求本帧 surface 非 null（Protocol.cs:49-50）↔ Rust `Snapshot::validate` `self.surface.is_none() → InvalidField`（protocol.rs:137-140）。双方均保留 surface 尺寸/finite 与 pointer finite/kind 校验；canonical Rust 同样**没有** pointer 落入 surface 矩形的约束，C# 未擅自加严或放宽。
- `JsonValue.cs` 未改（哈希同 red）：前导零、`+`、缺分数/指数、`UInt()` 逐字符十进制及 u64 溢出校验均为既有严格实现，新增数字 case 为回归防线而非改动对象。
- 测试对比：SelfTests.cs 哈希与 red 运行完全相同，未发现任何断言被删除或弱化；`RejectNamed` 仅接受 `WireFailure`，不吞异常。

## 构建与测试结果（原生证据）

- 构建：`powershell -NoProfile -ExecutionPolicy Bypass -File <TEMP>\build-windows.ps1 -Output <TEMP>\build\desktop-feedback-windows.exe`，Framework64 v4.0.30319 csc，`/langversion:5 /target:winexe /platform:x64 /optimize+ /warnaserror+`
- **build_exit=0**，输出 `Built …desktop-feedback-windows.exe (not launched; not acceptance)`
- 测试：`Start-Process -FilePath <exe> -ArgumentList '--self-test' -Wait -PassThru -RedirectStandardOutput/-RedirectStandardError`
- **test exitcode = 0**
- **stderr（完整原文）：`self-test: 147 checks passed (pure; no GUI/ready/input)`**
- **stdout：空**
- exe SHA-256：`C4B2A2F53EEE91C561933E43ED8532570E349D2A140B5B90589FB49DD68794BD`

## 数量核对

- 新增 80：21（id/version × 7 负例）+ 2（id 逗号）+ 9（id/version accept）+ 1（pointer_requires_surface）+ 2（null surface/pointer）+ 1（surface_without_pointer）+ 1（geometry version 逗号）+ 18（sequence/coordinate JSON 负例）+ 12（unsigned 负例）+ 6（unsigned exact）+ 6（coordinate valid）+ 1（pointer fraction/exponent）+ 1（pointer nonfinite）= 80。
- 旧 67 + 新 80 = **147** = 实测 stderr 计数。计数一致，非假设；fail-fast 未触发（退出 0），故全部断言本轮实际执行。

## 结论

RED gate 的首个失败（`strict_session_id_reject_129_bytes`）经 Protocol.cs `Token` 最小实现后，在全新原生构建上全量 147 项 self-test 通过，exit=0。**GREEN 成立**。真实 overlay 渲染、stop 流程与 capture exclusion 的 GUI 行为仍为 pending，需另行 GUI 验证；本报告不声称其通过。
