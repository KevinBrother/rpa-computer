# Computer Use Acceptance Fixture（视觉验收靶场 · 分层版）

给“只看截图”的 computer-use Agent 用的原生视觉靶场：每次试验生成全新随机内容，
Agent 必须通过截图识别 nonce、在四个打乱的图形中找到唯一目标（绿圆）并点击、
再在文本框里精确输入样例文本。**坐标和答案不会提供给模型。**

## 三条验收链路（`--suite` 分层）

历史版本把一段含 emoji/全角标点的复杂样例和输入注入混在一起，失败无法归因。
现按归因维度拆成独立 suite，**每个 suite 是独立靶场，结果绝不互相合并**：

| suite | case 数 | 链路 | 测什么 |
|---|---|---|---|
| `baseline` | 10 | **纯视觉 → 输入** | 短、清晰、互不相同的常用中文/ASCII/数字文本，仅看截图抄写。无 emoji、无特殊空格、无全半角标点混淆；仅普通空格分词；长度含 12/34，≤40。**此 suite 的 text 失败可直接归因输入注入。** |
| `punctuation` | 6 | **纯视觉 → 输入（标点）** | 每条独立覆盖全/半角括号、中/英文逗号、竖线、引号。不夹 emoji。特殊空格视觉不可辨，**不属于**此“必过”矩阵（NBSP 放 known-input）。 |
| `emoji` | 6 | **纯视觉 → 输入（emoji）** | 每条含一个常用不同 emoji（含 🌍✅），放大字形显示。**Windows 侧 WinForms 可能以单色（Segoe UI Emoji）渲染，截图不能唯一判定码点，此 suite 结果须如实记录限制，不计入“必过”矩阵。** |
| `known-input` | 10 | **已知原文 → 输入** | payload 由协调者**以原文直接交给 Agent**（不做视觉推断）。覆盖 ASCII、中文 12/34 字、历史 38-scalar 长样例、全角标点、NBSP、非 BMP emoji、组合字符（e+U+0301）、CRLF/Tab。期望值 = payload 按 runtime 契约做 CRLF→LF 归一化；`task_payload` 与 `expected_text` 在 manifest/证据中**始终分开**。CRLF/Tab 进入文本 view 的行为若不稳定，如实记录，**不删除案例**。 |
| （legacy，默认） | 1 | 历史单样例 | 不带 `--suite` 时的旧行为（无限 Next + 历史混合样例）。**legacy 结果与新 suite 结果禁止合并统计；旧失败数据不因本次分层被“洗白”。** |

## 构建

macOS（本机）：

```bash
./acceptance-fixture/build-macos.sh /path/to/output-dir
# 产物: <output-dir>/ComputerUseAcceptance.app
```

Windows（在 Windows 机器上执行；可经 SSH 仅做编译，绝不要在 SSH 会话里启动 GUI）：

```powershell
powershell -ExecutionPolicy Bypass -File build-windows.ps1 -OutDir C:\path\to\output
# 产物: <OutDir>\ComputerUseAcceptance.exe
```

两个脚本都只写自己指定的输出目录，已存在的同名产物会报错退出而不是覆盖。

## 启动参数

```
ComputerUseAcceptance[.exe] [--seed N] [--evidence-file PATH]
                            [--suite legacy|baseline|punctuation|emoji|known-input|multiclick|drag|scroll]
                            [--self-test] [--export-cases PATH]
                            [--coordinator-raise]   # macOS only
```

