# Desktop feedback renderer — Claude Code 双平台构建 / 纯 self-test 验证

日期：2026-09-30。任务：只审查、构建、执行明确无 GUI 的 `--self-test`；不写/修源码、测试、项目脚本；不 worktree / commit / push；不启动 agent；不改设置/TCC。源码由 Codex+gpt-6.1-sol 冻结（见 `desktop-feedback-renderer-sol-20260930.md`）。

## 结论（非 GUI 验收）

| 平台 | 编译 | self-test | 退出码 | stdout | stderr 摘要 |
|---|---|---|---|---|---|
| macOS (arm64) | exit 0 | exit 0 | 0 | 空（无 ready） | `self-test: 69 checks passed (pure; no GUI/ready/input)` |
| Windows x64 (.NET Framework64 v4 csc) | exit 0 | exit 0 | 0 | 空（无 ready） | `self-test: 67 checks passed (pure; no GUI/ready/input)` |

构建产物 SHA-256：

- Mac：`0da2d488aa8cea4398fa3d6ecb1b767c72e35df61e6b604a5a602a0ff85adc12`（`desktop-feedback-macos-cc`）
- Windows：`82FFB0840265A85BFE1E5C0133E0699DBA890857B56B40E479D1331AE609A747`（`desktop-feedback-windows.exe`）

**无真实 GUI，不能声称 click-through / no-focus / Stop 可达 / 截图排除已验收。** Mac `capture_exclusion=unsupported` 缺口保持（需 Host capture 改造）；Windows `requested` 仅为 API 设置成功，未做真实 GDI/model 截图排除验证。

## 1. --self-test 前置确认（通过）

- `desktop-feedback/macos/main.swift:6-11`：`--self-test` 分支在任何 `NSApplication` / screen / window 访问之前（`NSApplication.shared` 在第 72 行）；self-test 仅 Foundation 纯模型。
- `desktop-feedback/windows/Program.cs:63-69`：`--self-test` 分支在 `Native.Initialize()` / `Application.EnableVisualStyles()` / WinForms 任何 DPI 初始化之前（`[STAThread] Main` 首行）。
- 两平台 self-test 实测 stdout 均为空（无 ready、无协议输出），未创建窗口、未注入输入、未截图。

## 2. 独立规格与质量审查（只读，未自修）

对照 `.agents/tasks/desktop-feedback-v1-wire-20260930.md` 与 `crates/desktop-feedback/src/protocol.rs`（Host 侧），逐项核对：

- **NDJSON 字段**：Swift/C# `Snapshot.parse` 均要求 8 个字段精确匹配（Swift `Set(keys)` 相等；C# 数量+包含），`session/surface/pointer` 可 null（显式 `isNull` 判断），未知字段拒绝。与 Rust `deny_unknown_fields` 一致。
- **duplicate key**：两平台 JSON 解析器均显式拒绝重复键（Swift `object[key] == nil`；C# `map.ContainsKey`），且拒绝 escaped 重复键（C# 测试含 `"sequen\u0063e":2` 负例，键以原始字符存入 Ordinal 字典）。
- **版本/整数**：`version==1` 严格；`1.0`、布尔、`-1`、`18446744073709551616`（u64 溢出）均被数字词法（仅 `0-9`）+ 精确 UInt64 解析拒绝，不经 Double。C# `UInt()` 逐字符 digits 校验 + `NumberStyles.None`；Swift 同。与 Rust `u64` serde 精确解析一致。
- **16KiB 边界**：两平台帧上限 16383 内容字节 + LF（CRLF 的 CR 计入上限），超限立即报 `frame_too_large` 不缓冲不扩展；后台读 chunk 固定 4096；partial EOF → `truncated_frame`。实测于 self-test（`line_at_bound` / `oversize_bounded` / `crlf` / `eof_partial`）。与 Host codec 一致。
- **真实 Geometry.version 逗号**：`d1:o0,0:i1512x982:c3024x1964:r0` 保留断言（`surface_version_commas_preserved`，两平台均有）。Rust 侧 `validate_surface_version` 允许逗号，一致。
- **初始无 session**：`honest_initial`——`Waiting for Host`、禁 Stop、无 "executing" 宣称。
- **旧 sequence / 代次**：mailbox high-watermark（`mailbox_no_rollback`、`mailbox_seen_watermark`）；state 层旧/相同 sequence 不回滚（`old_frame`、`duplicate_frame`）；Stop 停当前 session+generation（`stop_current`），新 generation 重启可用（`new_generation_resets_stop`），Stop 输出不含 sequence 授权。
- **清理文案**：cleanup `failed/unknown/pending` 文案优先于 phase；仅 `closed` + `released/not_needed` 显示完成事实（`cleanup_truth` 枚举断言）。
- **装饰与 Stop 命中分离**：Stop 为独立 88×48 命中区（代码核实 `Windows.swift`/`Windows.cs`），状态条/ring 穿透；纯 self-test 不含窗口，实机验收另行。
- **异步有界 read/write**：读后台线程有界；stdout FIFO 队列上限 32，过载 `_exit(74)`/`Environment.Exit(74)` 不静默丢；EOF 退出 300ms 上界（Swift `asyncAfter` watchdog；C# watchdog 线程）。
- **ready 时序**：Mac 仅 `applicationDidFinishLaunching` 且窗口初始化成功后发一次（`main.swift:35`）；Windows 在首个消息循环 tick、全部 HWND 建立 + PMv2 检查后发（`Program.cs:28-35`）。self-test 路径均不可达 ready（实测 stdout 空）。
- **Mac unsupported / Windows requested 不虚报**：Mac 固定 `capture_exclusion=unsupported` + stderr 诊断；Windows `Native.RequestExclusion` 仅 Win10 2004+、DWM、三个顶层 HWND 设置并回读 0x11 全部成功才 `requested`，失败 `unsupported` + error，不回退 WDA_MONITOR。
- **错误码输出**：`WireOutput.error` 白名单 `[a-z0-9_]`，不回显入站 payload（两平台一致）。

