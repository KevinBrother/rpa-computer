# Windows renderer Desktop StageB — independent frozen review (CC+GLM, 2026-09-30)

状态：**REVIEW_ONLY / NO_FIX / NOT_GUI_ACCEPTED**。审阅仅做只读静态核对：未运行任何测试/GUI/构建/SSH/Host，未改任何源码、测试、构建脚本或设置，无 worktree/commit/push。执行方式：**单次 Claude CLI 运行，实际模型 glm-5.3-flash**；本报告为该单一运行产出，并非"两个独立模型交叉审阅"（初版开头表述有误，本版已更正）。

> **更正记录（同日复核后修订，源码零改动）**：① 开头执行方式改为"单一 Claude CLI 运行 glm-5.3-flash"。② R2 原表述（"DPI 在迁移前读取 + 30ms 一帧自愈"）经复核不成立，已改写为真实未知边界，见 §2.3 R2 与 F2c；初版错误文字保留于 R2 段内作为历史。

## 0. 冻结源核验（先于审阅完成）

Mac 侧 `shasum -a 256` 实测 8 个 cs 全部与 `windows-renderer-desktop-sol-20260930.md` §7 冻结表逐字节一致（IPC/JsonValue/Model/Native/Program/Protocol/SelfTests/Windows）。本审阅基于已核对的冻结源。GREEN 报告 `windows-renderer-desktop-green-cc-20260930.md` 的 235 项 native self-test 结果仅作为**纯路径证据**采信，未重跑。

## 1. 审阅输入

- 上述两份报告、`desktop-feedback/windows/{Model,Windows,Program,IPC,Protocol,Native,SelfTests}.cs` 全文逐行。
- `.agents/CONTRACT.md`（§2026-09-30 扩展：可选 feedback 模块、truthful state/cancellation、off-path 不受影响）与 `.agents/plans/implementation.md`（无真实桌面输入、不声称未验证行为）。

## 2. 逐项审阅结论（任务指定的重点面）

### 2.1 严格 Desktop bbox / 负坐标 / 屏间隙 — 无违规
`Model.cs:106-130`：全 long union、精确四分量等于 bbox（无 0.5 容差借用）、正面积 overlap/mirror 拒绝、重复 identity（OrdinalIgnoreCase）拒绝、span 超 int 拒绝；`Model.cs:65-67` `Contains` 为左闭右开，touching-edge 归属右侧屏有测试覆盖（`SelfTests.cs:288-293`）。负原点、纵向、对角、gap 指针均有纯测试。anchor 固定列表首屏、不随 pointer 漂移（`SelfTests.cs:311-312` 验证）。**未发现缺陷。**

### 2.2 native-unit 映射 — 无违规
`Windows.cs:172` 注释与实现一致：Pointer/ScreenFacts/RingCenter 均为 PMv2 virtual-screen 原生像素，无 screenshot 缩放二次应用；ring 以 `GetDpiForWindow(Ring.Handle)` 换算 44px 逻辑尺寸（`Windows.cs:174-178`），有 0-DPI 与溢出门。