- `--seed N`：可选项，固定 nonce/布局序列，便于协调者复现。
- `--evidence-file PATH`：可选项，**仅协调者使用**的 JSONL 证据文件（见下）。
- `--suite <name>`：分层 suite。**未知 suite 或缺值会明确报错退出（exit 1），绝不无声回落 legacy**；省略时保持 legacy 行为（历史样例，不得与新结果合并）。
- `--self-test`：无窗口/无截图/无输入的纯逻辑回归。检查各 suite case 数量、唯一 ID、nonempty、suite 定义、CRLF 归一化 expected 与 payload 差异、UTF-16 精确比较能拒绝 NFC/NFD 差异、legacy 保留；另含离屏绘制回归（未 clip 白色 fill 越界可检出、ShapesView fill/图形绘制严格限于自身 bounds）与真实 API 行为回归 `first-mouse-accepted`（`ShapesView.acceptsFirstMouse(for:) == true`）。**注意：这仅测试 fixture 自身的 click-through 行为，不代表所有 Mac 应用 first-click 都可直接激活控件。** 全部通过 exit 0，否则 exit 1（exit code 是权威判据；Windows GUI 子系统的 stdout 仅在重定向时可见，如经 ssh）。
- `--export-cases PATH`：**仅协调者**导出全部 suite 的 case 目录（id、task_payload、expected_text）为 JSON 后退出，不创建窗口。**此文件含答案，绝不能进入 GUI Agent 的发布目录、cwd、MCP 资源或 prompt。**
- `--coordinator-raise`（仅 macOS）：**协调者专用环境准备，不属于 Agent 动作**。GUI 路径注册 POSIX `SIGUSR1`（DispatchSource 主队列，进程内保留强引用）；收到信号时仅对本应用自己的单个窗口执行 `makeKeyAndOrderFront` / `orderFrontRegardless` + `NSApp.activate`，并在 evidence 记录 `coordinator_raise` 事件（pid/raised 及 key/active 布尔元数据，无任何文本答案）。`windowDidBecomeKey/windowDidResignKey` 另记录 `window_key` 事件（key/active 布尔），便于将 first-click 行为与 focus 状态关联。安装完成后 stdout 打印 `COORDINATOR_RAISE_READY=1 PID=<pid>`。用途：opaque backdrop 可能盖住 fixture 窗口（activation 不保证 raise），由协调者核对产物/PID/exe 身份后向该进程发 SIGUSR1 让 fixture 自己浮到前面。**绝不触碰其他 app/窗口；不用 AX/AppleScript/按键；不注入键鼠；不使用 always-floating 层级。** 未给该开关时不注册、不打印 ready。`--self-test`/`--export-cases` 仍提前退出、无 GUI。未知参数一律 fail-closed（exit 1）。**GUI Agent 不允许通过 shell 调用此开关**（Agent 工具与文件隔离边界不变）——只有协调者在启动 fixture 的会话里使用。

## 试验内容（Agent 可见）

- `Trial i[/N — caseID — suite: xxx]` 编号（suite 模式 header 明确显示当前 caseID，如 `Trial 1/10 — baseline-01 — suite: baseline`）、`NONCE: XXXXXX`（6 位随机字符）、四宫格图形、状态条、多行文本框。
- 目标规则就印在窗口上：*TARGET: the only GREEN CIRCLE — click it*（唯一绿圆）。
- 点对：状态条显示 **TARGET HIT**；点错：**WRONG TARGET**。
- 样例以大字号（约 24–28pt；emoji suite 更大）多行显示，不裁切、不超框。
- 文本框输入样例后点 **Check text**：与 expected_text **逐 UTF-16 码元完全一致**才显示 `MATCHED`，否则 `MISMATCH`（不做 Trim、不做任何 Unicode 归一化；Swift `String ==` 的规范等价语义被显式规避）。
- **Next trial**：新 nonce、新布局、推进到下一 case。suite 内 case 逐个推进、不回绕；
  最后一个 case 之后按 Next 显示 **SUITE COMPLETE — N/N cases**，继续 Next 不再出新 case。
- macOS 与 Windows 两个平台的 case 目录完全一致（逐 UTF-16 校验，见 `tools/check-cases-parity.py`）。

## 证据（协调者专属，与 GUI 成功分开判定）

`--evidence-file` 写入 JSONL，每行一个事件，带 UTC 时间戳：

| type | 含义 |
|---|---|
| `session` | 启动，记录 seed、platform、suite |
| `trial` | 新试验：trial、nonce、target_slot、layout、**suite、case_id、case_index、case_total、expected_text** |
| `hit` / `wrong` | 点击结果：trial、nonce、slot、target_slot、suite、case_id、case_index、case_total |
| `text_check` | 文本校验：matched（逐 UTF-16 相等）、expected_len、got_len、suite、case_id、case_index、case_total、task_payload、expected_text、actual_text、**expected_utf16_hex、actual_utf16_hex** |
| `suite_complete` | suite 内全部 case 推进完毕（legacy 模式不产生） |

