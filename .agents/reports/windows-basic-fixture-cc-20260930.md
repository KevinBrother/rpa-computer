# Windows B2 fixture + analyzer — CC 无窗口验证（windows-basic-fixture-cc-20260930）

日期：2026-09-30（Asia/Shanghai）。执行者：CC（tester/reviewer，非实现作者）。
源码冻结：`.agents/reports/windows-basic-fixture-sol-20260930.md`（SOURCE_FROZEN），20 文件清单
`acceptance-fixture/windows/.compile-only-b2-20260930-202039/source-sha256.txt`。
本机 Mac 未运行任何 Rust/Cargo/Mac/Linux 测试；未启动 GUI/Host/renderer/原生输入/截图；无 commit/push/worktree/subagent；无源码/helper/test/build 脚本修改。

## 1. 偏差与编排失误（必须记录）

1. **首轮构建目录被删除（违反 preserve-first-failure）**：我执行的
   `cmd /c rmdir /s /q ...out-computer-basic-cc-build` 删除了第一次构建尝试目录（含其 build.log/build.err/build.throw.txt/exe）。
   该次原始构建证据现已**不完整**，仅存转录摘录：throw 记录显示内层
   `powershell -File build-windows.ps1 -OutDir ...` 在 NativeCommandError 包装下 exit marker 写为 **-1**，但同目录实际产出了 exe，SHA256 `b0d330f7e6dbb71ac5eb0e6c140e93d44d7d5ec4e4b3b0918ebd27236b472044`（csc 非确定性，两次构建哈希不同属正常）。未再重复删除；最终构建是另一次全新 default OutDir 构建，未被删证据污染。
2. **退出码捕获曾不可靠（已纠正，不冒认）**：
   - winexe + PowerShell `1>` 重定向下 `$LASTEXITCODE` 为 null、stdout 空（第一组 console 目录只剩 3 个 0 字节文件）；当时读到的 `CONSOLE_EXIT=0` / `ST_EXIT=0` 均为 **cmd 单行 `%ERRORLEVEL%` 解析期展开产生的 wrapper 状态，不是原生退出证据**。这些读取一律作废。
   - 最终改用快照内单一子调用脚本 `cc-inv-20260930-selftest.ps1`（`& $exe ... | Out-File ...; $c=$LASTEXITCODE; if($null -eq $c){throw}; $c | Set-Content *-exit.txt`），每步立即落盘数值 marker，经 `ssh powershell -File` 执行。以下所有 exit 均以落盘 marker 为准。

## 2. Windows 快照与哈希

- 远端目录：`C:\Users\Administrator\AppData\Local\Temp\computer-basic-cc-f502dc6e139847f5b145cf2a920d399e`（新建唯一 TEMP，repo-relative 布局：acceptance-fixture/、scripts/、tests/）。
- 冻结 20 文件先在本地 staging 校验 20/20 OK，scp 后再在 acer-win 逐文件 `Get-FileHash` 比对，**20/20 与冻结清单完全一致**（本地留档 `stage-hash-verify.log`）。
- 另带入只读依赖：`acceptance-fixture/tools/check-cases-parity.py`、`acceptance-fixture/macos/Cases.swift`；**验证中途按需补入 `acceptance-fixture/macos/GestureCases.swift`**（见 §4 红灯）。

## 3. 构建（Framework64 csc，新 OutDir）

命令（acer-win，cmd 直调，重定向到快照内文件）：

```
cd /d ...\computer-basic-cc-f502dc6e139847f5b145cf2a920d399e\acceptance-fixture
powershell -NoProfile -ExecutionPolicy Bypass -File build-windows.ps1 1>..\build.log 2>..\build.err
```

- **exit 0**（build.log 尾行 `built: ...acceptance-fixture\build\windows\ComputerUseAcceptance.exe`；stderr 空，无 warning）。
- exe：`...\acceptance-fixture\build\windows\ComputerUseAcceptance.exe`
  **SHA256 `fc6c2d2c8451bce6ae8edf0aa154827e9e02e53aa8a944c2541edc85a71563bc`**（运行结束后再次 Get-FileHash 复核一致）。未复用 sol 的 compile-only 产物。

## 4. 无窗口分支结果（全部快照内 `console2\` 落盘 marker）

