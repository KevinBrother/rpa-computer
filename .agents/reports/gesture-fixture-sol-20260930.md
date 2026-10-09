# 原生手势靶场实现交付（Sol 接手）

日期：2026-09-30。工作目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。
状态：**源码已交付、必要编译检查已完成；Windows优先，测试执行、独立审查和真实GUI验收待协调者调度 CC + GLM。Mac/Linux测试全部pending，当前不要执行。**

## 角色与证据边界

- 已完整读取指定任务、现有fixture、用例规范、阶段B实施计划和最新角色覆盖说明。
- 保留停止的旧CLI部分实现，没有重置共享脏工作区、创建worktree、commit/push或启动其它agent/CLI。
- 仅写指定范围：`acceptance-fixture/**`、新gesture分析器/测试、本报告和独占runs目录。
  不修改Rust/Cargo、旧layered分析器/测试、设计/CONTRACT；已有四套text及legacy的Cases源文件未由我改写。
- **全程没有执行测试、self-test、export、parity、GUI输入、截图、窗口入口或SSH。**
  用户收紧角色前也没有运行纯测试。Python `py_compile`只编译语法、不执行被编译脚本或测试方法。
- 原任务要求的TDD fail-first执行证据尚不存在：先编写负例再修改实现，但按最新指令不允许我执行红/绿阶段；
  不把编译错误或静态推断包装成测试失败/通过。此门禁交给CC；源码中的negative cases不是验收结果。
- 已通知协调者源码可排期。以下编译证据仅用于开发可编译性，不替代CC测试或Windows实机构建。

## 改动文件

### 原生fixture与接线

- `acceptance-fixture/macos/GestureCases.swift`：保留旧catalog、事件model、layout和wheel符号契约；
  原841行文件拆分后248行；补原生timestamp/type/precision/phase字段、长距离held时间要求。
- `acceptance-fixture/macos/GestureJudge.swift`：纯判定、按下/释放状态序列、native多击、drag轨迹与scroll实际位移。
- `acceptance-fixture/macos/GestureSelfTest.swift`：保留并修正旧CLI的synthetic自测（缺释放、错误滚轮符号、曲线路径不足反转等）。
- `acceptance-fixture/macos/GestureRegression.swift`：新增顺序/释放/timeout/位置/合并/边界/JSON/export回归代码。
- `acceptance-fixture/macos/GestureViews.swift`：保留旧canvas的有效绘制/clip/firstMouse代码；
  改为top-left逻辑坐标，真实scrollWheel接收、自有编号grid及native NSTextView选择。
- `acceptance-fixture/macos/GestureController.swift`：独立trial状态、seeded布局微移、可读UI、own-window local monitor、oracle。
- `acceptance-fixture/macos/GestureExporter.swift`：在旧33条的manifest上追加30条，携带两平台explicit expected。
- `acceptance-fixture/macos/main.swift`：suite路由、模块接线、自测/export入口；logger增加run/session与毫秒UTC。
  保留旧main的drawing/主屏placement/firstMouse/仅显式coordinator-raise信号功能。
- `acceptance-fixture/windows/GestureCases.cs`：C#5对等catalog、事件model、layout、平台expected及manifest追加。
- `acceptance-fixture/windows/GestureJudge.cs`：C#5纯判定。
- `acceptance-fixture/windows/GestureNative.cs`：真实自有control WndProc，DOWN/UP/DBLCLK、drag held motion、native EDIT、WM_MOUSEHWHEEL。
- `acceptance-fixture/windows/GestureForm.cs`：独立WinForms UI、trial/check/evidence、原生click interval/slop。
- `acceptance-fixture/windows/GestureSelfTest.cs`：不构造控件的纯synthetic回归代码。
- `acceptance-fixture/windows/MainForm.cs`：入口路由、编译前console模式、logger numeric/raw字段和run/session。
- `acceptance-fixture/build-macos.sh`、`build-windows.ps1`：纳入新模块；Windows显式`/langversion:5 /codepage:65001`（UTF-8源码）；仍拒绝覆盖旧产物。
- `acceptance-fixture/start-windows.ps1`：固定suite whitelist加入三套gesture；未执行此脚本。