- oracle **只**写入协调者指定的 evidence 文件，不自动暴露给 Agent；不做 clipboard/OCR/UIA 辅助。
- legacy 模式字段约定：`suite=legacy`、`case_id=legacy-sample`、`case_total=0`（0 表示无限模式），`case_index` 随 trial 递增。
- `actual_text` 是文本框原样内容。CRLF/Tab 组合字符在文本 view 中的行为如与 payload 不一致，会直接体现在 `actual_utf16_hex` 中——这是诚实记录，不是 bug 修复对象。

注意：

1. 该文件**绝不能**出现在测试 Agent 的发布目录、cwd、MCP 资源或 prompt 中——
   它包含答案（nonce、目标位置、expected_text），只供协调者核对。
2. 它是**原生侧证据**，不替代视觉轨迹验证：判定通过仍须核对 Agent 的截图与点击序列。
3. 只记录本应用窗口内的交互，无全局截屏 / 键盘记录。

## 两条 UI 实现（同 case 契约）

| 平台 | 源码 | 技术 |
|---|---|---|
| macOS | `macos/main.swift` + `macos/Cases.swift`（纯逻辑，无 AppKit） | AppKit + swiftc（xcrun） |
| Windows | `windows/MainForm.cs` + `windows/Cases.cs`（纯逻辑，无 WinForms） | .NET Framework 4.x WinForms + csc.exe（C# 5 语法） |

- 窗口：macOS 900×760 / Windows 客户区 884×712（约 900×750），标题 `Computer Use Acceptance`，含「Cancel / Close」按钮，均不超出 1080 画面。
- `macos/Cases.swift` 与 `windows/Cases.cs` 必须保持同一 case 契约；改动任一后运行：

```bash
python3 acceptance-fixture/tools/check-cases-parity.py [runtime-manifest.json]
```

- 无任何第三方依赖、无内嵌浏览器、无文件系统/浏览器/终端 UI。

## 协调者预检命令

macOS（本机，无窗口）：

```bash
acceptance-fixture/build/<out>/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance --self-test
acceptance-fixture/build/<out>/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance --export-cases /path/to/cases.json
```

Windows（在 Windows 机器上执行，只做编译与自检，不拉起 GUI）：

```powershell
# 编译后：
C:\path\to\ComputerUseAcceptance.exe --self-test
C:\path\to\ComputerUseAcceptance.exe --export-cases C:\path\to\cases.json
```

## 操作员准备

- 把编译产物单独复制到一个**不含源码**的发布目录交给测试 Agent；
  本仓库整体源码（含本 fixture 源码）对测试 Agent 保持不可见。
- **绝不**把 `--export-cases` 产物或 evidence 文件放进该发布目录。
- 遮挡 / 最小化所有含源码的窗口（编辑器、终端历史、本 README），
  桌面只留靶场窗口与被测 Agent 运行环境。
- known-input suite 需协调者把 `task_payload`（注意不是 expected_text）以原文交给 Agent。
- macOS 首次运行未签名 app：右键打开，或 `xattr -d com.apple.quarantine <app>`。
- Windows：必须在交互式桌面会话启动，不要在 SSH 服务会话里拉起 GUI。

## 协调者启动/停止（Windows 交互会话）

GUI 无法从 SSH 会话直接拉起（Start-Process/WMI 都会落在 Session 0）。
`start-windows.ps1` / `stop-windows.ps1` 由**协调者**在远端通过一次性计划任务
在**当前活动控制台会话**里启动/停止已编译的 exe。脚本只做这一件事，
不做截图、不跑模型循环；**进程存活不等于 GUI 成功**，判定仍需协调者核对截图与 oracle。

部署（只复制两个辅助脚本到已知诊断目录；exe 已在远端编译好）：

```bash
scp acceptance-fixture/start-windows.ps1 acceptance-fixture/stop-windows.ps1 \
    acer-win:C:/Temp/accfix/
```

启动（协调者执行；先自建一个证据目录，脚本在其中新建 GUID 唯一运行目录，
只复制 exe、不带源码，oracle 证据 JSONL 也在该运行目录内）：