### 审查备注（非阻断，未修改）

1. **Renderer 对 `session.id` / `surface.id/version` 仅要求非空**，不做 Rust Host 侧的 ASCII-token ≤128 字符集/长度校验。方向为"renderer 更宽容"，id 经 C# `JsonValue.Quote` / Swift `JSONSerialization` 正确转义后回显，无注入面；但若 Host 发超长/非法 token，renderer 会接受并在 Host 侧 stop 校验时才失败。属 Host 契约责任，可接受。
2. Rust Host 校验 `pointer` 需要 `surface` 同帧存在；renderer 未做此交叉检查（宽容方向，安全）。
3. serde_json 拒绝前导零整数（`"01"`），Swift/C# `UInt64` 解析接受并归一为 1。极端边角，方向宽容，无实际影响。

未发现阻断性偏差。

## 3. Mac 构建与 self-test（实际执行）

- 专属 runs 目录：`.agents/runs/desktop-feedback-renderer-cc-20260930/`
- 构建命令（新唯一产物路径）：`./desktop-feedback/build-macos.sh "$PWD/.agents/runs/desktop-feedback-renderer-cc-20260930/desktop-feedback-macos-cc"`
  - exit 0，输出 `Built .../desktop-feedback-macos-cc (not launched; not acceptance)`。日志：`build-macos.stdout` / `build-macos.stderr`。
- self-test：`./desktop-feedback-macos-cc --self-test`（无其它参数、不启动 GUI）
  - exit 0；stdout 空；stderr `self-test: 69 checks passed (pure; no GUI/ready/input)`
  - 记录：`self-test-macos.exit` / `self-test-macos.stdout` / `self-test-macos.stderr`

CC 实际目标机路径（Mac 本机）：
`/Volumes/doc/workspace/datagrand/rpa/rpa-computer/.agents/runs/desktop-feedback-renderer-cc-20260930/desktop-feedback-macos-cc`

## 4. Windows 构建与 self-test（实际执行）