### 分析器、任务与说明

- `scripts/analyze-gesture-gui.py`：独立离线gesture诊断，无引用/改写旧text分析器。
- `tests/gesture_gui_analyzer.py`：26个unittest方法（仅源文件静态数量；未执行）。
- `acceptance-fixture/tools/check-cases-parity.py`：保留原text检查，新增30条ID/spec/instruction UTF-16 parity；
  支持Mac与Windows两份运行时manifest对照，保留旧33条和原known09语义；未执行。
- `acceptance-fixture/tasks/multiclick.md`、`drag.md`、`scroll.md`：真实CC+GLM、现有8computer工具、首次失败保留。
- `acceptance-fixture/README.md`：suite/原生语义/阈值/待CC命令/限制。
- 本报告；`.agents/runs/gesture-fixture-sol-20260930/`内专用编译日志、编译产物和Python编译cache。

`macos/Cases.swift`、`windows/Cases.cs`属于原共享未提交实现，本轮只读取/编译，不更改旧text/legacy内容。
新手势模块最大376行；没有继续向旧GestureCases堆至千行。

## 实现语义（尚不是运行结果）

### 多击

- 三套catalog各10个唯一case，同一轮不回绕；保留旧gesture caseID次序（single、double、triple）、reset及slow_two流程。
- Mac要求真实`NSEvent.clickCount`序列1..N，真实down/up平衡，native双击时间窗与位置阈值，外部命中/缺释放不通过。
- Windows按真实消息press计数：`DOWN,UP,DBLCLK,UP,DOWN,UP`可以组成三击；
  一个DBLCLK只算一个down，第三下不要求Clicks=3，也不要求必然有第二个DBLCLK。
- Canvas另外记录真实`MouseEventArgs.Clicks`。EDIT在native处理前记录WM消息以避免nested tracking吞事件；
  managed Clicks不可用时明确`-1`及`native_count_source=WM message classification`，不伪装为managed原始值。
- Standard EDIT无通用三击整行保证：Windows `multiclick-05`显式期望三次real down/up、DBLCLK和native非空selection；
  不手动全选来过关。若真实控件第三击使selection为空，本case如实失败，需CC核实平台约定。
- 非法count的“无事件”只证明app未收到输入，不单凭此证明API正确拒绝；拒绝需工具证据。不得算动作输入成功。

### 拖拽

- 必须存在一条完整、有序left down→held motion→up；文本拖选也不能仅凭selected text或终点成功。
- 方向、START/END、轨迹、短/长距离、近边缘释放分别判定；长距离>=500px且held>=750ms。
- 普通/短拖允许仅一个实际held motion样本；折线按实际收到的线段是否经过waypoint附近判断，不要求每个计划点回送。
- 曲线必须有至少两次可观察到的水平反转；合并到不足以证明转向时不编造成功。
- Released属于自有控件状态与收到的up，不声称fixture能证明全局物理button状态或清理其它应用。

### 滚动

- 两个同页panel显示row/column编号与offset，正向case必须有实际位移；错误panel/相邻panel改变不通过。
- Mac保留scrollingDelta与legacy delta、native timestamp/type、precision/inverted/phase/momentum；
  自有content向右/下=`-scrollingDelta`，precision单位=1px、nonprecision=30px，未调用super再重复滚动。
- Windows保留WM消息、原始wParam/lParam、signed delta和GetMessageTime；
  WM_MOUSEWHEEL正量向上、WM_MOUSEHWHEEL正量向右；自有panel120native单位=90px。
- `raw_*`和`contract_*`分开：后者统一正右/下**符号**，不是把两平台raw单位说成相同。
- 小幅位移>=15px、3tick位移>=45px；saturate case必须真实到达max且有实际位移，不接受起点就在底部的零移动。
- zero/invalid cases要求没有wheel事件及没有offset变化，单列非输入案例。