```bash
ssh acer-win "powershell -NoProfile -ExecutionPolicy Bypass \
    -File C:\Temp\accfix\start-windows.ps1 -EvidenceDir C:\Temp\accfix\evidence -Suite baseline"
```

- `-Suite` 受 `ValidateSet` 保护（`legacy|baseline|punctuation|emoji|known-input`），
  只向 exe 传固定 suite 枚举（`--suite <name>`；legacy 时不传），**绝不传任意命令**；
  非法值在参数绑定阶段直接被 PowerShell 拒绝。
- 会话解析：`WTSGetActiveConsoleSessionId` 取活动控制台会话，该会话内 explorer.exe
  的属主作为任务 principal（Interactive / Limited），不依赖 SSH 登录账号。
- 计划任务直接执行 staged exe + `--evidence-file <oracle>`（+ 可选固定 `--suite`），
  无 shell、无任意命令；每次运行新任务名（GUID），不删不改任何已有目录/任务；
  执行时限 30 分钟兜底。
- 源 exe 默认 `C:\Temp\accfix\ComputerUseAcceptance.exe`，启动前校验 SHA-256
  （默认期望值对应**旧 legacy-only 版本 exe**；部署新版 exe 后必须用 `-ExpectedHash`
  传入新编译产物哈希）。
- 输出 oracle 路径与 PID（**不含 nonce 内容**），并在运行目录写 `run-record.json`
  （含本次 suite）。

停止（只停记录且复核身份匹配的那一个任务/进程，证据目录保留）：

```bash
ssh acer-win "powershell -NoProfile -ExecutionPolicy Bypass \
    -File C:\Temp\accfix\stop-windows.ps1 \
    -RecordPath C:\Temp\accfix\evidence\run-<GUID>\run-record.json"
```

任何身份不匹配（任务 action/参数/principal、进程 PID/路径/会话/创建时间）都会拒绝操作，
不做模式匹配强杀，不删源 exe 或任何用户文件。

## 允许的测试 prompt 模板（不含 nonce / 坐标 / 答案）

> 屏幕上有一个标题为「Computer Use Acceptance」的应用窗口。请：
> 1. 读出窗口中显示的 NONCE 并复述；
> 2. 点击窗口中唯一的绿色圆形；
> 3. 在文本框中输入窗口上印出的样例文本，然后点击「Check text」；
> 4. 报告状态条与校验结果文字。
> 只能用截图观察，只能通过鼠标键盘操作这个窗口。

（known-input suite 的模板改为：协调者在任务描述里给出 payload 原文，要求 Agent
原样输入该文本——不依赖截图识别文字内容。）

## 2026-09-30 原生手势靶场（实现交付，测试交由 CC + GLM）

新增三个独立 suite：`multiclick`、`drag`、`scroll`，每套10个唯一 case。
旧四套 text 的32条与 legacy 一条导出契约保持不变（总共63条、8个suite），
尤其**没有更改 known-09 的期望或历史结果**。本次不是完整 computer-use 矩阵完成。

| suite | 核验方式 | 非输入案例 |
|---|---|---|
| multiclick | 自有目标真实 down/up；Mac原生clickCount1..N；Windows真实DOWN/UP/DBLCLK，每条DBLCLK只计一次press；可见原生文本选择 | `-10` 非法count无事件，拒绝须另查工具证据 |
| drag | 一次按下→held motion→释放；START/END/方向/可见轨迹；折线以收到的线段容忍合并；长距离≥500px且held≥750ms；边缘带≤45px且距中心≥40px | 无 |
| scroll | 同页两panel，真实wheel +实际offset/行列号；3 ticks至少45px、小量至少15px；边界case必须实际到达bottom且有位移 | `-09`零量、`-10`超限：无wheel、无位移，不计页面滚动成功 |

### 平台语义与证据

- Mac `NSEvent.clickCount` 不伪造。Windows 可能出现
  `DOWN,UP,DBLCLK,UP,DOWN,UP`：第三下无需Clicks=3，更不能把Clicks=2当两次down。
  Win canvas 另外保留真实`MouseEventArgs.Clicks`；EDIT原生消息分类与managed Clicks不可用分开标记，
  `native_count_source` 显式说明来源，不把推导计数冒充原始managed字段。
