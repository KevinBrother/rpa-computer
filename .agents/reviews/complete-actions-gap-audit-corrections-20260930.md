# 只读动作完整性审计 — 纠错报告（2026-09-30）

本报告针对 `.agents/reviews/complete-actions-gap-audit-20260930.md`（下称"原报告"）做准确性纠错。全程只读核对源码、既有报告与设计/用例文档，未修改任何源码/测试/脚本/用户设置，未运行 GUI/键鼠/截屏，未 commit/push。原报告保留不动。

标注约定：
- **【代码事实】** = 直接读到当前源码或文档原文；
- **【报告事实】** = 引自 `.agents/reports/layered-gui-validation-20260930.md`（下称"GUI验证报告"）原文，未做二次推断；
- **【推测/待证】** = 逻辑成立但无 GUI 证据，不得当作结论。

---

## 1. 重大纠错：GUI 进展引用过时

原报告 §4.2 称"Mac 四 suite 全部未完成、`-e` header 实拍验证未完成"。这是 GUI验证报告**中段**的快照，不是该报告最终状态。以 GUI 验证报告**后半段**为准：

### 1.1 Mac baseline 已完成（纠错为已验证）
- **【报告事实】** Mac `-e2` 真实 GLM run 正常 exit0，8 tools/67 calls/69 turns/1017168ms，17 项工具审计通过；10 个普通文字 case 首次及最终 raw UTF-16 exact **10/10**，应用 HIT **9/10**、WRONG 0，unresolved 0；最终截图 SUITE COMPLETE 10/10/MATCHED 由协调者实看。证据 prefix `layered-mac-baseline-e2-final-*`。
- **【报告事实】** `-e2` preflight 实拍截图 `layered-mac-baseline-e2-preflight-frame-20260930.png` 显示 caseID/轮次/nonce/target/header 完整——"`-e` header 未验证"的说法已过时，`-e2` 绘制修复有真机图像证据。
- **【报告事实】** case01 目标点击无 HIT/WRONG 事件（click 参数 [1277,473] 在绿色圆内但无 HIT），保留为未验证目标点击，文本通过不倒推 HIT。现象符合 NSView 未 override `acceptsFirstMouse` 的首次窗口激活 click-through 假设，根因未证明。
- **仍待验证（不纠错，维持原状）**：Mac known-input 10、punctuation 6、emoji 6、Calculator 完整 3 次。`-f` first-mouse 修复 API 回归 50/50 纯测试通过（SHA `5823306b...`），但 **GUI 未验收**；后续预检 no_display/rc4，known-input 未启动。
- **【报告事实】** GUI 验证报告结尾状态：Mac 两屏 asleep/inactive、active_count 0，目标标 **blocked** 而非 complete。

### 1.2 Windows baseline "5 分钟 helper 到期"归因核查（原报告此处并非张冠李戴，但需补全）
- **【报告事实】** Windows baseline run 实际时长 901248ms ≈ **15 分钟**，而旧 backdrop 自身 timer 仅 5 分钟（ScheduledTask expiry 25 分钟），最终帧背景露出 owned coordinator terminal。**时长与 5 分钟 timer 的矛盾是 baseline 自身证据内的**，原报告把这一条写在 baseline 段落是正确的，不是把别的 suite 的问题错安到 baseline。
- **但需补全（纠错归因表述）**：GUI 验证报告后来明确——known-input 第 7 例在**新 25 分钟 backdrop 仍在运行时**仍可见 owned 启动 terminal，说明背景露出**不只是** timer 到期单一原因：后启动的 agent powershell console 也会覆盖 normal backdrop。因此任何引用"Windows baseline 背景=helper 5 分钟到期"的表述应写成"旧 5 分钟 timer + agent console 覆盖，多因素"；punctuation 那轮 hidden launcher 实拍背景中性，不反向洗白之前的 runs。

### 1.3 Windows 四 suite 最终状态（与原报告一致，维持）
【报告事实】baseline 10/10、known-input 9/10（known09 CRLF/LF 预期差异，归因靶场跨平台换行预期与 Windows 控件表示不一致，已出独立诊断报告）、punctuation 5/6（模型参数丢 4 空格）、emoji 5/6（模型参数 U+2611 vs U+2705）。OS 文件隔离欠缺保留，不宣称完整黑盒门禁。

---

## 2. macOS click_state 固定 1：代码缺口成立，影响范围不得夸大

