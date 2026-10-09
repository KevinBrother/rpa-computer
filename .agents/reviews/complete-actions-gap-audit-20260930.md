# 只读动作完整性审计（设计阶段，未实施）— 2026-09-30

审查者：独立 CLI 审查。全程未修改产品源码/测试/脚本/构建配置，未运行 GUI/键鼠/截屏，未连接 Windows。本报告只写本文件。

术语约定：**【已证实】** = 直接读到当前代码/报告原文；**【真机确认】** = 代码路径成立但行为需真实 GUI 验证。

---

## 1. click count 1/2/3 从 Action → Plan/InputEvent → native driver 的语义保持

### 1.1 已证实链路

| 层 | 位置 | 结论 |
|---|---|---|
| 协议解析 | `src/runtime/actions.rs:163-185`，`CLICK_MAX_COUNT=3`（:19） | count 1..=3，0/4 拒绝（测试 `click_count_bounds` :470） |
| 计划 | `src/runtime/plan.rs:152-175` | count=N 发出 N 对 Press/Release，对间 `Sleep(MULTI_CLICK_INTERVAL_MS=90)`（actions.rs:24）；hold 窗口正确覆盖（测试 `click_count_marks_hold_window` plan.rs:459） |
| 中间调度 | `src/backend/dispatch.rs:78-87` | Button Press/Release 原样透传，**无 click-count 字段**——count 语义退化为"时间上相邻的 N 次单击" |
| macOS driver | `crates/native-input/src/macos/mod.rs:218-228` | `post_mouse_event` 对**所有**鼠标事件硬编码 `CGEventSetIntegerValueField(event, FIELD_MOUSE_CLICK_STATE, 1)` |
| Windows driver | `crates/native-input/src/windows/builder.rs:139-160` | press/up 仅 flags，dx=dy=0、mouse_data=0，无 click count（Windows 语义上本就依赖系统时序判定） |

### 1.2 缺口与风险

- **P0【已证实代码缺口】macOS count=2/3 失效**：macOS 识别双/三击要求 `clickState` 递增（2、3）。当前每次 press clickState 恒为 1，App 收到的是两次独立单击——连击选词、双击打开等语义丢失。缺口只在 native-input macos 层；runtime plan 层已经正确产生 press/release 对。
- **P1【真机确认】Windows count=2/3 是否被识别**：SendInput 相邻 LEFTDOWN/LEFTUP（dx=dy=0、间隔 90ms < 系统 DoubleClickTime ~500ms、坐标不变）理论上由 OS 判定为双击，但报告已知 SendInput 返回数 ≠ 应用处理成功（`.agents/reports/native-input-windows-20260929.md` "Not verified" 段）。无任何 GUI 证据。
- **P2【真机确认】count>1 时不重新 Move**：plan.rs:158-166 只在开头 Move 一次，第二次 press 前无 Move。macOS press 取 `cursor_point()`（macos/mod.rs:306）、Windows press dx=dy=0，都依赖光标未被外部移动。真实桌面有人为移动时第二次 press 可能偏移。影响小但设计上应知情。
- count=1 语义两平台均正确【已证实】。

### 1.3 最小 native-input 抽象设计（不保留假支持）

原则：不做"count 参数被 driver 忽略"的假接口。建议二选一：

- **方案 A（推荐，最小面）**：在 `Driver` trait（`crates/native-input/src/types.rs:178` 附近）将 `button` 保持不变，**新增** `click(button, count: u8)`（或 `button` 增带默认实现的 `click_state: u8` 变体），语义契约：driver 负责在时间窗内发出 clickState=1..count 的 macOS press 序列；Windows 实现退化为现有相邻 down/up（系统判定）。`BackendCore`（dispatch_core）把 Plan 的 count>1 压缩为一次 `click` 调用；count=1 走现有 press/release 路径不动。
- **方案 B**：driver 内部维护 last-press 时间/位置计数器，press 时自动递增 clickState，release/超时重置。侵入 driver 状态机，跨 release_all/Drop 语义要重新证明，且会污染"用户自己手动点击"的计数。

