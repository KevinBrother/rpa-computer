# CC+GLM 独立审查与纯测试：原子多击（2026-09-30）

角色：独立验证者（CC）。未写/未修任何产品源码、测试代码、构建脚本；未启动 Host/GUI/agent；未 commit/push；未运行 ignored 测试；未加 `--ignored`/`--include-ignored`。仅写本报告与 `.agents/runs/atomic-multiclick-cc-*.log`。

已读：`.agents/tasks/atomic-multiclick-implementation-20260930.md`、`.agents/reports/atomic-multiclick-sol-20260930.md`，并与实际代码逐文件核对。

## 一、规格审查结论：**通过（附 1 个测试断言缺陷，见质量节）**

逐条核对任务契约：

1. **count1..=3 贯穿 Runtime→BackendCore→Driver** ✅
   - `PlanEvent::Button { click_count }`（`src/runtime/plan.rs:13`）→ `InputEvent::Button`（`plan.rs:166`）→ `BackendCore::inject_event`（`src/backend/dispatch_core.rs:132-159`）→ `NativeInput::button` → raw `Driver::button`（`src/backend/dispatch.rs:86`）。独立 `Input::button`（`crates/native-input/src/session.rs:140`）同一签名，无旧签名 shim。
2. **Action Click count=N 编译为 N 对、元数据 1..=N，Runtime 掌管时序/取消** ✅
   - `compile_plan`（`plan.rs:196-218`）逐对 `click_count: i+1`，对间 `MULTI_CLICK_INTERVAL_MS` Sleep 属于 Plan，取消检查仍由 executor 在事件边界执行（见 `multiclick.rs::cancellation_during_each_click_or_gap...` 通过）。driver 内无 sleep/多击循环（grep 核实 Win/X11/Mac button 各只派发一个事件）。
3. **非法 count 前置拒绝** ✅ 三层：Action 解析（`src/runtime/actions.rs:174` 限 1..=3，count 0/4 在 `invalid_action_counts_reject_before_any_backend_injection` 中 inject_calls=0）；BackendCore（`dispatch_core.rs:139` `is_valid_click_count`，dispatch 前拒绝）；各 Driver（Mac `macos/mod.rs:109`、Win `windows/builder.rs:147`、X11 `linux_x11.rs:371`、独立 `session.rs:141`），均在任何 native 调用前。
4. **Mac 真实 event 字段** ✅ `create_mouse_event`（`macos/mod.rs:61`）为生产与测试共用 seam，`FIELD_MOUSE_CLICK_STATE` 写入传入 count；测试真实构造未 post 的 CGEvent 并读回 type/button/click_state/location/flags（`macos/mouse_tests.rs`）。Move click-state=0、dragged=1（`macos/mod.rs:53-56`），不把 Move 当第二次点击。
5. **Win/X11 真实原子事件** ✅ Windows `builder::mouse_button` 校验后构造真实 LEFT/RIGHT/MIDDLEDOWN/UP `EventSpec`（`windows/builder.rs:141-171`），经 SendInput 派发（`windows.rs:137`）；X11 `button_event_spec` 校验后经 XTestFakeButtonEvent 派发真实事件（`linux_x11.rs:369-410`）。两平台无 count 字段，代码注释与实现均明确"OS 按时序/位置聚合"，无 no-op 假成功。
6. **不改变文本语义** ✅ `normalize_newlines`、逐 scalar、CRLF→单 Return、Tab 映射逻辑与此前一致（`plan.rs:45-57`、`dispatch_core.rs:114-119,175-187`）；文本回归测试全部通过。
7. **Host 唯一 held authority** ✅ Host 仍用 raw Driver→DriverInput→BackendCore（`dispatch.rs:12` 模块注释、`DriverInput` 适配器），未套独立 Input；独立 `Input` 自持 `held_keys: HashSet` / `held_buttons: HashMap`（`session.rs:77-78`），两套状态互不借用。

## 二、质量/缺陷审查结论：**实现通过；发现 1 个新增测试的断言缺陷（产品行为正确，测试预期写错）**

1. **`Plan::is_answered_release` 多击/多 modifier** ✅ 已修正确（`plan.rs:99-148`）：
   - 按 `button + click_count` 找同一对的前一事件（rev 扫描，非匹配项 `None` 继续回溯），且要求该 press 在 `pressed` 列表中 → 第二击取消后不会发出第三击 release，也不会重复释放 count=1。测试 `release_pass_only_answers_the_same_successfully_pressed_pair`（`plan.rs:574-599`）覆盖 down1 成功/up2 未按/down3 未按等组合。
   - Key 分支按 key 匹配：chord 中 `s` release 不借用 `ctrl` 的成功 press（`plan.rs:597-598` 断言）。
