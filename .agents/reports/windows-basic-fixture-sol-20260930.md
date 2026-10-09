# Windows B2 pointer / keyboard — SOURCE_FROZEN

日期：2026-09-30（Asia/Shanghai）
源码指纹生成时间：20:21:49 +0800；本报告写入即结束本轮实现。
状态：**SOURCE_FROZEN / 可交 CC 做 Windows csc、无窗口self-test/export、analyzer验证；不是测试/GUI通过。**

## 1. 分工与边界

- 已读完整实施任务 `.agents/tasks/windows-basic-fixture-sol-20260930.md`、用例规范pointer/keyboard各10项、完整设计第3节、CONTRACT及真实runtime action/schema/plan/key实现。
- 已读最新 `.agents/tasks/windows-basic-fixture-cc-20260930.md`。当前CC任务只授权无GUI验证；两个GUI模板为后续排期准备，**不意味着现在授权启动GUI**。
- 本轮仅源码/测试源码/文档与必要compile-only。**没有运行任何测试、self-test、export、parity、程序集、GUI输入/截图/窗口、SSH、CLI/agent、commit/push/worktree。**
- Rust/Cargo、其他crates、Mac/Linux源码/构建/测试、旧gesture/text分析器及其测试均未写入。
- 没有覆盖旧Windows部署bin、GUI snapshot、oracle或首轮失败。没有系统权限/模型/安全配置调整。
- 用户提供的旧Windows csc/89 checks/63 parity、旧analyzer 26/26是前轮CC证据，不是本轮B2验证；此处不重复或冒认。

## 2. 精确写入文件

新增：

1. `acceptance-fixture/windows/BasicArguments.cs` — 纯参数解析。
2. `acceptance-fixture/windows/BasicCases.cs` — Windows两个10-case目录、pure事件模型、独立平台export。
3. `acceptance-fixture/windows/BasicJudge.cs` — raw/semantic纯判定。
4. `acceptance-fixture/windows/BasicNative.cs` — own HWND mouse/keyboard接收及安全命令。
5. `acceptance-fixture/windows/BasicForm.cs` — UI/trial/nonce、oracle、Check-once/Next/Finish。
6. `acceptance-fixture/windows/BasicSelfTest.cs` — 新pure自测源码；不构造控件。
7. `scripts/analyze-basic-input-gui.py` — 新独立B2离线分析器。
8. `tests/basic_input_gui_analyzer.py` — 新合成负例/正例回归源码。
9. `acceptance-fixture/tasks/windows-pointer.md` — 后续CC+GLM视觉任务模板。
10. `acceptance-fixture/tasks/windows-keyboard.md` — 同上。
11. 本报告。

修改（保留原有脏工作区内容）：

- `acceptance-fixture/windows/MainForm.cs`：仅本轮入口/usage变更，接入strict parser、追加BasicSelfTest、平台export、B2路由。未重写原legacy/text业务。
- `acceptance-fixture/build-windows.ps1`：追加顶层 `windows/Basic*.cs` 编译输入，保留Framework64 csc/C#5和不覆盖现有exe的保护。
- `acceptance-fixture/start-windows.ps1`：suite白名单增加pointer/keyboard，保留交互式session、安全身份/hash流程；未运行脚本。
- `acceptance-fixture/README.md`：追加独立Windows B2章节、能力门控、证据边界与CC待执行命令。

原 `Cases.cs`、`Gesture*.cs` 本轮没有改动；只作为编译依赖/指纹输入。
共享工作区的 `git diff` 含前轮大量未提交修改，不能把整个diff算作本轮新改动。

模块规模：6个Basic模块分别41 / 89 / 115 / 151 / 140 / 114行；analyzer约530行，测试约224行。
没有继续往旧大文件堆入suite实现。

## 3. 实现与原生判定

### 入口、旧逻辑保留

