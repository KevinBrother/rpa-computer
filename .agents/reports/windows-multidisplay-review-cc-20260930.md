# Windows 多屏 root 集成 — 独立静态评审（CC）

日期：2026-09-30。角色：只读评审，非实现者/测试执行者。实际模型：glm-5.3-flash（Claude Code CLI）；本轮仅此一个模型，无双模型对比。

## 评审范围与证据基础

- 读取 hand-off：`.agents/reports/windows-multidisplay-integration-sol-20260930.md`；契约：`.agents/CONTRACT.md`（含 2026-09-30 Windows multiscreen amendment）；锁定计划：`docs/superpowers/plans/2026-09-30-windows-multidisplay-integration.md`；源码契约文档 `docs/windows-multidisplay.md`。
- **冻结清单核对：`source-freeze.sha256` 全部 34 条逐文件 `shasum -a 256 -c` 通过，无 mismatch。** 原 6 RED 文件 hash `79c30152…` 一致（仅 pin）。
- 实际通读生产代码：`src/backend/{mod,windows,desktop}.rs`、`src/backend/display/{mod,budget,metadata,single}.rs`、`src/runtime/{runtime,session,session/observation,display,display_input,execute,plan,actions,error,tools}.rs`、`src/mcp/{bounded_output,jsonrpc,mock_backend}.rs`、`src/feedback/{backend,authority,mod}.rs`，并抽查 `multidisplay_integration.rs`/`display_tests.rs` 的 seam 断言。未运行任何测试/构建/GUI/网络（遵守评审边界）。

## 按任务 5 点逐项结论

### 1. 选择校验先于副作用 / 无静默回退 / authority+revision / capture pre-post / 缺设备与查询错误诚实

**未发现契约破坏。** 静态证据：

- `runtime/display.rs:8-34`：selector 严格解析——未知 key、`display` 缺失以外字段、`kind` 未知、`id` 空/`"desktop"`/超 128 全部 `invalid_arguments`，且在任何 backend 查询前拒绝（`multidisplay_integration.rs:35` 断言零查询）。省略 `display` = Primary，非静默改选。
- `backend/display/mod.rs:40-45`：`select` 先 validate 后 commit（`selection.clone()` 仅在成功后）；ID 不存在经 `metadata::geometry` → `display_not_found`（metadata.rs:64-69），不 fallback primary。
- generation 贯穿：`runtime.rs:194` open 绑定完整 `Generation`；`display::check`（display.rs:69-86）用 `Generation` 相等（含 tracker identity，非仅 revision）；observation 前后各一次（session/observation.rs:28、106）；每个 atomic event 经 `display_input.rs:142` 守卫；resume（runtime.rs:429-435）与 step 前 validate_step（session.rs:744）均查。
- capture pre/post：`display/mod.rs:46-64` — before snapshot → plan → `engine.observe` → **post generation 不等即 `geometry_changed`** → validate → PNG 字节检查。无旧图 fallback；provider 路径 PNG/mapping/拓扑三元一致性在 observation.rs:45-67 逐项校验后采信。
- 多屏 legacy 回退被显式封死：observation.rs:81-86 — 有完整 snapshot 但 `capture_display` 返回 None 且 displays>1 → `capture_error`，不静默用整图缩放。缺设备/查询错误：`display_not_found`、`provider_error` 映射（display/mod.rs:85-95）均透传，无 None 冒充成功（Backend trait 默认实现的 None 仅限 legacy primary-only，且有 `topology_note` 说明）。

### 2. 实际 resize PNG 映射 / 负原点 / 混合 DPI / gap / native 单位

**未发现契约破坏。** 静态证据：