### Oracle与分析器

- app-only记录run/session/suite/caseID/nonce/trial、UTC毫秒、trial开始/spec/platform_expected，
  原始事件/归一符号、own-control可见offset/selection/released、event_count和first/final check。
- Check/Next/Finish不记入target事件；每trial reset；完成标记只表达visited，不洗成matched。
- Mac text tracking的monitor是application-local并限定当前自有NSWindow，不是global hook。
- 分析器按run/session/case/nonce/time核对事件和checks，拒绝伪case、spec偏离、重复first、未归属/错nonce、缺时间/缺原生stream。
- 工具名按实际`computer_step`处理。只在原生事件被唯一call/result时间包络包住时关联参数；
  coarse/同时间多candidate、无结果、时间缺失→unknown，不使用全局第N个call配第N个check。
- `first_*`与`final_*`分别统计；final是最后一份被记录的Check快照，不宣称知道未Check的后续状态。
  first不能被重试升级。缺terminal的partial即使10条match也不输出100%。
- `first_input_successes`排除zero/reject；`model_verified`核对全部实际assistant model标签是否为`glm-5.3-flash`。
- 不递归读取/输出tool_result中的图片内容；报告固定`gui_verified=false`，policy与截图仍是独立CC门禁。

## 实际执行的必要编译检查

均没有启动任何编译产物。

| 命令 | 实际结果 | 日志 |
|---|---|---|
| `xcrun swiftc -typecheck main.swift Cases.swift Gesture*.swift` | 初次旧代码`dir2` optional未解包导致exit1；修正后typecheck exit0，无测试 | `macos-typecheck-01.log`、`-02.log`、`-03.log` |
| `mcs -langversion:5 -target:winexe ... -r:System.dll -r:System.Drawing.dll -r:System.Windows.Forms.dll` | exit0，本机Mono的C#5语法/引用编译；**不是Windows csc实机构建** | `csharp5-compile-handoff.log`，早期版本日志另留 |
| `PYTHONPYCACHEPREFIX=<专用runs>/pycache python3 -m py_compile ...` | exit0，仅语法编译；没有import测试模块或执行测试方法 | `python-syntax-handoff.log` |
| `./acceptance-fixture/build-macos.sh "$PWD/.agents/runs/gesture-fixture-sol-20260930/macos-build-final"` | exit0，真实AppKit链接构建；binary未运行 | `macos-build-final.log` |

以上日志都位于`.agents/runs/gesture-fixture-sol-20260930/`。

Mac构建产物：
`macos-build-final/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance`。
SHA-256：`0e2a864a685b79fadcaea247466dd9406178715a0e98cb950deaaf64bd60ec28`。

本轮没有“测试通过数”、“GUI成功率”或runtime export/parity成功证据，不能据上述exit0填写这些门禁。

## CC + GLM 接手命令：Windows优先（尚待执行）

**用户最新优先级：先Windows；当前不要执行Mac/Linux测试、Mac self-test/export或两平台runtime parity。**
Mac源码和既有编译证据保留，不计为Mac测试通过。实现者没有执行下列任何命令。

协调者复制`acceptance-fixture/windows/*.cs`（包括旧Cases.cs和全部新Gesture*.cs）、
`build-windows.ps1`、start/stop辅助脚本到Windows，再调度真实CC+GLM。
不复制本机Mono编译产物去冒充Windows csc产物。输出目录必须全新，不覆盖旧exe。