- 原 `--export-cases PATH` 仍走未修改的 `GestureExporter.ManifestJson()`，目标保持旧63条，不添加pointer/keyboard。
- 新 `--export-platform-cases PATH` 只导出Windows B2 20条，顶层 `platform=windows`。
- 原 `SelfTest.Run()` + `GestureSelfTest.Run()` 原89断言/逻辑保留，只在后面 `AddRange(BasicSelfTest.Run())`。**新实际check总数交CC报告，不在未执行时猜测。**
- 三个console分支均在 `Application.EnableVisualStyles()` / Form创建 / message filter注册之前return。
- 参数缺值、重复、非法suite/seed、未知参数、混用console模式通过纯parser清晰exit1；默认legacy保持。

### pointer-01..10

- center move限定中心±16 native px；edge限定目标右侧内12px；小目标24×24并有红色decoy。
- 真实WM_MOUSEMOVE、LEFT/RIGHT/MIDDLE down/up/DBLCLK；原始wParam/lParam、native message/time与semantic kind/button/area分开。
- DOWN数量来自实际消息；DBLCLK不会算成两次down，也不被当成所要求的单击。
- 右键产生本应用ContextMenuStrip，记录Opened回调；菜单不包含系统操作。
- 第二窗 `ShowWithoutActivation`；要求真实own WM_MOUSEACTIVATE发生前不是active，再要求down/up来自该第二窗的实际target HWND。不是拿主窗点击冒充。
- pointer-03：app只确认native命中，显示NEEDS_TOOL_EVIDENCE；analyzer要求step.based_on对应实际较小width_px/height_px，并有同surface_id/geometry_version的更大前序观察。仅设置max_width不够。
- pointer-09：**固定blocked/needs_capability**，不会升级matched，不发送目标动作。完整未来门控定义见目录的platformExpected：actual observation + 明确padding rectangle + 内部坐标 + 明确not_started拒绝 + own event0。目前root无padding map契约。
- pointer-10：越界move负例，app event0仅NEEDS_TOOL_EVIDENCE；离线需真实invalid_action/not_started、清理结果和唯一调用包络。

### keyboard-01..10

- 只有本进程明确自有的两个EDIT HWND进入IMessageFilter；在WinForms Tab/menu预处理前记录实际排队WM_KEYDOWN/UP、SYSKEYDOWN/UP、CHAR/SYSCHAR。不是全局hook，也没有读取别的应用按键。
- raw MSG/native timestamp/wParam/lParam/own+focus HWND，与派生modifiers、控件text/selection分开。
- 普通键使用 `key_hold a 80ms`，因为真实runtime禁止空modifiers的key_chord。
- Enter/Tab捕获在自有多行EDIT，原生期待CRLF+TAB；Ctrl+A、Shift+Left基于原生text/selection readback。
- Alt+J、Ctrl+Shift+J是本应用有界安全命令，只在真实消息到达后更新可见指示；无系统快捷键、SendKeys/SendInput/SendMessage或人工补事件。
- Shift hold 120/900ms以GetMessageTime原生down/up差值判定，公开容差-40/+600ms；工具参数仍需精确。
- modifier-release要求Ctrl真实up在普通b down之前且b时modifier为0，文本为b。
- 非法键整体拒绝需工具显式拒绝 + own键事件0 + EDIT未变；app单独不能PASS。

### 管理边界与oracle

- 每case独立nonce/trial，caseID固定不回绕；大字ID/nonce/要求/结果。
- `basic_trial/input/check/trial_end/suite_complete`带run/session/suite/case/nonce/trial/platform、UTC ts/utc_ms、相对t_ms；trial带spec/platformExpected。
- Check只读实际控件状态，不发送输入；每trial仅一次Check。管理区单独basic_admin，Check后目标输入另存basic_post_check_input，不回填first。
- 未释放状态使Next禁用；Close/cleanup交真实工具，不伪造成功release。
- 末尾SUITE COMPLETE只表示遍历，不是10/10输入通过。

## 4. 独立analyzer与测试源码