方案 A 只动 `types.rs`（trait 增量）、`macos/mod.rs`（clickState 序列 + 单元测试可注入 fake source 断言 clickState 字段）、`windows.rs/builder.rs`（降级实现 + 纯测试）、`dispatch_core.rs`（压缩逻辑）。**同时删除/禁止任何把 count>1 静默当作 count=1 的路径**。旧 fake 支持不留：`NativeInput::button` 不接收 count，click 语义在 trait 上显式存在。

Linux/X11（`crates/native-input/src/linux_x11.rs`）同步加 no-op/正确实现，避免 trait 变更破坏编译。

---

## 2. drag / scroll / key_hold / chord

### 2.1 drag

- Plan 完整【已证实】：`plan.rs:179-203` Move 起点 → Press → 按弧长插值的 Move 串（`drag_interpolate` :275，端点精确命中、单调性有测试 :323-331）→ Release；hold 窗口覆盖整段。
- macOS 拖动事件类型【已证实】：`macos/mod.rs:37-47` `move_event_type` 在左/中/右键按下时发 `*_MOUSE_DRAGGED`，否则 `MOUSE_MOVED`；held 位由 `button()` 维护（:309-311）。
- Windows【真机确认】：无独立 dragged 事件类型，按住期间的 MOVE|ABSOLUTE 由系统表现为 drag——代码正确，行为未验证。
- 失败清理【已证实，纯测试】：release 失败保留 held 并可重试（dispatch.rs 测试 :317-359）；release_all 逐项尝试、失败项保留（:362-413）；Drop 兜底释放（:595-617）；executor 只对"实际注入过的 press"发应答 release（execute.rs:282-311 + plan.rs:97-114）。
- **P2【真机确认】**：拖动中间 Sleep 被截止/取消后 release pass 补发的 UP 是否让目标 App 完整结束拖拽，无 GUI 证据。

### 2.2 scroll

- 符号契约【已证实】：dispatch 层不取反（dispatch.rs:6-8、测试 :236-258）；macOS `scroll_params` 正=下/右取负 wheel（macos/mod.rs:69-74，测试 :396-402）；Windows vertical 取负 delta、horizontal 不取反（builder.rs:199-219，测试 :542-571）。三处一致。
- 单位【已证实，行为差异保留】：macOS `CG_SCROLL_EVENT_UNIT_LINE`、每 tick 1 wheel 单位；Windows `WHEEL_DELTA=120`/tick。同 tick 数两平台滚动量不同——不是 bug，但 GUI 用例不能跨平台共用手感断言。
- 溢出/边界【已证实】：±100 ticks 协议层拒绝（actions.rs:237-243）；Windows `checked_mul` + `i32::MAX` 测试（builder.rs:565-571）；零 tick 两平台 noop Ok（macos/mod.rs:342-346、windows.rs:149-153），双零在 action 层已拒（actions.rs:234-236）。超大输入会被清理边界前拒绝，无注入残留。
- 定位【已证实】：scroll 事件本身无坐标，Plan 先 Move 后 Scroll（plan.rs:204-214），滚动发生在目标位置。

### 2.3 key_hold / chord

- Plan【已证实】：chord 修饰键 press→key press/release→修饰键逆序 release（plan.rs:228-251）；hold press→Sleep→release（:252-264）。
- 取消/截止不遗留【已证实，纯测试】：hold 内 Sleep 截短→报 partial + release pass 补 UP（execute.rs:138-162、测试 `deadline_truncates_max_hold_and_reports_partial` :398）；mid-text 取消→partial+release_all（:352）；cleanup 失败如实报（:454）；Drop 兜底（dispatch.rs:595）。
- **P1【真机确认】**：真实按住 5s（HOLD_MAX）与中途取消的 GUI 行为零证据；修饰键 flag 累积/清除（macos/mod.rs:89-100）纯测试通过但真机 chord（如 Ctrl+Shift+S）未跑过。

---

## 3. 多屏：capture target / input 坐标 / DPI / geometry freshness

### 3.1 已实现【均已证实】

