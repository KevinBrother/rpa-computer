# 分层视觉识别与原生输入验收靶场 — 实现报告（2026-09-30）

Worker: Claude CLI 实现线程（sonnet→GLM 配置）。未启动任何 fixture GUI；未远程部署；
未 commit/push；产品 src/crates/scripts 未触碰。legacy 失败历史未被改动或"洗白"。

## 交付文件

### 新增
| 文件 | 行数 | 说明 |
|---|---|---|
| `acceptance-fixture/macos/Cases.swift` | 283 | 纯逻辑（无 AppKit）：Suite 枚举、四 suite case 目录、`normalizeCRLF`、`utf16ExactMatch`（显式 UTF-16 码元比较，规避 Swift `String ==` 规范等价）、40 项 self-test、export manifest 构造 |
| `acceptance-fixture/windows/Cases.cs` | 420 | 同上 C# 5 版本（in-box csc 兼容，无插值/null 条件/out var/nameof），无 WinForms 依赖 |
| `acceptance-fixture/tools/check-cases-parity.py` | — | 跨平台 parity + 规格检查器：解码两平台源码字符串字面量，逐 UTF-16 比对；校验数量/长度/CRLF/覆盖面；可选与运行时 `--export-cases` manifest 比对 |
| `.agents/runs/layered-fixture-build-20260930/` | — | 构建输出（macOS .app + manifest + 临时 self-test harness；目录名未被占用，未覆盖旧产物） |

### 修改
| 文件 | 说明 |
|---|---|
| `acceptance-fixture/macos/main.swift` | 362→475 行。CLI 新增 `--suite`（未知/缺值→stderr 报错 exit 1，不回落 legacy）、`--self-test`、`--export-cases`（均不创建窗口/不进 NSApplication）；窗口 900×650→900×760；样例 label 26pt（emoji suite 40pt）多行不裁切；suite 流转：case 逐个推进（不回绕），末 case 后 Next 显示 `SUITE COMPLETE — N/N cases` 且不再出新 case；oracle `session/trial/hit/wrong/text_check` 均带 suite/case_id/case_index/case_total（text_check 另有 task_payload/expected_text/expected_utf16_hex/actual_text/actual_utf16_hex）；`matched` 改为显式 UTF-16 逐码元相等 |
| `acceptance-fixture/windows/MainForm.cs` | 407→536 行。同样 CLI 三模式（`Main` 返回 int，`--self-test`/`--export-cases` 无窗口）；ClientSize 884×622→884×712（外框约 900×750，<1080）；样例 24pt，emoji suite 用 `Segoe UI Emoji` 32pt；suite 流转与 SUITE COMPLETE 同 macOS；oracle 字段同上 |
| `acceptance-fixture/build-macos.sh` | swiftc 现同时编译 `main.swift` + `Cases.swift` |
| `acceptance-fixture/build-windows.ps1` | csc 现同时编译 `MainForm.cs` + `Cases.cs` |
| `acceptance-fixture/start-windows.ps1` | 新增 `[ValidateSet('legacy','baseline','punctuation','emoji','known-input')][string]$Suite='legacy'`；仅向 exe 传固定 `--suite <枚举值>`（legacy 不传），无任意命令；run-record.json 记录 suite；session1 身份验证与 stop-windows.ps1 身份守卫均未改动 |
| `acceptance-fixture/README.md` | 重写：三/五条链路表（baseline/punctuation/emoji/known-input/legacy + 合并禁令）、参数说明、oracle 字段表、协调者预检命令、`-Suite` 用法、移除过时样例文字与"expected 23"等旧数字 |

## Case 契约（两平台一致，共 33 case）

- `baseline` ×10：短清晰中文/ASCII/数字，仅 U+0020 分词，UTF-16 长度集 {11,12,13,27,28,29,34,35}，max 35 ≤ 40，无 emoji/符号/特殊空格，互不相同。
- `punctuation` ×6：全/半角括号、中/英文逗号、竖线、引号各一条隔离样例；无 emoji/VS16/非 BMP。
- `emoji` ×6：状态 ✅ / 地球 🌍 / 火箭 🚀 / 微笑 😀 / 铃铛 🔔 / 书本 📚，每条恰一个且互不相同。
- `known-input` ×10：ASCII；中文 12；中文 34；历史 38-scalar 样例（=legacy 样例，UTF-16 39 码元）；全角标点；NBSP（U+00A0）；非 BMP 🦄；组合字符 `resumé 和 café`（无预合成 U+00E9）；CRLF（payload `\r\n` → expected `\n`，manifest/证据中 task_payload 与 expected_text 分开）；Tab。expected 一律 = `normalizeCRLF(payload)`，由 `FixtureCase` 构造器派生，两平台不可能分叉。
- `legacy` ×1（默认模式）：历史样例逐字保留（self-test 中以字面量再次断言）；`case_total=0` 表示无限模式。