| Gate | 命令 | 数值 exit | 结果 |
|---|---|---|---|
| --self-test | `& exe --self-test` | **0**（self-test2-exit.txt=0） | `SELF-TEST PASSED (143 checks)`；FAIL 行 0；PASS 行 143（+1 汇总行）。**实际总数 143**，未猜测；旧 89 自测在 stdout 中无法单独拆分，仅确认总量与 0 FAIL |
| --export-cases | `& exe --export-cases legacy63-2.json` | **0** | `"id"` 计数 **63**；stdout 含 coordinator-only WARNING |
| --export-platform-cases | `& exe --export-platform-cases windows-basic20-2.json` | **0** | 7756 bytes；`"id"` 计数 **20**；`platform=windows`；pointer total 10 + keyboard total 10；pointer-09 `status=blocked/needs_capability`、`valid_input_candidate=false`；pointer-10/keyboard-10 `valid_input_candidate=false`；`ten_valid_trials_per_action_gate=NOT_COMPLETED`；`macos=pending`。未伪造 Mac parity |
| legacy parity（旧63） | `python tools\check-cases-parity.py legacy63-2.json` | 首跑 **1**；补只读输入后 **0** | 首红原因：`FileNotFoundError ... macos\GestureCases.swift` —— **打包缺口，非目录不一致**：冻结 20 文件清单未含 parity 工具必需的只读依赖 GestureCases.swift。首红 stdout/stderr/exit 原样保留（parity63-2.log / parity63-2-exit.txt=1）。仅补拷该只读输入（无任何源码/测试修改）后重跑（parity63-3.log）：`PARITY OK: swift == cs at UTF-16 level; spec satisfied; 63 cases across 8 suites` + `runtime manifest match: OK` |
| B2 analyzer 测试 | `python tests\basic_input_gui_analyzer.py`（Windows 本机 Python 3.12，依赖可用） | **0** | `Ran 30 tests ... OK`（实际 30，未猜测；含 padding 不升级、拒绝需工具证据、模型校验、CLI exit2 等） |

原始产物本地留档：`.agents/runs/windows-basic-fixture-cc-20260930/artifacts/`（logs、numeric exit files、两份 manifest JSON）。

## 5. 冻结前独立源码审阅（结论）

- 入口：`--self-test` / `--export-cases` / `--export-platform-cases` 均在 `Application.EnableVisualStyles()` 与任何窗体/消息过滤器创建前 return（MainForm.cs:488-519）；legacy 默认与旧 `GestureExporter.ManifestJson()` 路径未改动。
- 参数解析（BasicArguments.cs）：未知参数/缺值/重复/非 uint seed/控制台模式互斥均清晰抛错→exit 1；符合报告声明，selftest 内 8+ 项负例源码属实。
- app-only 证据模型（BasicCases.cs/BasicSelfTest.cs）：raw WM message/wParam/lParam/native time 与 semantic kind/button/area 分离；pointer-09 硬编码 blocked 不可升级；pointer-10/keyboard-10 拒绝例 app 单独只能 `needs_tool_evidence`，与 analyzer 负例一致；inactive-click 要求第二窗 prepare + 实际目标 HWND；hold 用原生 down/up 时间差。**合成测试全绿不能证明真实 GUI 行为**（报告 §7.5 已列真实 GUI pending 项，我未做任何 GUI 验证）。
- analyzer：独立小解析器，仅单层 JSON metadata、不递归/不输出图片（有 image 不透传测试）；first/final、retry、blocked、rejection、valid_input 分列；分母固定 10；模型校验 glm-5.3-flash。

## 6. 未宣称 / pending

- **这不是 GUI 验收**：没有任何 10 次有效 trial、任何真实输入、任何截图；`gui_verified=false`、`ten_valid_trials_per_action_gate=false`。输入套件任务未获授权，未执行。
- pointer-09 padding 契约仍为功能阻断（root padding map 未接）；拒绝例依赖真实工具显式 invalid_action/not_started。
- 首轮构建原始目录已删（§1.1），仅转录摘录存活；最终构建证据完整。
- 远端快照与产物保留在上述 TEMP 目录，未清理（除已删除的首个构建目录外无其它删除）。