- 枚举与选屏：`src/backend/capture.rs:63-89` `target_screen()` 枚举全部显示器并**只选 `is_primary`**；每次 geometry()/capture() 重新枚举（display 变更即时可见）。
- DPI：macOS 预期像素 = 逻辑点 × backing scale（capture.rs:95-100 + macos.rs 交叉校验 :119-125）；Windows PMv2 派生公式与 screenshots 0.8.10 一致（capture.rs:19-33 文档 + 测试 :224）。Windows 进程 DPI 感知由 `prepare_thread` 强制（desktop.rs:46）。
- 坐标映射：`CoordMap::to_native`（image.rs:130-137）image 像素 → native，含 `input_origin` 偏移，天然支持负 origin 副屏坐标；Windows SendInput 绝对坐标覆盖虚拟桌面含负 origin、clamp 单调性纯测试（builder.rs:463-508，三屏布局）。
- freshness：geometry 版本串含 display id/origin/size/capture 尺寸/rotation（capture.rs:123-140）；会话两阶段检查（capture 前 `geometry()`、capture 后 `capture.geometry`），不匹配 fail-fast 不静默重绑（desktop.rs:107-149 策略 + :236-321 测试）。

### 3.2 未实现 / 缺口

- **P1【已证实】无 target display 选择 API**：`target_screen()` 硬编码 primary，`Backend` trait（backend/mod.rs）无按 display 捕获/注入的入口。次屏"可截图、可点击"的能力不存在。
- **P1【已证实】副屏零 GUI 证据**：Mac 报告记录两屏休眠/inactive、fixture 落在 x=2430 次屏造成预检失败（`.agents/reports/layered-gui-validation-20260930.md` 多处）；这恰说明当前连"次屏可见性"都未验收。主屏不同 DPI/缩放测试**不能**当作多屏完整证据。
- P2【已证实】：`is_primary` 依赖 screenshots crate 枚举顺序；多屏同规格时主屏判定无法从代码层证明（依赖 crate 行为）。

---

## 4. 测试证据盘点：纯测试 vs 真实 GUI

### 4.1 纯测试已覆盖（不可冒充 GUI）

- runtime：action 解析/边界、plan 编译/hold 窗口/插值、executor 截止/取消/清理（execute.rs、plan.rs、actions.rs 各内嵌测试）。
- dispatch：held 状态机、scroll 透传、失败保留/重试、Drop 清理、错误码透传（dispatch.rs:114-707）。
- native-input：macOS 35 passed（`.agents/reports/native-input-mac-final-20260929.md`，纯测试、无注入）；Windows builder 23 + windows.rs INPUT 构造测试（仅类型检查于 msvc target，未在真机跑全套）。
- capture/geometry：版本串、编码尺寸、Windows/macOS 派生公式守卫（capture.rs 测试）。

### 4.2 真实 GUI 证据现状（引用原始报告，不改写不洗白）

来自 `.agents/reports/layered-gui-validation-20260930.md`（glm-5.3-flash）：

- **Windows 四 suite 通过率（原样保留）**：baseline **10/10**（first+final exact、三方 UTF-16 一致，但 backdrop 计时器 5 分钟到期后露出 owned terminal，**不能宣称全程中性桌面隔离**）；known-input 精确 **9/10**（known09 CRLF：任务/模型/应用均为 CRLF，checker expected 为 LF，保守标 input_divergence 未改判）；punctuation **5/6**（模型参数漏 4 空格）；emoji **5/6**（模型参数勾选符号错误；且 WinForms 单色渲染本就无法唯一判码点）。旧 2026-09-29 复杂样例 **0/9 失败证据保留**。OS 文件隔离欠缺、前两 suite 背景露出均保留，**不宣称完整黑盒门禁通过**。
- **Mac 未完成项（原样保留，截至该报告）**：四 suite（baseline/known-input/punctuation/emoji）全部未完成；Calculator 3 次完整运行未完成（首测 480s 超时 rc124，只算部分证据）；两屏 inactive/休眠期间 `-e` header 实拍验证未完成；Mac 绘制 clip 修复后仍需真机截图验收（离屏自测 49/49 ≠ 根因确认）。