## 验证记录（全部本机实际执行）

### 1. 纯逻辑先行（无 GUI）
`xcrun swiftc -O` 编译 `Cases.swift` + 临时 harness（runs 目录内，未入库）→
`SELF-TEST PASSED (40 checks)`，exit 0。40 项含：数量/唯一 ID/nonempty/CRLF 契约/zh12/zh34/38-scalars(=38)/NBSP/组合字符/非 BMP/Tab/legacy 保留/baseline 约束(≤40、纯空格、无符号、含 12&34、互异)/punctuation 无 emoji/emoji 六个已知码点/suite 解析接受+拒绝/UTF-16 比较六语义（同串✓、emoji✓、NFC vs NFD ✗、尾随空格 ✗、CRLF vs LF ✗、NBSP vs 空格 ✗）。

### 2. macOS 完整构建 + 内置 self-test
```
./build-macos.sh ../.agents/runs/layered-fixture-build-20260930/macos
built: .../ComputerUseAcceptance.app
710ac5da8ede627336a012911418e614b039cabc685ab4810376f3cc50b7960c  Contents/MacOS/ComputerUseAcceptance
```
`ComputerUseAcceptance --self-test` → 40/40 PASS，exit 0（无窗口创建）。

### 3. CLI 失败路径（均 stderr 报错 + exit 1）
- `--suite foo` → `error: unknown suite 'foo' (...)`
- `--suite`（缺值）→ `error: --suite requires a value (...)`
- `--suite=punctuation-x` → 同样拒绝

### 4. export-cases
`--export-cases cases-manifest-macos.json` → exit 0，打印 coordinator-only 警告。
抽查：known-09 payload=`\r\n` / expected=`\n`；known-06 含 NBSP；legacy total=1；
suite totals {baseline:10, punctuation:6, emoji:6, known-input:10, legacy:1}。
manifest SHA-256: `20d52cb1...962534`。

### 5. 跨平台 parity（Windows 源码结构检查）
```
python3 tools/check-cases-parity.py cases-manifest-macos.json
PARITY OK: swift == cs at UTF-16 level; spec satisfied; 33 cases across 5 suites
runtime manifest match: OK
```
含防退化篡改测试：改动 C# 中一个 payload 单词后 checker exit 1。同时做了
C#5 兼容结构检查（无 `$""`/`?.`/`out var`/`nameof`/`SampleText` 残留，花括号/圆括号平衡）→ OK。

### 6. 契约不变量
- `matched` 仅当 actual 与 expected 逐 UTF-16 码元相等；无 Trim、无 Unicode 归一化；
  Swift 端显式不用 `String ==`（规范等价会放行 NFC/NFD）。
- oracle 仅写 coordinator 指定 evidence 文件；无 clipboard/OCR/UIA；未自动暴露给 Agent。
- build/start 脚本不覆盖已有产物；start-windows.ps1 仅转发 ValidateSet 固定枚举。

## 哈希清单（SHA-256）

| 文件 | 哈希（前 64 位完整） |
|---|---|
| macos 编译产物 binary | `710ac5da8ede627336a012911418e614b039cabc685ab4810376f3cc50b7960c` |
| cases-manifest-macos.json | `20d52cb12e3b82d57d34b1ffaa79357df97eaf91aefb4681d558610394962534` |
| macos/main.swift | `cfa802d48ccd572f011f5325350e2b586fd6fe4fa78d72685a16c702a8818aca` |
| macos/Cases.swift | `ac62a1db9c7494103588949c88d2743e3dc3ba85520d51aa38b61d1035525121` |
| windows/MainForm.cs | `d4010fbcaa05ea3d5e25a4f43b9ae543e7fd83651e22034211f53f151fbe265e` |
| windows/Cases.cs | `09d1e7253d80a3ee63aa2729ae52843b6c8327be9c565bac42c4187a566e76f2` |

## 未验证项（诚实声明）