- Mac `NSEvent` 同时记录`scrollingDeltaX/Y`、`deltaX/Y`、precise/inverted/phase/momentumPhase，
  fixture的content正右/下=`-scrollingDelta`；precision事件1原生单位=1px，非precision1单位=30px。
  Win vertical `WM_MOUSEWHEEL` 正delta向上，horizontal `WM_MOUSEHWHEEL` 正delta向右；
  保留原始消息、wParam/lParam和signed delta，120 native单位在自有panel中移动90px。
  `contract_dx/dy` 为正右/下的符号约定，**不是声称两平台单位相同**。
- Native文本控件选择不由fixture伪造。Mac三击要求整句；Windows标准EDIT没有通用三击选行保证，
  当前显式期望为三次真实down/up、至少一次DBLCLK、可见非空选择。实际原生选择不符合时应失败，
  不能偷偷全选来过关。
- Oracle为app-only：每条包含run/session、suite、caseID、nonce、trial、UTC毫秒时间；trial含开始时间、
  spec/platform_expected；input_event含真实原始字段与独立语义字段；check含first/final、event_count、
  可见选择/offset/released。Mac仅对自有window使用local monitor观察native text tracking，**不是global hook**。
  Check/Next/Finish的点击不计target；每例reset；最后SUITE COMPLETE仅代表visited，不代表通过率。
- 保留主屏布局、边界clip、目标acceptsFirstMouse及仅显式`--coordinator-raise`时注册owned raise。
  本次没有启动任何窗口/截图/注入输入；控件可读性、native tracking与滚动路径仍待CC真实截图核验。

### CC + GLM 的待执行命令（实现者不运行）

> **最新排期：Windows优先。当前不要执行Mac/Linux测试，Mac self-test/export及两平台runtime parity全部pending。**
> 下方Mac命令仅为以后解除暂缓时的参考；当前CC先执行Windows构建/self-test/export及三个GUI任务。
> Python测试与分析器也交CC在Windows执行，不由实现者或本机Mac代跑。

只在协调者调度的真实 Claude Code + GLM 会话执行；先做纯验证，再由协调者独占桌面安排GUI。
**编译成功不等于任何一条case通过；下列命令尚无本轮CC执行证据。**

macOS（把`<fresh-output>`替换为全新目录，build脚本不会覆盖旧产物）：

```bash
./acceptance-fixture/build-macos.sh <fresh-output>
<fresh-output>/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance --self-test
<fresh-output>/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance --export-cases <mac-cases.json>
python3 tests/gesture_gui_analyzer.py
python3 acceptance-fixture/tools/check-cases-parity.py <mac-cases.json>
```