```powershell
# CC在Windows源码副本上执行；路径示例可由协调者替换为独立诊断目录。
$Out = 'C:\Temp\gesture-fixture-cc-20260930\windows-build'
$Evidence = 'C:\Temp\gesture-fixture-cc-20260930\evidence'
New-Item -ItemType Directory -Force -Path $Evidence | Out-Null
powershell -NoProfile -ExecutionPolicy Bypass -File .\acceptance-fixture\build-windows.ps1 -OutDir $Out
if ($LASTEXITCODE -ne 0) { throw 'Windows csc build failed' }
$Exe = Join-Path $Out 'ComputerUseAcceptance.exe'
& $Exe --self-test *> (Join-Path $Evidence 'windows-self-test.log')
if ($LASTEXITCODE -ne 0) { throw 'Windows self-test failed' }
& $Exe --export-cases (Join-Path $Evidence 'win-cases.json')
if ($LASTEXITCODE -ne 0) { throw 'Windows export failed' }
Get-FileHash -Algorithm SHA256 $Exe
```

按最新指令，Python analyzer纯测试也由CC在Windows执行（需要可用Python3，不能改为本机Mac跑）：

```powershell
python tests/gesture_gui_analyzer.py
# 可选：仅Windows导出与两份源码catalog的静态检查，不启动Mac应用，不是Mac runtime验收。
python acceptance-fixture/tools/check-cases-parity.py <Windows证据目录>\win-cases.json
```

GUI必须在用户的interactive console session由协调者串行调度CC+GLM，不能在SSH/Session0启动。
由既有受控helper准备目标窗口（真实GUI阶段由CC执行，源码实现者不调用）：

```powershell
$Hash = (Get-FileHash -Algorithm SHA256 $Exe).Hash
.\acceptance-fixture\start-windows.ps1 -ExePath $Exe -EvidenceDir $Evidence -Suite multiclick -ExpectedHash $Hash
# 完成并核实本轮后，用返回的run-record.json身份受控停止，再分别安排drag、scroll。
# .\acceptance-fixture\stop-windows.ps1 -RecordPath <本轮返回的run-record.json>
```

三个任务分别为`acceptance-fixture/tasks/multiclick.md`、`drag.md`、`scroll.md`。
GLM只用现有8computer工具，首次失败保留，Check一次，不重试凑成功；
Next/Finish至SUITE COMPLETE；Close computer session但保留fixture。
独立CC随后按suite离线诊断（例，Windows Python执行）：

```powershell
python scripts/analyze-gesture-gui.py --suite multiclick --oracle <oracle.jsonl> --transcript <cc-stream.jsonl> --output <Windows证据目录>\multiclick-analysis.json
# drag、scroll分别独立执行；政策审计与关键截图由CC另外核对。
```

Mac/Linux测试、Mac export和两平台runtime parity保持pending，待用户解除暂缓后另行排期。

## 未验证与范围缺项

1. Windows实际.NET Framework csc编译及self-test/export。
2. 两平台运行时catalog export/parity、26个Python测试、Mac/Win synthetic状态机self-test、fail-first证据。
3. 全部30条case的真实GUI首次尝试、actual GLM标签/工具政策、截图与控件可读性/无重叠验证。
4. Mac native NSTextView nested tracking是否完整送达local monitor；Windows EDIT第三击的真实selection结果；
   WM_MOUSEHWHEEL在实际窗口/焦点配置中的路由；precision/native scrolling与当前runtime backend方向单位的对照。
5. 指定完整规范中的pointer、keyboard、cancel、focus、geometry、多屏、跨应用suite仍未实施于本写集。
6. 旧multiclick catalog的case9为slow_two而不是监督者执行中取消：**没有宣称取消partial/release覆盖**。
   原完整规范的前三个multiclick ID排序与旧catalog也不同，本次保留旧部分实现的ID次序，GUI任务以可见要求为准。
7. 每基础动作至少10次“首次有效输入”需要trial展开；multiclick含非法count、scroll含zero/reject，
   三套各10个semantic case不能直接填满此门禁。随机布局/nonce也不冒充新增semantic case。
8. known09未来按平台修正expected的新run尚待专门工作；本轮不污染旧期待或洗改历史结果。
9. 独立源码审查未由实现者启动其他agent；交协调者调度CC+GLM审查。

结论：实现与必要编译检查可交接；没有任何理由将本报告标成30例GUI通过或完整computer-use交付完成。