> **2026-09-30 更正**：本节原第 1 条"Windows 编译未执行"已发生并已处置——协调者在
> acer-win 真机 csc 首次编译报错（日志 `.agents/runs/layered-win-build-20260930.log`）：
> `Cases.cs(333,17): error CS1593: Delegate 'Action' does not take 2 arguments`。
> 原因：suite-parse 循环里误用 3 参 helper `add` 传 2 参。最小补丁已修（`add(`→`add0(`，
> 仅此一处；已用跨行括号配对脚本复核全部 `add`/`add0` 调用元数一致，无同类残留）。
> **Windows 真 csc 编译/self-test 仍待协调者重跑，在拿到新机编译日志前不假称通过。**
> 补丁后 `windows/Cases.cs` SHA-256：
> `518998c2855db5352db29aeb243d57333bf84256c4bc3e1d615fbc27e2297344`。

1. **Windows 编译与 self-test 未执行**（本机无 Windows/pwsh）。由协调者经 SSH 执行：
   `powershell -ExecutionPolicy Bypass -File build-windows.ps1 -OutDir <new dir>`，然后
   `ComputerUseAcceptance.exe --self-test`（exit code 权威；winexe stdout 仅在重定向时可见）
   与 `--export-cases <path>`，产物 manifest 应与 `cases-manifest-macos.json` 内容一致
   （键序/空白可不同，用 `check-cases-parity.py <win-manifest>` 或 JSON 语义比对）。
   *初次真机编译失败见上方更正。*
2. **PowerShell 脚本语法未机检**（本机无 pwsh），`start-windows.ps1` 的 `-Suite` 仅人工审查；
   首次远端使用时 ValidateSet 绑定行为建议先用一个非法值（如 `-Suite foo`）确认被拒。
3. **GUI 行为未启动验证**（按任务约束不由 worker 启动）：窗口布局（900×760 / 884×712
   内各控件不重叠、样例 26/24pt 38-scalar 换行不裁切、emoji 放大、SUITE COMPLETE 显示、
   键盘输入到 NSTextView/TextBox 后 actual_text 的真实形态）需协调者首次 GUI 会话目检。
4. **CRLF/Tab 在文本 view 的实际行为未实测**：known-09/known-10 按要求保留；
   NSTextView/WinForms TextBox 可能改写换行/Tab 表示，结果将如实出现在
   `actual_utf16_hex`，不得删 case。
5. **Windows emoji 渲染**：Segoe UI Emoji 32pt 下 CJK 前缀依赖 GDI 字体回退，可能单色；
   按要求不伪称截图可唯一判定码点。
6. `stop-windows.ps1` 未改动（身份守卫保持原样），本次未回归测试它。

## 补充静态检查（2026-09-30 补丁轮，全部本机脚本执行，未启动 GUI）

1. **CS1593 修复**：仅 `Cases.cs:333` 一处 2 参调用误用 `add`，改为 `add0`；跨行精确
   arity 复核：`add` 全部 3 参、`add0` 全部 2 参，无其他同类问题；花括号/圆括号平衡
   （48/48、263/263），无 `$""`/`?.`/`out var`/`nameof`（C#5 兼容）。
2. **布局静态检查（几何解析自源码 frame/bounds 字面量）**：
   - macOS 900×760：11 个控件两两无重叠、无越界；WinForms 客户区 884×712 同样通过。
   - 大字号换行估算：26pt（mac）/24pt（win）下最长的 zh34 payload ≈ 2 行
     （62pt/76pt < 144/142 label 高）；known-09 CRLF 分段渲染 ≈ 3 行
     （93pt/114pt < 144/142）——均不会裁切。known-05 全角标点 1 行。
   - Check 结果区（result label 330 宽）与三个按钮水平不重叠，按钮底边
     （mac 56 / win 686）均在窗口内。trial/suite header（Trial i/N — suite: xxx）
     在两平台均位于窗口顶部独立行，不被 nonce/样例遮挡。
   - **实际可读性（字形渲染、真实换行点、主题对比度）留协调者截图目检**，本机仅静态估算。
3. **macOS 回归**（新唯一目录，未覆盖旧构建）：
   `./build-macos.sh ../.agents/runs/layered-fixture-build-20260930-b/macos` →
   binary SHA-256 `710ac5da…b7960c`（与上轮一致，macOS 侧零改动）；self-test 40/40 PASS；
   `--export-cases` + `check-cases-parity.py` → PARITY OK（33 cases，runtime manifest 匹配）。