- **【代码事实】** `crates/native-input/src/macos/mod.rs:223`：对所有鼠标事件硬编码 `CGEventSetIntegerValueField(event.0, ffi::FIELD_MOUSE_CLICK_STATE, 1)`。runtime plan 层已正确产生 count 对应的 press/release 对（`src/runtime/plan.rs` Click 分支，~148-180 行，对间 `Sleep(MULTI_CLICK_INTERVAL_MS=90)`，`actions.rs:24`）。
- **正确结论（P0）**：driver 无法携带多击元数据，macOS 原生多击语义（NSEvent.clickCount 递增）**由本代码路径不能保证**。这是明确的代码缺口。
- **纠错（不得断言）**：原报告直接写"count=2/3 失效""连击选词、双击打开等语义丢失"。实际上：部分应用/框架会自行按时间与位置聚合相邻 mouseDown 判定双击，此时即使 clickState=1 也可能表现为双击；另一部分依赖 NSEvent.clickCount 字段则会失败。**没有 GUI 证据前不能断言所有 macOS 应用实际事件 clickCount 均为 1、双击一定失效**——正确表述是"无法保证多击语义，具体失效范围待 fixture（检查 NSEvent.clickCount）真机验证"。用例规范 `docs/computer-use-acceptance-cases.md` multiclick-01..10 正是为此设计（Mac 直接检查 NSEvent.clickCount，"不能用'鼠标事件发了两次'替代"）。

---

## 3. 方案纠错：不接受把多击压成 driver.click(count) 耗时循环

原报告 §1.3 方案 A 建议 `BackendCore` 把 count>1 压缩为一次 `driver.click(button, count)` 调用，由 driver 在时间窗内发出 clickState 序列。**本纠错报告不接受该方案**，理由：

1. **破坏可取消性**：多击对间的 90ms 睡眠目前在 runtime 侧是 `PlanEvent::Sleep`，享受 executor 的 deadline 截止/取消/清理路径（GUI 证据链里的 partial+release 机制依赖它）。压缩进 driver 后，取消/睡眠调度被藏进 native driver，runtime 不再掌管时序，§2.3 的取消清理证据模型全部要重证。
2. **破坏提取库边界**：native-input 定位是"原子事件派发库"（设计文档 §6：native-input 只产生原生输入；Runtime 负责控制权、状态与停止）。带内部时序循环的 click() 是策略层职责下沉。
3. **与设计文档一致的正确方向**：设计文档 §3"多击：完整保留次数和平台原生多击语义……native-input 保留事件元数据能力"。应评估**每个 Button 原子事件携带 click-count/click-state 元数据**的方案：
   - runtime plan 继续产出 N 对 press/release（含 Sleep），继续掌管时序、取消与清理；
   - 事件结构增元数据字段（如 `click_index/click_count`），driver 在 macOS 上据此设置递增 clickState，Windows/X11 无原生 count 字段时**明确由 OS 按时序聚合**并如实标注该语义来源；
   - 该方案是否引入 trait/IR 破坏性变更、Windows App 端 `MouseDown.Clicks` 实际读数等均**【推测/待证】**，需按用例 multiclick suite 真机验证，本报告不据此实施。

---

## 4. Linux/X11 与 Rc/RefCell 两处事实纠错

### 4.1 X11 现状（纠"no-op 假支持"担忧的表述基础）
- **【代码事实】** `crates/native-input/src/linux_x11.rs:379` 有真实 `fn button(&mut self, button, direction)`，经 `XTestFakeButtonEvent`（:343）派发真实系统按钮事件；无 count 字段，plan 的多对 press/release 会逐一真实注入，count 语义与其他平台一样退化为"时间上相邻的 N 次单击"。**当前 X11 没有"count 被忽略/no-op 成功"的假支持路径**——count 从未到达 driver 层。纠错要求维持：任何新设计不得给 X11 加 no-op 成功或静默忽略 count 的假实现；只能正确系统事件（OS 时序聚合）或显式 unsupported。
- 同理纠正原报告 §1.3"Linux/X11 同步加 no-op/正确实现，避免 trait 变更破坏编译"——"加 no-op"这个提法本身应删掉。

### 4.2 Rc/RefCell 编译阻塞：无证据，且与当前代码不符
- **【代码事实】** 当前源码中 `Rc/RefCell` 仅出现在 `#[cfg(test)]` 测试代码（`src/backend/dispatch.rs:122-226`、`crates/native-input/src/macos/mod.rs:388+`、`crates/native-input/src/session.rs:234+`），非测试代码无使用。
- **【代码事实】** 本次实跑 `cargo check --target x86_64-pc-windows-msvc --all-targets` 失败，但失败点是 **`ring` 依赖的 C 编译**（cross-compile 工具链缺 `assert.h`），与 Rc/RefCell 无关。
- **【代码事实】** 检索 `.agents/reports/` 与 `.agents/reviews/` 全部文档，"Rc/RefCell 阻塞"只出现在原报告自己 §6 第 4 条（还标注"原报告的"，指向不明）。**无任何来源支撑该说法**，应整体删除，回归门禁第 4 条只保留"现有纯测试全绿 + Windows msvc target 可编译"（后者当前受本机 cross-compile 工具链限制，需在具备工具链的环境验证）。

---

## 5. 引用准确性纠错（文件名/行号/trait 名）