Windows（协调者复制源码后由CC在Windows实际.NET Framework csc构建；不要用Mono结果替代）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\acceptance-fixture\build-windows.ps1 -OutDir <fresh-win-output>
& <fresh-win-output>\ComputerUseAcceptance.exe --self-test
& <fresh-win-output>\ComputerUseAcceptance.exe --export-cases <win-cases.json>
```

协调者收回Windows导出后，CC核对两平台运行时清单（含保留的text/legacy）：

```bash
python3 acceptance-fixture/tools/check-cases-parity.py <mac-cases.json> <win-cases.json>
```

三个source-free GLM任务是`tasks/multiclick.md`、`tasks/drag.md`、`tasks/scroll.md`。
只允许既有8个computer工具；每例仅一次首次完整要求/Check一次、失败保留，最后Close computer session但留fixture。
离线诊断每个suite独立运行：

```bash
python3 scripts/analyze-gesture-gui.py --suite multiclick --oracle <oracle.jsonl> --transcript <cc-stream.jsonl> --output <report.json>
# drag / scroll 同样按suite独立执行
```

分析器不会按第N个调用配第N条check：按case/nonce/run/session/time界定输入，要求真实owned事件与
工具call/result的唯一因果时间包络。时间戳、结果、原生字段缺失或歧义→unknown。
`first_*`与`final_*`分开；分母为10个planned唯一case，不拿已见的1例算100%；
即使10例看似匹配但没有SUITE COMPLETE，partial也不输出100%。
zero/reject不计`first_input_successes`。raw tool-result图片数据不递归遍历/输出。
工具政策审核仍由已有独立审计脚本完成；截图由协调者/CC实际看。分析器本身不是GUI验收门禁。

**余项**：pointer、keyboard、cancel、focus、geometry、多屏、跨应用suite；
每基础动作至少10次首次有效操作的trial展开（本套含拒绝/零量，不能凑此门禁）；
监督者多击取消partial/release；known09的未来新平台修正run。
multiclick保留旧实现的caseID次序与slow_two流程，并未把case9称为“取消已覆盖”。

### 当前 Windows 优先交接

`build-windows.ps1`使用.NET Framework csc、`/langversion:5 /codepage:65001`，无需NuGet；
主入口在创建Form/启用GUI前处理self-test/export。全部新源码为`windows/Gesture*.cs`，
须连同原`MainForm.cs`与`Cases.cs`复制，不能遗漏模块或用本机Mono产物替代。
先由CC实际csc构建、自测、导出，再由协调者按interactive console session串行安排
multiclick/drag/scroll任务；start helper要传新exe的SHA-256，不能沿用旧legacy哈希。
Windows优先命令及停启身份记录详见`.agents/reports/gesture-fixture-sol-20260930.md`。
Mac/Linux测试目前全部pending；此前Mac编译不是测试或验收。

## Windows B2：pointer / keyboard（2026-09-30，源码待 CC 验收）

本节是 Windows-only 新增实现；上文旧跨平台导出仍只有 **63 cases**，旧89项self-test逻辑未改，
在原self-test后追加B2纯测试。**Mac实现/测试仍pending；此批不改旧Windows部署、raw oracle或GUI snapshot。**
实现者只做源码与必要compile-only；下列测试/运行命令仅交CC在Windows执行，不是已通过的证据。

### 模块、入口与判定边界

- `windows/BasicCases.cs`：pointer/keyboard各10固定ID、平台要求、Windows独立manifest。
- `BasicArguments.cs`：严格参数解析，缺值/重复/未知参数/suite/非法seed/mode冲突明确exit1。
- `BasicNative.cs`：自有 HWND 鼠标 WndProc；自有 EDIT 的线程消息过滤器（真实WM_KEY/SYSKEY/CHAR，先于WinForms Tab/menu预处理）。
  不安装全局hook，不读别的应用按键；自有Alt+J/Ctrl+Shift+J在记录真实消息后仅更新安全命令指示，无系统快捷键。
- `BasicJudge.cs`：pure raw/semantic判定；`BasicForm.cs`：大字UI、trial/nonce、Check-once、Next/Finish、oracle。
- `BasicSelfTest.cs`：追加纯合成测试，不创建窗口；不是GUI证据。
- `--suite pointer|keyboard` 只在Windows可用。省略suite仍legacy。
- `--export-cases PATH`：原legacy/text/gesture **63条语义不变**。
- `--export-platform-cases PATH`：独立 `platform=windows`、仅pointer/keyboard共20条，不混进旧63 parity。
- self-test / 两种export 均在 `Application.EnableVisualStyles` 与任何窗口创建之前返回。
- 鼠标源包括真实中键消息，双击消息不会被当作单击两次；中心命中要求中心±16px，内边缘条宽12px，小目标24×24。
  非激活第二窗必须有own WM_MOUSEACTIVATE之前非active证据且down/up来自实际第二窗目标HWND。
- 键盘readback使用原生 `TextBox.Text/SelectionStart/SelectionLength`，不normalize actual。
  Enter/Tab期望原生CRLF+TAB；a/b期望默认Latin布局，布局不同如实失败，不改系统设置。
  shift hold要求真实GetMessageTime down/up，120ms/900ms允许 -40/+600ms 调度容差，同时工具参数必须精确。
- `basic_trial/input/check/trial_end/suite_complete`含run/session/case/nonce/trial、spec/platformExpected、UTC/ms/相对时间；
  raw MSG/native timestamp/wParam/lParam/own+focus HWND，与area/modifier/visible状态分别存储。
  Check不补发输入；管理操作单独 `basic_admin`，Check后输入单独 `basic_post_check_input`。
  未释放时Next禁用；不能以无重复字符声称release。

### runnable 与 blocked（不是通过数量）

- pointer-01/02/04..08：可供CC实机首次操作；pointer-03原生命中还需真实缩小的MCP observation尺寸关联。
- pointer-09：**blocked / needs_capability**，当前root无padding map，始终不升级pass、不发送目标输入。
  完整后续门控定义为：actual observation_id + 明确padding矩形 + 内部图像坐标 + 明确not_started拒绝 + own目标event0。
  只有提供真实新能力后的独立实现/新run才可解锁，不能手改oracle升级。
- pointer-10、keyboard-10：拒绝case；app event0仅 NEEDS_TOOL_EVIDENCE；分析器还需唯一真实invalid_action/not_started与clean result。
- keyboard-01..09：可供CC实机运行，modifier/hold必须有完整原生down/up；聚焦click不算键盘目标输入。
- 不是cancel/focus/geometry整套、多屏、跨应用或每动作10个有效trial的交付。

### 新独立分析器与统计

`scripts/analyze-basic-input-gui.py`仅处理pointer/keyboard；`tests/basic_input_gui_analyzer.py`为新纯回归源码。
为拒绝/缩图读取明确的JSON结果metadata，采用独立小解析器，不修改/依赖旧gesture/text分析器的私有状态。
只读外层tool_use/tool_result和已知单层MCP text metadata包络；不递归image或输出图片。
缺少时间戳/显式结果、误case/nonce、模糊多调用、native/raw/focus不一致均unknown；禁止第N配第N。
显式检查actual model `glm-5.3-flash`；非模型证明不能计入matched。

`first`/`final`、retry checks/trials、blocked、rejection_confirmed、valid_input分别输出。
`first_pass_rate`/`final_pass_rate`分母为10个planned唯一case；valid_input仅指有原生+工具佐证的首次匹配输入，
不是所有注入尝试数。拒绝不并入matched或valid_input；因本批每suite含拒绝/blocked，不会拿partial报100%。
固定 `gui_verified=false`、`ten_valid_trials_per_action_gate=false`，不可替代工具政策和关键截图审阅。
程序只输出diagnostic，exit0不意味着GUI通过；`--output`使用新文件，拒绝覆盖既有报告。

### CC Windows 待执行命令（此处仅列出，不代表运行过）

由CC在**新的固定源码快照目录**执行，`$out`必须全新；不能覆盖原 `computer-gesture-cc-20260930-185517`。
按当前CC排期，用独立 `.ps1` 中直接 `& exe` 并立即保存 `$LASTEXITCODE`；不依赖 `Start-Process.ExitCode`（可能为null）。

```powershell
$out = Join-Path $env:TEMP ('computer-basic-cc-' + [Guid]::NewGuid().ToString('N'))
powershell -NoProfile -ExecutionPolicy Bypass -File .\acceptance-fixture\build-windows.ps1 -OutDir $out
if ($LASTEXITCODE -ne 0) { throw 'csc build failed' }
$exe = Join-Path $out 'ComputerUseAcceptance.exe'
& $exe --self-test 1> (Join-Path $out 'self-test.log') 2> (Join-Path $out 'self-test.err')
$code = $LASTEXITCODE
if ($null -eq $code -or $code -ne 0) { throw "self-test failed/exit missing: $code; preserve first failure" }
& $exe --export-cases (Join-Path $out 'legacy63.json') 1> (Join-Path $out 'export63.log')
$code = $LASTEXITCODE
if ($null -eq $code -or $code -ne 0) { throw "legacy export failed/exit missing: $code" }
& $exe --export-platform-cases (Join-Path $out 'windows-basic20.json') 1> (Join-Path $out 'export20.log')
$code = $LASTEXITCODE
if ($null -eq $code -or $code -ne 0) { throw "B2 export failed/exit missing: $code" }
python .\acceptance-fixture\tools\check-cases-parity.py (Join-Path $out 'legacy63.json')
python .\tests\basic_input_gui_analyzer.py
# 可另跑旧分析器回归；不更改其文件/断言。
python .\tests\gesture_gui_analyzer.py
# 收回真正GUI转录/oracle后，两suite分别分析；输出文件必须不存在：
python .\scripts\analyze-basic-input-gui.py --suite pointer --oracle <pointer-oracle.jsonl> --transcript <pointer-cc.jsonl> --output <new-pointer-analysis.json>
python .\scripts\analyze-basic-input-gui.py --suite keyboard --oracle <keyboard-oracle.jsonl> --transcript <keyboard-cc.jsonl> --output <new-keyboard-analysis.json>
```

Windows原生csc与全部测试仍待CC。UI只能协调者按既有 `start-windows.ps1` 的 interactive console机制启动，
参数白名单已增加pointer/keyboard；必须新exe/new SHA256/newrun，无源码GUI目录，旧部署不动。
GLM任务分别为 `tasks/windows-pointer.md` 与 `tasks/windows-keyboard.md`，只8工具、首帧缺窗/授权弹窗STOP+close、
不改权限、不使用AltTab/Win/Explorer、不重复首次失败。

## Windows focus StageB（独立 focus-01..10，待 CC 执行）

Windows 原生自有 A/B 窗口、EDIT、modal D、原生菜单、最小化/自有恢复按钮位于
`windows/Focus*.cs`。严格入口 `--suite focus`；`--export-focus-cases PATH` 导出独立
`windows-focus-v1` manifest（10例）。所有 self-test/export 分支位于任何 WinForms GUI
初始化之前，不创建窗口、不发输入。**不改变旧 `--export-cases` 63例和
`--export-platform-cases` B2 20例的定义；Mac 不新增 focus。**

- 每例大字 caseID/nonce/指令/首次结果、Check once/Next；第5例在 modal D 内 Check，
  第10例仅 Observe → Check → computer_close，Close 后不点 Finish。业务按钮才可改变
  modal/menu/window 状态，Check 不补焦点或合成输入。
- 只记录已注册的自有 HWND：WndProc 原生消息、queue/own-EDIT 键盘 MSG，raw message/
  wParam/lParam/native 时间与语义字段分离；并记录自身 GetFocus/GetActiveWindow、精确
  文本/selection/Enabled/WindowState。无全局 hook、其他应用读取或人工输入注入。
- `focus-09` 本地正确输入最多 `needs_supervisor`，所有用户窗口未被输入始终 unknown。
  `focus-10` app 最多 `needs_tool_evidence`；独立分析器还要求实际允许工具清单、真实
  `glm-5.3-flash`、观察图元数据/image block、唯一 Check 请求、无盲输轨迹、clean Close、
  完整 CLI 终态及 Close 后心跳。安全停止不算目标有效输入。
- 独立离线分析器 `scripts/analyze-focus-gui.py`，测试源码 `tests/focus_gui_analyzer.py`；
  **只读依赖 `scripts/analyze-basic-input-gui.py` 必须一并复制**。不读图像像素，不修改旧 analyzer。
  按 case/nonce/trial/时间、原生事件组与唯一 call/result 关联，不按第 N 次输入配对；
  缺证据 unknown，首次与最终分开，重试单列，partial 固定以10例为分母。
  因9/10不算有效输入，正常最多8个 `matched`（80%），不是把受限例补成100%。
  `gui_verified=false`、`global_protected_apps=unknown`、十次有效trial门禁 false 不自动升级。
- 新 GLM 视觉任务：`tasks/windows-focus.md`，只8工具、源码隔离、自有窗口边界、首帧授权
  弹窗/缺窗 STOP Close；不绕路/改权限。当前 GUI 授权阻断仍 pending。

CC 在**新 Windows 冻结副本和独占输出目录**用 Framework csc 构建，再运行 self-test、
三个独立 export、旧63 parity、原B2和新focus analyzer测试；不能覆盖旧 GUI snapshot/bin。
实际命令、源文件哈希、compile-only 证据及未验证项见
`.agents/reports/windows-focus-fixture-sol-20260930.md`。实现者不执行任何测试/程序集/GUI；
本机 C#5/Python 编译不能替代 Windows CC 验收。Mac/Linux 测试仍不安排。