4. case 目录与 suite 覆盖**零改动**（本轮仅改 helper 名）。

## 遗留约定

- legacy 与新 suite 结果禁止合并统计；旧失败数据不洗白（README 已明示）。
- `--export-cases` 产物与 evidence 文件不得进入 GUI Agent 发布目录（README 已明示）。
- 后续改任一平台 case 后必须跑 `tools/check-cases-parity.py`。

## 第三轮（2026-09-30）：macOS 可见性修复 + 输入域修正

### 1. 只读探针（`.agents/runs/layered-fixture-mac-window-probe-20260930.py`）

仅调用非变更 CoreGraphics API（ctypes）：`CGMainDisplayID`、online/active display
list、`CGDisplayBounds`、`CGWindowListCopyWindowInfo`（只读 bounds/onScreen/layer，
不收集窗口标题）。结果（`.agents/runs/layered-fixture-mac-window-probe-20260930.json`）：

- `main_display_id = 2`，display 2 bounds `[0, 0, 1920, 1080]`（main/active/online）；
  display 1 bounds `[1920, 0, 1920, 1080]`（副屏，active/online）。
- fixture 主窗口（PID 66565，win 21468，900×792，on_screen=true，layer 0）bounds
  `[2430, 95, 900, 792]` —— 完全落在 display 1（x≥1920）上，不在主屏。
- backdrop PID 66608 两块全屏窗分别覆盖 x=0 与 x=1920，正常。

**根因确认**：`window.center()` 在多屏下居中到了非主 display，而 Host 截图只截
`CGMainDisplayID=2`，故截图只见 backdrop 空白。本轮未激活/移动任何现有窗口；
只有协调者会重启修复后的 fixture。

### 2. 修复（仅初始定位 + 输入域，case 目录零改动）

- `macos/Cases.swift`（纯逻辑）：新增 `ScreenInfo` / `ScreenPlacement.chooseFrame`
  （按 displayID 匹配主屏并居中；主屏缺失或窗口放不下 → 返回 nil，调用方必须
  loud fail，不静默回落、不裁切）与 `fullyInside`。新增 6 项 self-test：
  主屏不在列表首位（x=510,y=144）、负 origin 副屏、水平横排副屏（x=2430，复现
  本次 bug 几何）、垂直负 Y、主屏缺失 → nil、主屏过小 → nil。总数 40 → **46**。
- `macos/main.swift`：`window.center()` 替换为 `placeWindowOnMainDisplay()`——
  用 `CGMainDisplayID` 匹配 `NSScreen.deviceDescription` 的 `NSScreenNumber`，
  在该 screen.frame（AppKit 坐标，不手动换算 Y）内居中；找不到匹配/放不下时
  `Config.fail` 列出所有屏 geometry 并 exit 1，绝不无声地开到不可见位置。
- `macos/main.swift` NSTextView：显式禁用 `isAutomaticQuoteSubstitutionEnabled /
  isAutomaticDashSubstitutionEnabled / isAutomaticTextReplacementEnabled /
  isAutomaticSpellingCorrectionEnabled / isContinuousSpellCheckingEnabled` = false。
  注：`automaticGrammarCheckingEnabled` 不是 NSTextView 成员（编译器确认），已去掉
  该行；组合字符 case 的防护由禁用拼写纠正 + 严格 UTF-16 比较保证。
  实际 CRLF 差异只记录不偷偷 normalize ActualText，`matched` 保持严格逐码元比较。
- `windows/MainForm.cs`：TextBox 增加 `AcceptsTab = true` 与 `AcceptsReturn = true`
  （否则 Tab/Enter 触发焦点/默认按钮而非写入文本）。

### 3. 重建与验证（唯一目录，未覆盖旧产物）

```
./build-macos.sh ../.agents/runs/layered-fixture-build-20260930-c/macos
<binary> --self-test                     → PASS 6/6 placement, SELF-TEST PASSED (46 checks), exit 0
<binary> --export-cases .../cases-manifest-macos.json → exit 0
python3 tools/check-cases-parity.py .../cases-manifest-macos.json
  → PARITY OK: swift == cs at UTF-16 level; spec satisfied; 33 cases; runtime manifest match: OK
```

首次构建因 Swift 3 属性改名（`automatic...Enabled` → `isAutomatic...Enabled`）失败，
按编译器提示改名后通过；该空 .app 骨架为本轮失败构建遗留，已删除重建。