### 4.3 建议新增：确定性 fixture vs 真实应用用例（明确区分）

需在 acceptance-fixture 内**新增确定性 fixture**（随机 nonce、逐 UTF-16 oracle，延续现有框架）：

1. **multi-click fixture**：显示"双击计数/三击计数"面板（WinForms `Click` 事件天然聚合、macOS `mouseDownWithTheEvent clickCount`），oracle 记录 App 感知的 clickCount——这是验证 §1 缺口的唯一可信证据，纯测试无法替代。
2. **drag fixture**：起点/终点命中区 + 拖动路径采样区，oracle 记录按住期间收到的 dragged/move 事件序列与释放位置。
3. **scroll fixture**：双轴滚动条位置读数，oracle 记录 delta 符号与幅度。
4. **key_hold fixture**：按住时长上/下限指示 + 取消后残留修饰键状态自检。

真实应用用例（不可用 fixture 冒充、也不可互相替代）：Notepad 双击选词（Windows 真机确认项 §1.2）、Finder/资源管理器框选拖拽、浏览器双轴滚动、真实 App 内长按行为、Calculator 补完 3 次（未完成项）。每条用例在报告中标注"GUI 证据"或"纯测试"，禁止混计。

---

## 5. 实施写入分工（避免同文件并行写与 1000+ 行大文件）

| 工作包 | 独占文件范围 | 内容 |
|---|---|---|
| **A. native-input** | `crates/native-input/src/types.rs`、`macos/mod.rs`、`windows.rs`、`windows/builder.rs`、`linux_x11.rs`（若 click 语义需同步） | §1 方案 A 的 `click` trait 增量 + mac clickState + Windows 降级实现 + 各自纯测试。types.rs 是跨包契约，**A 先单独一轮落地 trait 变更并通过全部平台 `cargo check`，B/C 才并行**。禁止单人同轮改 macos 与 windows 两个 driver 文件之外的层。 |
| **B. runtime/backend geometry** | `src/backend/capture.rs`、`src/backend/desktop.rs`、`src/backend/mod.rs`、`src/runtime/session.rs`（仅 geometry/target 相关函数） | target display 选择参数化：`TargetScreen` 增 display 选择、`Backend` trait 增按屏 capture/geometry、session 绑定扩展。**不碰** plan/execute/actions。B 与 A 无文件交集（A 在 crates/，B 在 src/backend+runtime），可并行；但 `Backend` trait 变更需在 A 的 `Driver` trait 落地后 rebase 一次。 |
| **C. fixture** | `acceptance-fixture/macos/*.swift`、`acceptance-fixture/windows/*.cs`、两个 build 脚本 | §4.3 的 4 个新 fixture + case 目录扩展。与 A/B 零交集，可全程并行。 |

防大文件规则：每包新增逻辑放**新模块文件**（如 `crates/native-input/src/macos/click.rs`、`src/backend/multi_display.rs`、`acceptance-fixture/macos/Suites.swift`），现有文件只加少量接线；单文件增量 ≤300 行。协调门禁：A 的 trait 变更 PR 合入前，B/C 只做纯准备不合并。

---

## 6. 测试门禁建议（实施前必须满足）

1. macOS click：fixture multi-click oracle 显示 clickCount=2/3，纯测试断言 `CGEventSetIntegerValueField(clickState)` 序列（fake source 注入）。
2. Windows count：Notepad 双击选词 GUI 证据 ≥1 次；SendInput 计数不作为通过依据（沿用原报告既定边界）。
3. 多屏：双屏 fixture 各自 capture+点击命中，且 geometry 版本串在切换 target 时变化；次屏用例不得在休眠屏上执行。
4. 回归门禁：现有纯测试全绿 + `cargo check --target x86_64-pc-windows-msvc --all-targets`（原报告的 Rc/RefCell 阻塞项须先清）。
5. 旧证据纪律：Windows 0/9、四 suite 已知失败数、Mac 未完成项全部原样保留在任何新汇总中，禁止洗白或合并统计。