| 原报告表述 | 事实 | 结论 |
|---|---|---|
| "`Driver` trait（`crates/native-input/src/types.rs:178` 附近）" | `pub trait Driver` 在 **types.rs:129** | 行号错误 |
| "旧 fake 支持不留：`NativeInput::button`"（置于 native-input §内） | `NativeInput` trait 是 **src/backend/dispatch.rs:22** 的 backend 层内部 trait，`fn button` :27；native-input crate 内的 trait 名是 `Driver` | 归属混淆；后端透传层与 driver 契约是两件事 |
| "`actions.rs:163-185` count 解析" | count 解析在 **~161-185**，`CLICK_MAX_COUNT` :19、`MULTI_CLICK_INTERVAL_MS` :24 | 基本准确 |
| "`plan.rs:152-175`" click 编译 | Click 分支约 **148-180** | 基本准确 |
| "`macos/mod.rs:218-228` clickState 硬编码" | 实际在 **:223**（`post_mouse_event` 内） | 准确 |
| "press 取 `cursor_point()`（macos/mod.rs:306）" | `cursor_point` 定义 :230，`button()` :304 调用 :306 | 准确 |
| "`dispatch.rs:78-87` Button 透传无 click-count 字段" | :78 起 `fn button` 仅映射方向，无 count 字段【代码事实】 | 准确 |
| Windows builder scroll/溢出行号 | `WHEEL_DELTA` :29、`checked_mul` :207 | 准确 |
| 拖拽/scroll/key_hold/chord 各节 | 抽查的行号与代码一致【代码事实】；Windows drag 行为、key_hold 真机项维持【推测/待证】 | 本节无需纠错，维持原结论 |

**当前无任何名为 `NativeInput` 的 native-input crate trait**——设计讨论中提"Driver"时一律指 `crates/native-input/src/types.rs:129`。

---

## 6. 设计文档 §3/§4/§6 与用例规范状态声明一致性核查

`docs/superpowers/specs/2026-09-30-complete-computer-actions-design.md` 与 `docs/computer-use-acceptance-cases.md` 交叉核对：

- **一致**：设计 §1"Windows四suite已真实GLM执行 10/10、9/10、5/6、5/6，点击32/32"与 GUI 验证报告一致（32=10+10+6+6 全 HIT，无 WRONG，HIT 事实成立；但 case01 之外的 Mac 点击验证不在此列）。§1"Mac尚欠known-input、punctuation、emoji和计算器3次"准确；设计文档**没有**犯原报告"四 suite 全未完成"的错。
- **一致**：用例规范第 4 行"状态：用例定义/待实现"、第 103 行"不代表…已经可执行"、状态枚举 planned/pure_verified/gui_verified 等，与设计 §5"该文档是用例规范，不冒充已经存在的可执行fixture"一致，无状态洗白。
- **一致**：设计 §3"多击：完整保留次数和平台原生多击语义；不能仅连发若干click并声称所有平台均正确"——与 macOS click_state 代码缺口匹配，且其"事件元数据能力"的措辞与本报告 §3 的原子事件元数据方案方向一致；**不包含**原报告方案 A 的"压缩为一次 driver.click"表述。
- **一致**：设计 §6 操控提示分层（native-input 只产原生输入 / Runtime 管状态 / Host 侧 Presentation）与"事件元数据留在 driver、时序留给 runtime"的边界一致。
- **需注意（非矛盾，是留白）**：设计 §3"已有接口能满足契约时不增加新MCP工具；需要事件字段时修改内部IR"——即多击元数据走"修改内部 IR"路径，但该 IR 变更（Button 事件携带 click_index/click_count）在设计文档中**未具体展开**，实施前应先锁定字段与 trait 契约，避免不同 worker 各自发明（与 §4.2"不能让不同worker各自发明协议"同一条纪律）。

---

## 7. 纠错结论汇总

1. **进展纠错**：Mac baseline 已 10/10 文本、HIT 9/10（case01 无 HIT 保留），`-e2` header 有实拍；原报告"四 suite 全未完成"过时，必须以 GUI 验证报告后半段为准。Windows baseline 的"旧 helper 5 分钟到期"描述方向正确（run ≈15 分钟 > timer 5 分钟），但背景露出是多因素（含 agent console 覆盖），引用时不得单一归因。
2. **P0 表述纠错**：macOS click_state 固定 1 是已证实的代码缺口（macos/mod.rs:223），正确表述为"不能保证多击语义"；禁止断言所有应用实际 clickCount=1/双击必然失效。
3. **方案否决**：不采纳"压成 driver.click(count) 耗时循环"；正确方向是 Button 原子事件携带 click-count/state 元数据，runtime 掌管时序/取消，driver 只做原子派发；Windows/X11 无原生 count 字段时明确由 OS 时序聚合并如实标注，效果真机验证。
4. **X11/Rc 纠错**：X11 当前有真实 button 事件注入、无 count 假支持；Rc/RefCell"编译阻塞"无来源、与代码不符（仅测试代码使用；msvc target 当前失败原因是 ring C 交叉编译工具链）。
5. **引用纠错**：`Driver` 在 types.rs:129；`NativeInput` 是 src/backend/dispatch.rs:22 的后端内部 trait；其余抽查行号基本准确。
6. **文档一致性**：设计文档与用例规范的状态声明相互一致、无洗白；唯多击 IR 元数据的具体契约需在实施前锁定。