- provider 路径用冻结库 `ObservationMapping`（display_input.rs:22-27 `map_image`、metadata.rs:46-51 逐 region `image_rect`），observation.rs:54-58 强制 `mapping.size() == 解码后实际 PNG 尺寸`——不是请求上限，也不从总 bbox 比例派发。
- `metadata::geometry`（metadata.rs:52-103）从 native bounds 独立计算 union（i64 中间量，负原点安全：测试断言 `(-100,0,200,150)`）；`input_origin`/`input_size` 是 native physical pixels，`native_unit="physical_pixels"` 显式（metadata.rs:10-14），mock 单位显式声明不从 cfg 推断（single.rs 模块注释 + mock_backend.rs:42-45）。
- 用户 waypoint 不能落 gap：`contains`（display_input.rs:22-27，region 存在时走 `map_image` 拒绝 gap）在 validate_step（session.rs:797-807）对所有 `action.positions()`（含 drag 全部 path 点，actions.rs:111-118）前置拒绝 → `not_started` 零注入。跨屏 drag 内部采样用库 `DragPlan`（display_input.rs:42），采样可跨 gap，且每次 move 按最新 generation 重采样并比对（display_input.rs:152-163）。
- 边界收敛风险（edge rounding 溢出屏幕）：依赖 `map_image`/`DragPlan::sample` 的冻结库钳制——静态无法证明，列入实机 gate。

### 3. cancel/deadline 顺序与 not_started vs partial / 幂等与 resume 失效

**未发现契约破坏。** 静态证据：

- `execute_plan_guarded`（execute.rs:127-263）顺序：cancel → deadline(前) → guard → cancel（防慢枚举期间 Stop）→ deadline(后)（execute.rs:204-218，对应新增测试 `topology_guard_cannot_dispatch_after_input_deadline`）。首个 inject 前 deadline 耗尽/取消 → `NotStarted` + `NotNeeded`（execute.rs:269-275, 291-292）；已注入后失败/取消 → `Partial` + best-effort release（只回答已成功 press 的 release，execute.rs:326-355），`release_all` 失败诚实 `cleanup_failed` 并 fault。
- 幂等：dedup 在一切 stale 校验之前（session.rs:563-586），同 body 原样回放 prior record，不重注入；body 逐字节比较（canonical_body），非 hash。`get_step` 已知请求 `is_error=false` + `step_is_error`（runtime.rs:322-329），符合 2026-09-24 澄清。
- resume：先 display::check 完整 generation → geometry 全量比较（runtime.rs:429-453）→ 才清 cancel；`invalidate_observations` 丢弃 basis 但保留 request/results（runtime.rs:461-462, session.rs:453-457）。feedback 永久撤销在 `Runtime::call` 顶部硬闸 + 事后闸（runtime.rs:79-109），open/resume/observe 无法绕过（`display_tests.rs:68` 覆盖）。
- 说明（非缺陷）：mid-drag 拓扑/查询中止时 `StepRecord.cancelled=true`（execute.rs:190 guard 失败置 cancelled）——这是本轮 seam 测试显式断言的设计（`multidisplay_integration.rs:220`），cancelled 语义为"输入被停止"，与用户 Stop 共用标志。可接受；如需区分可后续加独立字段，不属本轮破坏。

### 4. 预算链与交付失败保留 dispatch truth

**未发现契约破坏。** 静态证据：

- 分层链完整：源 64MiB/engine 192MiB（display/mod.rs:24-25）→ `budget::bounded` 把**所有**可能 plan 钳到 `(16MiB-128)/6` 输出像素（budget.rs:7-16，race 不可能换用不受限计划）→ 库 estimator 预检（mod.rs:52）→ 实际 PNG 字节复检（mod.rs:64 及 observation.rs:105）→ session 缓存 32MiB admission（session.rs:282-324，单图超限 `resource_limit`）→ MCP base64 分配前检查（bounded_output.rs:38-45）→ 最终 counting-writer 实测转义序列化长度（bounded_output.rs:25-31, 52-55）。64MiB/1MiB cap 未动（jsonrpc.rs:12-15）。
- 交付失败：`tool_response_with_limit` 超限路径保留 `input_outcome/observation_outcome/cleanup_outcome/cancelled/error/request_id` 并标记 `delivery_error:output_budget` + `image_delivery_outcome:withheld`（bounded_output.rs:57-95），不截断 PNG/JSON，不改写 dispatched。测试覆盖（oversized_image_delivery_retains_dispatch_and_cleanup_truth）。
- 整帧无 cap 以上泄漏路径：`FrameWriter::send` 最终防线（jsonrpc.rs:317-331）；remote client 侧同 cap（remote/client.rs:413）。