哈希（SHA-256）：

| 文件 | 哈希 |
|---|---|
| macos binary（-c 构建） | `eff0aae563ed060cd6cd79bf6b2c02bd88b38e0f1cf782cac36ae89471727c0f` |
| macos/main.swift | `d37e0776eef204b29d01b37a456ced9a4f5d770a5cb1f683308ed923957fb50f` |
| macos/Cases.swift | `ddeac570dcb252acf6c3870eb6ff176cc9c38d71586322acba90ab871c17a41a` |
| windows/MainForm.cs | `692818d11df112bfef3d88259064d1a4a4917c17eeb3704a90555746ec1f59a9` |
| cases-manifest-macos.json | `20d52cb12e3b82d57d34b1ffaa79357df97eaf91aefb4681d558610394962534`（与首轮一致，case 数据零改动） |

### 4. 未验证项（诚实声明）

- **实际可见性待协调者截图确认**：修复只基于探针几何证据 + 单元级 placement
  测试；重启后的 fixture 是否出现在 `CGMainDisplayID=2` 截图中，需协调者目检。
  本轮不声称完整验收通过。
- `windows/MainForm.cs` 已改（AcceptsTab/AcceptsReturn），需在 acer-win 重新编译；
  现有 out-fixed 哈希 `77FABB90…` 早于本次改动。Cases.cs 的 CS1593 修复
  （`518998c2…`）协调者已真机编译通过，本轮未再改 Cases.cs。
- NSTextView 自动替换在真实键盘注入路径中的行为未实测（不启动 GUI）。

## 第三轮补充（2026-09-30）：Windows 真 csc self-test 3 项测试逻辑错误修复

协调者反馈：out-fixed 构建（CS1593 已修后）真机 `--self-test` exit 1，3/40 失败
（全文 `.agents/runs/layered-win-selftest-full-20260930.log`）。三处均为 **C# 测试
逻辑错误**，case 目录零改动：

1. `non-crlf-payload-equals-expected`：谓词把"含 CRLF"写反（要求 CRLF case 也
   payload==expected）。改为 `c.Payload.Contains("\r\n") || c.Payload == c.ExpectedText`，
   并补 `crlf-equality-counterexample` 断言（known-09 是唯一允许 payload≠expected 的
   反例，其余全部相等）。
2. `known-legacy-38-scalars`：原 `Count(ch in D800..DFFF)` 把高低代理都计数，39 码元
   减 2 得 37。改为只数高代理（合法 surrogate pair 数），并加 `known-legacy-bmp-nonbmp-mix`
   回归（legacy 样例同时含 BMP ✅ U+2705 与非 BMP 🌍 U+1F30D、恰 1 对代理）。
3. `punctuation-no-emoji`：原谓词 `ch < 0xD800 && ch != 0xFE0F` 对全角标点/emoji
   均无实际约束力。新增 `HasEmojiScalar`（非 BMP 代理 / U+FE0F / U+2600–U+27BF 为
   emoji；U+FF00–U+FFEF 全角、U+3000–U+303F、U+2018–U+201D 明确不算），逐 case 检查
   并报告 offender；补 `punctuation-no-emoji-detector-not-vacuous`（检测器能识别
   ✅/🌍/VS16，同时放行全角标点与普通文本，防恒真）。

**意外事故与恢复（诚实声明）**：修复过程中一个临时转义脚本因 `str.index` 命中文件
前部的 `"code (alpha) beta"`（punctuation-02 payload）导致 Cases.cs 被拼接损坏；
已用逆运算精确重建（自检：33 case 单份、SelfTest 单份、花括号 50/50、圆括号 296/296、
add/add0 全量 arity 扫描无错配、无 `$""`/`?.`/`out var`/`nameof`）。emoji/全角字面量
已全部转为显式 `\uXXXX` 转义。

**本机真实验证（Mono，非推测）**：本机有 Mono，直接复现了 C# 编译+自检——
`mcs -target:winexe ... MainForm.cs Cases.cs`（exit 0）+ `mono ... --self-test` →
**SELF-TEST PASSED (43 checks)**，exit 0（40 原有 − 3 修复 + 3 新增断言）；三项修复
检查与新增三项全部 PASS。`--export-cases` 经 mono 运行后
`check-cases-parity.py` → PARITY OK（33 cases，runtime manifest 匹配）。