2. **uncertain press/release 保留元数据** ✅ BackendCore：press 先 `held_buttons.insert` 再派发（`dispatch_core.rs:149-150`），release 成功才 remove（`dispatch_core.rs:152-157`）；release_item 缺 metadata 明确报错、不再静默 fallback 为 1（`dispatch_core.rs:85-87`）。独立 Input 同语义（`session.rs:140-158`），重复 press 覆盖同 entry 不积累、不双重 held。集成测试 `uncertain_press_or_release_cleanup_keeps_pair_metadata_and_skips_later_pairs` 验证 3×2 组合：后续对被跳过、cleanup 后 held 空、`cleanup_calls==1`。
3. **release_all / Drop** ✅ 两套实现均先排序后逐项尽力释放，成功才 forget，失败聚合报错且保留重试（`dispatch_core.rs:209-250,253-274`；`session.rs:203-278`）。空集合 noop 成功有测试。
4. **测试测生产构造而非平行 oracle** ✅
   - Mac：`mouse_tests` 通过生产 `create_mouse_event` 构造真实 CGEvent 读回原生字段（仅不 post）。
   - Runtime 集成：`multiclick.rs` 走真实 `Session`→`execute_step`→真实 `BackendCore`→recording NativeInput；断言 recording 调用序列，不重构平行计划。
   - Win/X11 planner 纯测试针对真实 builder/spec 函数（SendInput/XTest 无事件读回 API，属合理边界）。
5. **不将派发成功当 GUI 成功** ✅ 测试与报告均以 `InputOutcome::Dispatched` 等调度层结果为准；`mouse_tests` 文件头明确"Never … CGEventPost"。本报告同样不宣称 GUI 验收。
6. **发现的缺陷（交回实现者，未修改）**：
   - `src/runtime/session/tests/multiclick.rs:194`：`assert_eq!(record.error.as_ref().unwrap().code, "input_failed")`。
   - 实际：`runtime/error.rs:41` 将后端码 `"input_failed"` 映射为协议码 `codes::INPUT_ERROR == "input_error"`；record.error 持有协议码，故实际值为 `"input_error"`。
   - 定性：**测试预期缺陷**，非产品缺陷——该用例的元数据保留、后续对跳过、cleanup 释放等行为断言全部先于 194 行且通过；仅错误码预期与协议映射表矛盾。修复建议：改为 `"input_error"` 或对照 `crate::runtime::error::codes::INPUT_ERROR`。该失败使整条测试链（`cargo test --lib` / `--tests`）exit 101，需实现者修正后重跑。

## 三、实际运行记录（macOS arm64，本机；未运行 ignored；未运行 Host）

| 命令 | 结果 | 日志 |
|---|---|---|
| `cargo test --manifest-path crates/native-input/Cargo.toml` | **exit 0**：72 passed / 0 failed / 0 ignored；doc-tests 0 | `.agents/runs/atomic-multiclick-cc-native.log` |
| `cargo test --lib` | **exit 101**：225 passed / **1 failed** / 1 ignored（未运行） | `.agents/runs/atomic-multiclick-cc-lib.log` |
| `cargo test --tests` | **exit 101**：同一失败（225 passed / 1 failed / 1 ignored） | `.agents/runs/atomic-multiclick-cc-tests.log` |
| `cargo build --release --bin computer-host` | **exit 0**（增量，未运行产物） | `.agents/runs/atomic-multiclick-cc-build-release.log` |

唯一失败：`runtime::session::tests::multiclick::uncertain_press_or_release_cleanup_keeps_pair_metadata_and_skips_later_pairs`（见质量节第 6 条）。

## 四、freeze hash 校验：**一致**

对 `.agents/runs/atomic-multiclick-sol/handoff-source-hashes.json` 全部 15 个文件逐一重算 SHA-256：**15/15 一致**，无缺失。dirty 仓库本身不作失败判断；实现者声明的冻结范围内源码自其报告以来未变动。

## 五、未覆盖项 / 风险

- **未测 Windows/Linux 真实目标机**：仅实现者 cross-target check，无目标机链接/运行；SendInput/XTest 真实行为未验。
- **未测任何 GUI**：macOS 本批为纯测试（无 GUI 输入/截屏/窗口启动）；"双击/三击被目标应用聚合"完全未验收，本批通过 ≠ 通用能力通过。90ms Runtime 间隔与系统双击阈值匹配性仍待真机验证。
- Mac 事件为"构造未 post 的真实 CGEvent 读回"，post 后 OS/应用的 click-state 消费行为不能由此推断。
- uncertain 失败后的真实物理按钮状态仍不可知，元数据保留≠状态确定。

## 六、处置建议

代码冻结范围内实现符合契约；**唯一阻塞项**为 `multiclick.rs:194` 断言码。交回实现者做最小测试修正（一行预期值）后，由协调者安排重跑 `cargo test --lib` / `--tests` 及 Windows/Linux/真机 GUI 阶段。