### 5. feedback 转发 / 撤销 / surface id / 无 native 依赖

**未发现契约破坏。** 静态证据：

- `FactBackend` 全量转发 `display_selections/display_snapshot/select_display/capture_display`（backend.rs:25-79），capture 错误原样透传（display_tests.rs:37-38 断言错误不被吞）。
- surface：单屏用真实 `geometry.surface_id`（来自 metadata.rs:91-95 的真实 display id）与真实 bounds；desktop 专用 `"desktop"` + 全选 union（authority.rs:342-351）。gap 内部 transit 不发布 ring 目标（backend.rs:104-115，regions 外 Move 置空 position 不发 confirmed），confirmed 仅在 native inject 成功后（backend.rs:91 之后）。`DispatchStamp` = (session, surface_id, version)（authority.rs:23-28, 333-341），同一动作内稳定、surface version 变化即失效——此前我怀疑 click pointer 因逐事件 stamp 不同而不发布，核对后确认 stamp 在同 surface version 下相等，click/drag pointer 语义正确。
- 撤销：每个 atomic input 前 `begin_dispatch` + `is_terminated` 双检（backend.rs:81-90），撤销返回 `cancelled` 错误；`stop()` 撤权与永久 revoke 同临界区（authority.rs:131-147）；shutdown 区分 retire/complete_terminal/quarantined（runtime.rs:529-546, feedback/worker.rs:12-18）。
- native-input 零依赖：feedback 仅经 `Runtime::new_with_feedback` 包装 Backend，off 路径不构造 FactBackend；wire v1 未改；`capture_exclusion_verified:false` 恒定（runtime.rs:132），不冒充 verified。

## 发现

**无阻断性 / 契约破坏性缺陷。** 两条低severity备注（供 Codex owner 参考，非必须修）：

1. `src/runtime/execute.rs:190`（及 `src/runtime/display_input.rs:131` 防御性 prepare 失败路径）：guard 失败复用 `cancelled` 标志，导致拓扑中止/防御性 invalid_action 的记录 `cancelled=true` 且跳过 settle。与本轮新测试断言一致（设计选择），但与"取消=用户 Stop"的直觉语义混同；最小改法是 `ExecutionOutcome` 增加独立 `guard_aborted` 字段并仅在用户 cancel 时置 `cancelled`。回归点：`multidisplay_integration.rs` 的 `topology_change_or_query_error_mid_drag_stops_and_releases_truthfully` 与 `cancellation_mid_cross_screen_drag…` 两测试需同步改断言。
2. `src/runtime/display.rs:62-65`（describe 错误路径）：`available:false` + `preflight_error` 时未携带 `display_selections`；`computer_describe` 失败后模型无法得知 backend 仍支持 id/desktop。文档（docs/windows-multidisplay.md:14）只要求三项，故非违约；最小改法是错误分支也输出 `backend.display_selections()`。回归：`describe_is_fresh_complete_physical_topology_without_capture` 补一条失败分支断言。

## 静态证明边界 / 剩余 GUI gates（不宣称 complete）

以上全部为**源码级静态推理 + 冻结 hash 核对**，不构成任何 Windows 实机证据。`cargo check exit0` 只证明类型检查。仍需（与 hand-off 清单一致）：