- 新analyzer不修改/依赖旧gesture/text analyzer的全局SPECS或结果。由于B2需要显式拒绝与真实观察尺寸，新小解析器只读已知外层tool_use/tool_result及单层JSON metadata；不递归/输出image。
- case/nonce/run/session/trial + UTC/relative/native时间 + unique call/result interval关联；无第N配第N。
- 缺字段/错focus、native与raw不一致、结果伪成功、工具ID/请求重复、重叠时间、缺模型、错误模型、缺观察尺寸→unknown，不升级pass。
- 检查实际assistant model全部为 `glm-5.3-flash`；只看模型自报不算证据。
- first/final、retry checks/trials、blocked、rejection_confirmed、valid_input分别输出。valid_input只计有原生+工具佐证的首次成功输入；拒绝/blocked不计。
- planned分母固定10；不会以只看见1例就报100%。缺终态仍partial；解析有缺行/坏JSON时清空匹配汇总为unknown。
- 固定 `gui_verified=false` / `ten_valid_trials_per_action_gate=false`，exit0也不是GUI验收。
- 回归源码包含原生字段错配、缺事件/时间/结果、错case/nonce/suite、重复first、first/final区分、wrong HWND/focus、held无up、hold时间、假dispatch、拒绝event0、padding不得升级、scaled实际尺寸、图片不遍历、非法suite/CLI参数等；C#新增覆盖两个suite语义与参数解析。
- **没有执行这些测试。**先写测试源码再实现，但未进行red/green运行；CC独立执行。

## 5. 已执行的仅编译证据

本机：Mono C# compiler 6.12.0.0；Python 3.14.0。
**这只是跨平台C#5语法/引用编译，不是Windows Framework csc结果，也不是Mac/Linux测试或验收。**

最终命令（确实执行；生成物从未运行）：

```sh
out=acceptance-fixture/windows/.compile-only-b2-20260930-202039
/Library/Frameworks/Mono.framework/Versions/Current/Commands/mcs \
  -langversion:5 -target:winexe \
  -out:$out/ComputerUseAcceptance.compile-only.exe \
  -r:System.dll -r:System.Drawing.dll -r:System.Windows.Forms.dll \
  acceptance-fixture/windows/MainForm.cs acceptance-fixture/windows/Cases.cs \
  acceptance-fixture/windows/Gesture*.cs acceptance-fixture/windows/Basic*.cs
PYTHONPYCACHEPREFIX="$PWD/$out/pycache" python3 -m py_compile \
  scripts/analyze-basic-input-gui.py tests/basic_input_gui_analyzer.py
```

最终结果：C# **exit0，空stderr/stdout（无warning）**；Python **exit0**。
证据位于上面out目录的 `csharp-compile.log`、`csharp-exit.txt`、`python-compile.log`、`python-exit.txt`。

保留全部开发编译过程，不掩盖中间失败：

- `.compile-only-b2-20260930-200932/`：C# exit0，1个未使用回调字段warning；Python exit0。字段随后移除。
- `.compile-only-b2-20260930-201935/`：C# exit0；Python exit1，新追加测试源码的多余右括号SyntaxError；拆成具名metadata/result语句修复。
- `.compile-only-b2-20260930-202039/`：最终C#/Python均exit0。

这些目录只含本轮隔离compile-only生成物/日志/缓存，位于授权Windows写集内，不是部署bin。CC **不要复用这些exe**。

## 6. 冻结指纹与CC Windows命令

完整20个输入文件SHA256列表：
`acceptance-fixture/windows/.compile-only-b2-20260930-202039/source-sha256.txt`
（含本轮变更与未修改的Cases/Gesture依赖；报告自身未纳入，避免自引用。）

核心指纹：

- `MainForm.cs`: `675e80f3a188f0c5f05d6ee722e7cc75b84a70e4f7ecd8c8168c7db41073f64b`
- `BasicCases.cs`: `9a40ca4bed926cf4da2cda2e3cdc4429cba6f1d04ec392e261e08c3f965fa6d1`
- `BasicForm.cs`: `fe066d214c9d620df1ae2ac37dfeed5e68344b95cf9d7dc2e08d8c1b75cd431a`
- `BasicJudge.cs`: `d9783bef6b210d0c7bae25090b416081a855afbc60ef24f5c3023e8542b8cc49`
- `BasicNative.cs`: `5c1def6ef4ba51c76522474549d41471f0f0b32206cc1f47f78680475c2b48f8`
- `BasicSelfTest.cs`: `cd94a48f450952bfe79aa2a73453e8138fa3af32f080ae9a088f9df224c32a8b`
- `analyze-basic-input-gui.py`: `630c2f611de4b22e018748ebcea185dc87ef2369b0adf0f085cf0a5ad6d8bd4c`
- `basic_input_gui_analyzer.py`: `cb91d5a8feffc26c5e45b9ae725a0444cc59f910ee0560633a8b100063fe25d9`