修复后 `windows/Cases.cs` SHA-256：
`6bf1a8e1b33016f5dddde78a7350550b82914b8753dabd475bc217d40af44909`。
**仍待协调者真机 csc 重编译 + self-test + export parity**（Mono 验证不能替代
in-box csc；新版本 artifact 不覆盖旧错误证据）。MainForm.cs 的
AcceptsTab/AcceptsReturn 改动仍需一并重编译。

## 第四轮（2026-09-30）：macOS `--coordinator-raise`（coord 专用环境准备）

背景：窗口已正确落在主屏 bounds [510,144,900,792]，但 opaque backdrop 盖在其上
（probe `layered-backdrop-20260930/probe-fixed.json`：bg index 1 vs fixture 18）；
App activate / activateAllWindows 及 helper 自激活+3s 重试均未 raise，真实 GLM
截图只见菜单误判可见。修复方向：让 fixture 自己 orderFront，而不是依赖系统
activation 行为。

### 实现（仅 `macos/main.swift`，Windows/输入库/Host 零改动）

- `Config` 新增 `--coordinator-raise` 开关；未知参数改为 **fail-closed**
  （`unknown argument '<arg>'` + exit 1。注：此前未知参数被静默忽略，本轮按
  要求收紧为显式拒绝）。
- 仅 GUI 路径：`applicationDidFinishLaunching` 末尾若开关打开，`signal(SIGUSR1, SIG_IGN)`
  + `DispatchSource.makeSignalSource(signal: SIGUSR1, queue: .main)`，AppDelegate
  持强引用 `coordinatorRaiseSource`。收到信号仅对本应用单个 owned window 执行
  `makeKeyAndOrderFront` + `orderFrontRegardless` + `NSApp.activate`，evidence 记
  `coordinator_raise` 事件（仅 pid/raised，无文本答案）。不触碰其他 app/窗口、
  无 AX/AppleScript/按键、无键鼠注入、无 always-floating 层级。
- 安装完成后 stdout 打印 `COORDINATOR_RAISE_READY=1 PID=<pid>`（fflush）。
  开关未给时不注册、不打印 ready；`--self-test`/`--export-cases` 仍提前 exit 无 GUI。

### 构建/验证（唯一目录 `-d`，未覆盖旧产物）

```
./build-macos.sh ../.agents/runs/layered-fixture-build-20260930-d/macos
<bin> --self-test                       → SELF-TEST PASSED (46 checks), exit 0
<bin> --self-test --coordinator-raise   → SELF-TEST PASSED (46 checks)（提前退出，无 ready 行，无 GUI）
<bin> --frobnicate                      → error: unknown argument '--frobnicate', exit 1
<bin> --export-cases ...-d/cases-manifest-macos.json → exit 0
python3 tools/check-cases-parity.py     → PARITY OK（33 cases，runtime manifest 匹配）
```

哈希（SHA-256）：

| 文件 | 哈希 |
|---|---|
| macos binary（-d 构建） | `3eca09ef9c835c2021915f750b1b4cf9f4dd89ff74abef86b402dde6a827bff4` |
| macos/main.swift | `d32abf888846a839388db4c4d8cce94264bee416be78526e5eb50b6e571c2a0b` |
| macos/Cases.swift | `ddeac570dcb252acf6c3870eb6ff176cc9c38d71586322acba90ab871c17a41a`（未改动） |
| cases-manifest-macos.json | `20d52cb1…962534`（与此前一致，case 数据零改动） |

### 未验证项（诚实声明）

- **SIGUSR1 → raise 的实际效果待协调者验证**：流程 = coord 启动带
  `--coordinator-raise` 的新 `-d` 产物 → 核对 stdout `COORDINATOR_RAISE_READY=1 PID=<pid>`
  与产物/PID/exe 身份 → 向该 PID 发 SIGUSR1 → GLM 截图核对窗口出现在主屏。
  worker 未启动 GUI、未发 SIGUSR1（本轮 worker 侧一条测试命令曾误带
  `--coordinator-raise` 启动了 GUI 进程，已立即 kill 清理，无信号发送、无窗口操作；
  此后所有验证均为无 GUI 的 CLI 路径）。
- README 已补：此功能为**协调者环境准备，不属于 Agent 动作**；GUI Agent 不得
  经 shell 调用该开关（Agent 工具/文件隔离边界不变）。
