# Desktop feedback renderer — Codex / sol 源码交接

日期：2026-09-30；目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。

## 分工与边界

- 按用户最新分工，代码实现由当前 Codex 完成；未调用 Claude CLI 或其它 agent，没有将旧 CLI-only 约束沿用到实现。
- 测试/验收由协调者安排 Claude Code + GLM。**本次未执行任何 self-test、测试或验收，历史纯测试自检也没有。**
- 只执行实现必需的编译检查；编译不代表测试通过、GUI成功或截图排除成功。
- 未启动窗口、未GUI输入、未截图、未SSH；未worktree、commit、push或恢复脏树。
- 写入范围仅新 `desktop-feedback/**` 和本报告；未修改 Rust、root Cargo、Host capture 或 acceptance-fixture。

## 源码实现摘要

- Swift/AppKit 与 C#5/WinForms 独立 renderer 和构建入口，不增加内核依赖。Linux 默认反馈 renderer 首版未支持，不修改 Linux 内核能力。
- 主界面分为点击穿透状态条 + 独立 Stop 命中窗口 + 轻量 pointer ring；品牌 accent/label 可配，标签长度/控制字符/转义有限制。
- 初始 Waiting for Host，无 session 禁 Stop，不宣称正在执行。Host 状态/清理事实唯一来源；failed/unknown/pending 优先告警。
- Stop / 窗口关闭只请求当前 session+generation；本地转“请求中”，不假称已释放。closed 禁 Stop，EOF 退出自身，不自行清理输入。
- Strict NDJSON 字段/duplicate-key/UTF8/枚举/无符号64位校验；不以 sequence 授权，旧帧不能回滚。
- `surface.version` 未套 session token 白名单；新增 Swift/C# 测试用例断言真实 `d1:o0,0:i1512x982:c3024x1964:r0` 可解析且保留。
- 按 Host codec 将16KiB定义为包括 LF，内容16383bytes；后台4096bytes读取、超限立即报错。latest mailbox1槽、stdout FIFO32帧，UI不阻塞等待Host管道。
- UI主线程30ms消费；ready仅真实GUI初始化后发一次；heartbeat每秒来自UI定时器；EOF/坏帧关闭自己的窗口，有300ms输出退出上界。
- 原生输入坐标，不使用截图像素缩放；负原点、多屏拓扑核对及 unavailable 状态/诊断；无新帧也每秒检查显示布局。
- `--self-test` 放在任何 GUI/DPI 初始化之前，仅 Foundation / 纯 C# 模型，stdout无ready，不注入事件。

## 已执行命令与实际结果（仅开发编译）

工作目录均为上述 rpa-computer。

1. `swift --version`：exit 0，Apple Swift 6.3.3，`arm64-apple-macosx26.0`。
2. 初次 `./desktop-feedback/build-macos.sh`：exit 1，`Protocol.swift` guard 第二个 throwing expression 缺少 `try`。已按编译器诊断修正；同时修正测试代码 throwing autoclosure 的编译表达式。
3. 修正后 `./desktop-feedback/build-macos.sh`：exit 0。后续与最新源码相关的相同命令均 exit 0；最终脚本包含 `-target arm64-apple-macosx13.0 -swift-version 5 -O -warnings-as-errors`，真实链接 AppKit/CoreGraphics，输出：
   ```text
   Built /Volumes/doc/workspace/datagrand/rpa/rpa-computer/desktop-feedback/build/desktop-feedback-macos (not launched; not acceptance)
   ```
4. `./desktop-feedback/check-csharp-compile.sh`：exit 0，`mcs -langversion:5 -sdk:4 -warnaserror+ -target:winexe -platform:x64` 静态编译完整C#源码和测试代码，输出：
   ```text
   C#5 .NET4 syntax compilation only; NOT Windows Framework64 build; NOT executed.
   ```
   此命令未调用 Mono运行时；**不能替代真实 Windows Framework64 v4 csc 构建**。

测试正文先写，用户纠正分工后不执行RED/GREEN；不能称为“已通过TDD”或“所有测试通过”。

## 平台限制与 Host 缺口

### Mac — 已知缺口，非已验证