- Windows 交叉链接 test exe（全新 CC target，JSON 精确取路径+SHA）后按 filter 逐个执行：原 6 RED 转 GREEN、23 项新 seam tests、受影响 root 回归（数值原生退出码）。
- 真实多屏实机（协调授权后）：真实枚举与 physical units、负原点/混合 DPI/竖屏/gap 的实际 PNG 与映射、热插拔/旋转 mid-drag 停止与 release、实际 cleanup failure、Stop 后禁止 open/resume、反馈窗口截图排除（requested ≠ verified）。
- 冻结库 `map_image`/`DragPlan::sample` 的边界钳制（edge rounding 不溢出屏幕）、`Generation` 相等含 tracker identity——本轮依赖库契约与库内测试声明，未独立证明。
- 采样式 generation 检查无法证明两次查询间瞬时切换又恢复；无 OS topology 通知订阅（hand-off 已声明）。

评审结束；缺陷修复移交 Codex owner。

---

## Correction（2026-09-30，依据真实 CC Windows 执行证据追加）

**原第 5 点"feedback 无契约破坏/无阻断"结论被实际执行证据推翻。**

真实 CC Windows 交叉验证（`.agents/reports/windows-multidisplay-green-cc-20260930.md`，evidence `filters-run4.output`）：29 项中 26 pass / **3 fail（`feedback::display_tests::` 3/3，native exit 101）**。三项失败均 `Protocol(InvalidField)` 系：

- `wrapper_forwards_complete_facts_and_query_capture_errors` — display_tests.rs:31 `unwrap()` on `Err(Protocol(InvalidField))`
- `desktop_surface_has_union_bounds_and_internal_gap_move_has_no_ring_target` — display_tests.rs:46 同上
- `full_desktop_stop_still_permanently_prevents_open_or_resume` — display_tests.rs:77 open 被 `cancelled`/`refused the new session authority` 拒绝

**codec 边界核实（补读原始冻结源确认，非此前独立发现）**：

- 冻结生产代码 `src/backend/display/metadata.rs:7-9`：`generation()` 返回 `format!("topology:{g:?}")` —— Debug 格式含**空格、`{`、`}`、`(`、`)`** 等字符。该 token 经 `metadata::geometry`（metadata.rs:101）成为多屏 `Geometry.version`。
- `src/feedback/authority.rs:342-351`：`surface()` 把 `Geometry.version` 原样放入 `Surface.version`。
- `crates/desktop-feedback/src/protocol.rs:216-220` `validate_surface_version`（`:222-231` `validate_ascii_token`）只接受 **≤128 ASCII 字母数字 + `-_.:`**（C# renderer 侧同样约束）。空格/花括号不合规 → `Diagnostic::InvalidField`。
- 失败链：`grant()`（authority.rs:114-115 `surface.validate()`）→ `FeedbackError::Protocol(InvalidField)`；`Runtime::open`（src/runtime/runtime.rs:197-205）把 grant 失败转为 `cancelled`/"refused the new session authority"，解释第三个失败的表现。多屏 Surface.version 即 Debug 格式 generation，**必然**校验失败；单屏旧格式（如 `d1:o0,0:i1512x982:c3024x1964:r0`）本合规，故此缺陷是多屏 generation 投影引入的真实 prod-token 编码错误，不是测试断言问题——**修复方向是生产 token 编码（改为 charset 兼容的 opaque 编码，或协调 codec 侧），不是调整测试**。

**评审方法修正**：本审查核对了 `Surface::validate` 存在与 surface id/bounds 语义，但未把冻结 generation 投影的字符集与 codec 白名单交叉比对（我当时引用的 `version.contains("tracker")` 断言也顺带默认了合法性）；静态"无契约破坏"结论在 feedback wire 边界上不成立。第 5 点其余转发/gap/撤销语义的静态分析不受此影响，但 feedback 路径在修复前应视为 FAIL。

**基线声明**：以上 correction 基于旧 SOURCE_FROZEN manifest（`946e8ef4…`）评审；root owner 现已开始修代码，当前工作树变化不作为本次审查 baseline。修复未完成、未验证；修复后的代码须由 CC 在新 target 重测（至少 filter4 三项 + 受影响 feedback/runtime 回归），本轮不宣称任何修复完成。模型：GLM（本条 correction 与原报告同）。