以下**仅待CC在Windows新快照目录执行**，遵循其任务的无GUI限制及first-red停止规则：

```powershell
# 放进短的独立调用 .ps1，逐项保留stdout/stderr与数值exit。
$out = Join-Path $env:TEMP ('computer-basic-cc-' + [Guid]::NewGuid().ToString('N'))
powershell -NoProfile -ExecutionPolicy Bypass -File .\acceptance-fixture\build-windows.ps1 -OutDir $out
$code = $LASTEXITCODE
if ($null -eq $code -or $code -ne 0) { throw "csc exit=$code" }
$exe = Join-Path $out 'ComputerUseAcceptance.exe'
& $exe --self-test 1> (Join-Path $out 'self-test.log') 2> (Join-Path $out 'self-test.err')
$code = $LASTEXITCODE
if ($null -eq $code -or $code -ne 0) { throw "self-test exit=$code; STOP and preserve first red" }
& $exe --export-cases (Join-Path $out 'legacy63.json') 1> (Join-Path $out 'export63.log')
$code = $LASTEXITCODE
if ($null -eq $code -or $code -ne 0) { throw "export63 exit=$code" }
& $exe --export-platform-cases (Join-Path $out 'windows-basic20.json') 1> (Join-Path $out 'export20.log')
$code = $LASTEXITCODE
if ($null -eq $code -or $code -ne 0) { throw "export20 exit=$code" }
python .\acceptance-fixture\tools\check-cases-parity.py (Join-Path $out 'legacy63.json')
$code = $LASTEXITCODE
if ($code -ne 0) { throw "legacy parity exit=$code" }
python .\tests\basic_input_gui_analyzer.py
$code = $LASTEXITCODE
if ($code -ne 0) { throw "B2 analyzer tests exit=$code; STOP and preserve first red" }
```

- 避免 `Start-Process.ExitCode=null`；按CC任务直接 `& exe` 并立即保存 `$LASTEXITCODE`。
- 新平台manifest须独立确认pointer10 + keyboard10 + platform=windows；旧export63作旧parity，不给Mac伪造B2。
- 不执行Mac程序或Mac/Linux测试；现有parity所需只读源码/catalog一起带入Windows新快照。
- 正式GUI及离线分析真实记录另等协调者授权。模板已准备，当前CC任务不启动任何GUI/Host/renderer。

## 7. 明确pending、阻断与未覆盖

1. **没有剩余源码实现阻断；已冻结供CC验证。**本机编译工具可用，最终compile-only完成；Windows Framework csc、PowerShell脚本实际执行、全部纯测试/GUI仍pending。
2. pointer-09是明确的**功能阻断**：root padding契约未接，不能通过，不算有效输入。本轮只交付定义、UI状态及不可绕过门控。
3. pointer-03依赖真实观察metadata；如同surface/geometry前序尺寸缺失、没有真正缩小或时间不可信，维持unknown，不推测成功。
4. 拒绝例需可解析的明确invalid_action/not_started结果；工具适配器仅返“success/error” prose或缺字段时unknown，不以app零事件替代。
5. own第二窗首次激活/点击、右键菜单能否直接管理区Check、EDIT Enter/Tab/selection、Alt安全组合、DPI布局/可读性、焦点/消息到达与hold容差，均需要之后真实Windows GUI验证；未宣称工作正常。
6. 时间关联要求工具转录与oracle都有可靠UTC，跨机器时钟偏差会降级unknown，不自动改系统时间。缺关键截图或政策审核仍非GUI验证。
7. 未完成cancel/focus/geometry整套、多屏、跨应用、全部动作至少10次首次有效trial；拒绝、blocked、聚焦/管理click、重试均不凑数。
8. 旧授权弹窗造成的0输入暂停保持原状；没有改权限或绕路。Mac/Linux继续pending。

**SOURCE_FROZEN：本轮停止源码写入，不为等待CC继续扩展；任何CC first-red请基于此指纹新建有界修复任务。**