### 2.3 DPI 迁移与实际 Stop 边界 — fail-closed，无虚假声称，附 2 个 GUI 未验项
`Windows.cs:122-143`：跨屏移动前先隐藏旧 Bar/Stop（仅 monitor 变更时，**非每 tick 隐藏，无 flicker 断言错误**）；按实际窗口 DPI 计算 layout；赋值 Bounds 后回读实际矩形、Stop HWND DPI、可见性、Button 正尺寸与 client containment，任一不满足抛 `surface_layout_unavailable` 且 `HideFeedback()` 后 rethrow。这是 fail-closed，不做不可操作按钮、不谎报可见。遗留 GUI 未验（非已证 bug）：
- **R1（low，GUI unknown）** `Windows.cs:129` 跨屏迁移只先移 Bar 到目标屏，`Windows.cs:131` 的 Bar DPI 与 `Windows.cs:138` 的 Stop DPI 读取依赖 WM_DPICHANGED 在 SetWindowPos 后同线程同步生效。若 Stop 迁移后 DPI 回读滞后返回旧值，会误抛 `surface_layout_unavailable` → 整个会话 Fail(65)。方向是"误杀"而非"谎报成功"，无安全违规；需真实跨 DPI 显示器迁移 GUI 验证确认。
- **R2（已纠正，改判为 GUI 未验边界，非已证缺陷）** 初版曾断言"`Windows.cs:174` 在 Ring 迁移前读 DPI，故跨屏瞬间按旧屏 DPI 计尺寸、下一 tick 30ms 自愈"。**该表述不成立**：重读实际代码，`Windows.cs:173` 已先执行 `Ring.Location = resolution.RingCenter` 将 ring HWND 迁至新屏，`Windows.cs:174` 的 `GetDpiForWindow` 是在迁移**之后**调用；且 `Program.cs:48` 仅在 `changed`（新快照生效）或距上次 Render ≥1s 时才 Render，不存在"每 30ms 必然重 Render 自愈"的证据——pointer TTL（click 0.65s / 其他 0.35s）可能先到期隐藏 ring。代码顺序上 DPI 读取发生在目标屏迁移后，是正确的写法；**但跨 monitor 时 WM_DPICHANGED/`GetDpiForWindow` 是否在 `Ring.Location` 赋值后同步返回新屏 DPI，属纯静态无法证明的 WinForms/PMv2 运行时行为**，本审阅不能证明 ring 尺寸在新屏上必然正确，也不能证明它必然错误。归入必需 GUI gate（跨 DPI 屏 ring 尺寸实测）。

### 2.4 事件 callback 取消 / Fail / Finish — 无违规
`Program.cs:21-29`：click → `state.RequestStop()`（CanStop 门 + 去重，`Model.cs:9-18`）→ **先** queue Stop 帧**再** Render；渲染异常经 `Fail` → `Error` + `Finish(65)`，不逃逸 click 事件，不会出现"cancel 已排队但进程以 0 退出"的不一致。`Finish`（`Program.cs:59-62`）幂等（`finished` 门）、停 timer、Dispose 窗口、`OutputPump.Finish` 由写线程队列清空后 `Environment.Exit(exitCode)`（`IPC.cs:92-105`）+300ms watchdog（`IPC.cs:87-91`）兜底——不保证 Host 已收帧这一点 sol 报告已如实声明。`Program.cs:22` 的 `finished` 门防 Stop-after-Finish。`OnFormClosing`（`Windows.cs:33-36`）非 Host 关闭转 Stop 请求，Dispose 置 `HostShutdown` 防 close 被取消。**未发现缺陷。**

### 2.5 ShowWithoutActivation / Stop 可用性 — 无违规
`Windows.cs:20-31`：`ShowWithoutActivation=true` + WS_EX_NOACTIVATE + WM_MOUSEACTIVATE 返回 MA_NOACTIVATE（result 3，非 AND_EAT）；Stop HWND（interactive）无 WS_EX_TRANSPARENT 可点击，Bar/Ring 有 TRANSPARENT click-through。Opacity 0.97 的 layered 窗口仍接收点击，Stop 实际可用性属 GUI gate（未验，非已证 bug）。

### 2.6 screenshot affinity 生命周期 — 无违规
`Native.cs:30-53`：一次性 WDA 0x11 设置 + readback 验证 + 失败回滚到 0 + 显式 `failure` 与 `ready.capture_exclusion` 如实区分 requested/unsupported；窗口只移动不重建，WDA 跨 move/monitor 持续有效；无 WDA_MONITOR 部分降级。`RequestedExclusion 不冒充实测 exclusion` 的边界声明与代码一致。首个 tick 前 overlay 已显示但 affinity 未设置的一帧窗口期，属设计顺序（构造必须先有真实 HWND），非违规；GUI gate 应覆盖。

## 3. 具体发现（均为非违规级）