- 当前官方 Apple SharingType.none 文档将其描述为 macOS 不再使用的 legacy 常量。不能以 setter/透明/点击穿透宣称排除。
- 本 renderer 固定真实 ready `capture_exclusion=unsupported`。
- 只读调查现有 Host：`src/backend/macos.rs` -> screenshots0.8.10 `darwin.rs` -> core-graphics0.23.2 `CGDisplay::screenshot` -> `CGWindowListCreateImage`；全屏 on-screen 路径没有排除 renderer 的显式 filter。
- **Mac feedback-13 不满足，未进行模型截图排除验证。** 后续必须独立对接 Host capture（例如官方 ScreenCaptureKit 按 renderer PID/app/窗口过滤）。本次没有修改Host。
- 不扩展本批冻结消息 schema 来谎称拥有窗口过滤能力；Host后续的过滤能力应与renderer本地unsupported区分。

### Windows — 有条件 requested，实际排除未验收

- PMv2 设置/核实失败明确退出；GUI需被控端交互session，Session0拒绝。
- embedded supportedOS manifest+GetVersionEx 检查Win10 2004/build19041，DWM成功，再对bar/Stop/ring三个自身顶层 HWND设置0x11且回读0x11。全部成功才requested；失败unsupported+error，不回退WDA_MONITOR。
- Host当前 screenshots0.8.10 为GDI StretchBlt/GetDIBits。API成功不是该路径与最终model图像排除证据；待CC+GLM实机验证，失败再由Host owner调整后端/策略。
- 没有真实 Windows csc 构建、API执行或交互证据；C#5静态编译只提供实现阶段语法证据。

官方 primary 文档及精确本地 backend 调查记录：`desktop-feedback/docs/capture-exclusion.md`。

## 待协调者 CC+GLM 执行命令

Mac（构建不启GUI，self-test无GUI）：

```sh
cd /Volumes/doc/workspace/datagrand/rpa/rpa-computer
./desktop-feedback/build-macos.sh
desktop-feedback/build/desktop-feedback-macos --self-test
```

Windows（在协调者指定repo目录，原生编译）：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\desktop-feedback\build-windows.ps1
# WinExe要明确等待退出并获取退出码；以下只有self-test，不创建窗口
$p = Start-Process -FilePath .\desktop-feedback\build\desktop-feedback-windows.exe -ArgumentList '--self-test' -Wait -PassThru -RedirectStandardError .\desktop-feedback\build\self-test.stderr.txt -RedirectStandardOutput .\desktop-feedback\build\self-test.stdout.txt
$p.ExitCode
Get-Content .\desktop-feedback\build\self-test.stderr.txt
Get-Content .\desktop-feedback\build\self-test.stdout.txt
```

纯测试代码覆盖：协议/字段/重复键（含escaped重复）/UTF8/枚举/整数overflow、surface.version逗号、初始/idle/closed及清理失败状态、新代次Stop、负原点坐标、不缩放截图单位、styles负例、输出转义、16KiB上界/立即超限/CRLF/partial EOF、mailbox序号水位。

实机门禁另行调度（当前均未验收）：初始化ready真实性、初始禁Stop、等待Agent状态、不抢焦点、跨进程穿透、首次Stop点击/无输入残留/旧代次拒绝、窗口关闭Stop、EOF、heartbeat/慢管道/损坏帧、混合DPI/拔屏、Windows实际截图排除。Mac截图排除需Host改造后再验收。

## 文件列表（全部新文件）

```text
desktop-feedback/.gitignore
desktop-feedback/README.md
desktop-feedback/build-macos.sh
desktop-feedback/build-windows.ps1
desktop-feedback/check-csharp-compile.sh
desktop-feedback/docs/capture-exclusion.md
desktop-feedback/tests/README.md
desktop-feedback/macos/JSONValue.swift
desktop-feedback/macos/Protocol.swift
desktop-feedback/macos/Model.swift
desktop-feedback/macos/IPC.swift
desktop-feedback/macos/SelfTests.swift
desktop-feedback/macos/Windows.swift
desktop-feedback/macos/main.swift
desktop-feedback/windows/JsonValue.cs
desktop-feedback/windows/Protocol.cs
desktop-feedback/windows/Model.cs
desktop-feedback/windows/IPC.cs
desktop-feedback/windows/Native.cs
desktop-feedback/windows/Windows.cs
desktop-feedback/windows/SelfTests.cs
desktop-feedback/windows/Program.cs
desktop-feedback/windows/app.manifest
desktop-feedback/windows/desktop-feedback-windows.exe.config
.agents/reports/desktop-feedback-renderer-sol-20260930.md
```

Ignored本地编译产物：`desktop-feedback/build/desktop-feedback-macos`、`desktop-feedback/build/desktop-feedback-windows-syntax-only.exe`、`desktop-feedback/build/module-cache/**`。
所有新增源码/脚本/说明文件均小于1000行；没有在acceptance-fixture中添加renderer。