- 目标机：`acer-win`（SSH），TEMP 确认 `C:\Users\Administrator\AppData\Local\Temp`（只读查询）。
- 全新临时目录：`C:\Users\Administrator\AppData\Local\Temp\computer-feedback-cc-20260930-20260930-182414`
- 上传内容（scp，仅源码/manifest/config/已有 build-windows.ps1，未上传任何 build 产物/缓存）：`windows\` 下 8 个 `.cs` + `app.manifest` + `desktop-feedback-windows.exe.config`，及根 `build-windows.ps1`（首次平铺导致 CS2001，已在临时目录内重组为脚本要求的 `windows\` 子目录布局后重跑；未改任何文件内容）。
- 构建：`powershell -NoProfile -ExecutionPolicy Bypass -File .\build-windows.ps1 -Output <temp>\build\desktop-feedback-windows.exe`
  - csc：`%WINDIR%\Microsoft.NET\Framework64\v4.0.30319\csc.exe`，`/langversion:5 /target:winexe /platform:x64 /warnaserror+`
  - exit 0，输出 `Built ... (not launched; not acceptance)`。日志：`build-windows.stdout` / `build-windows.stderr`（含首次失败原始记录）。
- self-test（仅 `--self-test`，WinExe 用 `Start-Process -Wait -PassThru` 重定向等待收集；**未启动正常 UI、未操作 Notepad24332、未创建计划任务、未启动 Host**）：
  - exitcode=0（`self-test-windows.exit`）
  - stdout 空；stderr `self-test: 67 checks passed (pure; no GUI/ready/input)`（`self-test-windows.output`）

CC 实际目标机路径（acer-win）：
`C:\Users\Administrator\AppData\Local\Temp\computer-feedback-cc-20260930-20260930-182414\build\desktop-feedback-windows.exe`

## 5. 源 manifest / SHA-256

### macos/

| 文件 | SHA-256 |
|---|---|
| IPC.swift | 217e522f40a44fa61f922cafd473352d57efcc55236199f757a0921c5a4fc92d |
| JSONValue.swift | 82ccc00c9790cf4a32dfd57f8a2a3e9efa63d68c53d15434c4dc70c8608d12aa |
| Model.swift | 6ec6378725ac7a8567ca2cda85619bcd65630fdef8c0fe4bb467887562bde504 |
| Protocol.swift | 85382a890b1d846873866c7e97fa0fb4e878fec72d5b22134906c89f5a0a3ba1 |
| SelfTests.swift | e0ef001b4f606a9275d704b65d8fd72fb9e416f4e8f1feafdd7f5076dd8ac37c |
| Windows.swift | 3a34ed2355e3aa68529c187580912447b766ca080ee90a3b47cfead8123ba922 |
| main.swift | 9918af6557fa4144b09284b9d239739f22744d0f8d3056bf19f3fd8aa33af30b |
| build-macos.sh | aacf35eaf8c0e2463a0d2379b2dfcf9093b55d371ba0155defa3a7089c61f2a9 |

### windows/

| 文件 | SHA-256 |
|---|---|
| IPC.cs | f24d5b1029da08c74d99aba6ad6fbe9b2f58a9a656a4af23fbaf2156a0170ba3 |
| JsonValue.cs | 8d23af10506240d9f72be513d3d846a73363a6691013ca885675ec5e9a03ab4a |
| Model.cs | 74e158d8111adb5ebcdfdf6f5cf83f901e8cc064310070d05187a66cca869b2c |
| Native.cs | 74e33605d0f82909bf21c3568ccf453c4bf79213ed792184598d60cef1c2b911 |
| Program.cs | 3afb0375d09bf523131a24e6f921cfe095fedb5148a9f2f3e9f551d3ff7ba675 |
| Protocol.cs | 4d9c9d8280670093ffdb1ce5de4144f56225381de1c1f0d03e29fbc415e49edc |
| SelfTests.cs | e160803f1e4df586f66920a2f8faf7b0ddfd9cd6874f5fe918e4b608af557eaa |
| Windows.cs | 9cacb3069ff4fe9426a9a04ba01aa8b8acc8bac855042140ca84a4c8733343ab |
| app.manifest | 4f5f2c276e99b0ada7cbd8ec34324717029642aeec86a3f1ab1eb7fe669d0089 |
| desktop-feedback-windows.exe.config | 9d49e908618fe07944a81351c5c8e4495b9320028543d8bc7fdfdf9cb44a5c7c |
| build-windows.ps1 | f2e113e490d8be515053a91a78109530b16a9b04ed2fdcc03e96198d7f86d19e |

## 6. 待实机验收项（本次均未验收）

- 初始化 ready 真实性、初始禁 Stop、等待 Agent 状态。
- 不抢焦点、跨进程点击穿透。
- 首次 Stop 点击可达性 / 无输入残留 / 旧代次拒绝；窗口关闭转 Stop。
- EOF 行为、heartbeat 每秒、慢管道背压、超大/坏帧实机表现。
- 混合 DPI / 负原点 / 拔屏（`Display unavailable` + `surface_geometry_unavailable`）。
- **Windows 真实 GDI/model 截图排除验证**（`requested` ≠ 已验证排除）。
- **Mac feedback-13 截图排除**：保持缺口，需 Host capture 改造（如 ScreenCaptureKit 过滤），renderer 侧 `unsupported` 属实。

## 7. 本任务写入范围

- 本报告：`.agents/reports/desktop-feedback-renderer-cc-verification-20260930.md`
- 日志/产物：`.agents/runs/desktop-feedback-renderer-cc-20260930/*`（含 Mac 构建产物、两平台构建/self-test 日志、Windows runner 临时脚本副本）
- Windows 目标机：仅 `C:\Users\Administrator\AppData\Local\Temp\computer-feedback-cc-20260930-20260930-182414\`（源码副本、build 产物、self-test 重定向文件）
- 未修改任何项目源码/测试/脚本；未 commit/push；未启动任何 agent；未改设置/TCC。