| # | 位置 | 触发/原因 | 严重度 | 最小修复建议 + 回归测试 |
|---|---|---|---|---|
| F1 | `Windows.cs:185`（恒 `return null`）+ `Program.cs:49-51` | Render 的 geometry 失败全部走 throw→Fail 路径（sol §4 明确此设计），`code != null` 的去重分支（`lastGeometryError`）是**不可达死代码** | info/low | 非安全缺陷：错误不会丢失（Fail 路径覆盖）。建议后续清理死分支或在纯测试中加一条"`Render` 在 valid state 下返回 null"的契约测试防将来有人改返回语义后绕过去重。**本轮不改源。** |
| F2 | `Windows.cs:129` | 跨屏迁移仅移动 Bar.Location 预置，Stop 依赖 :134 的 Bounds 赋值迁移；若未来有人把 :138 的 Stop DPI 校验删除，旧屏 Stop 可能短暂可见 | info（当前代码有校验，安全） | 无需修复；GUI gate 中加入跨屏迁移时 Stop 不在旧屏闪现的检查项。 |
| F2c | `Windows.cs:173-178`（R2 更正后的存留边界） | `Ring.Location` 先于 DPI 读取（顺序正确），但迁移后 `GetDpiForWindow` 是否同步返回新屏 DPI 属运行时行为，纯静态无法证明 | info，GUI unknown（非已证 bug） | GUI gate：跨 DPI 屏移动 pointer，实测 ring 尺寸是否匹配新屏 DPI。 |
| F3 | ring 覆盖边缘 | RingCenter 只 clamp 中心点到屏内（`Model.cs:79-82`），ring 半径可越过屏边/gap（`Windows.cs:179-182`） | info，用户可见潜在改进，**非契约违规**（契约要求 ring 仅在合法真实屏点显示——中心点满足） | 若将来要求整环可见，可在 Render 用 `RingCenter` 与屏 bounds 差值收缩 size；需新增纯测试 + GUI 确认。不改。 |

无任何 **concrete correctness/safety regression**（严格 bbox、负坐标、gap、native unit、DPI 迁移虚报、Stop 边界谎报、cancel/Fail/Finish 一致性、affinity 撒谎）在静态审阅中被证明。

## 4. 纯测试盲区（诚实列举，不与"已证 bug"混淆）

235 项 self-test 仅覆盖纯值路径（解析/状态机/resolver/layout 值计算/Style/IPC 缓冲）。以下**未被任何纯测试证明，也未被本轮审阅证伪**，必须 GUI gate：

1. 真实 `Screen.AllScreens`→`ScreenFacts` 适配（OS 保证 DeviceName 非空、WorkingArea⊆Bounds 的假设未在真实多屏/热插拔下验证）→ `Windows.cs:152-156`。
2. `Place()` 真实 HWND 行为：WinForms 最小尺寸/位置 clamp、WM_DPICHANGED 时序（R1）、`Show()` 后真实可见、Button Dock 与 client containment。
3. 真实点击 Stop → `RequestStop` → Host 收 stop 帧 → cleanup 回报的全链路；`OnFormClosing` 用户路径。
4. `RequestExclusion` 真实 WDA 设置/回滚与截图实际排除（代码自身已声明 requested≠proof）。
5. `OutputPump` 队列溢出/watchdog `Environment.Exit` 路径与 `InputPump` 后台线程（仅间接覆盖 BoundedLines/LatestMailbox）。
6. Timer 30ms tick 与真实消息循环的 `ready` 首分支顺序。

## 5. 结论

静态审阅未发现契约违规或安全回归。**能证明的（静态/纯路径）**：resolver/layout/state/协议纯逻辑与冻结源一致；`Ring.Location` 在 DPI 读取之前迁移、Stop/Bar 实际矩形回读校验存在、Stop 先入队后渲染、`Fail`/`Finish` 幂等收尾、affinity requested/unsupported 如实区分。**不能证明的（GUI/运行时）**：跨 DPI 迁移时 `GetDpiForWindow` 在 Location 赋值后是否同步返回新屏 DPI（R2/F2c，真实未知边界，非已证一帧错误——初版"30ms 自愈"说法缺乏依据，Render 仅在 changed 或 ≥1s 时发生）；以及 §4 所列全部 GUI 行为。发现 1 处不可达死代码（F1）与数处非违规边界项（R1/F2c/F3），均不构成本轮阻断。**不声称实现完成、不声称 GUI 验收。** 剩余必需 GUI gates：多屏含负原点/跨 DPI 真实迁移（含 ring 尺寸实测）、ShowWithoutActivation+Stop 真实点击、capture exclusion 实测、热插拔、Host 故障收尾（§4.1–4.6）。
