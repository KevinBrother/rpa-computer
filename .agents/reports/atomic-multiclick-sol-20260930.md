# 原子多击接手交付（2026-09-30）

## 状态与职责

**READY_FOR_CC_TEST：产品源码及测试代码实施已结束，源码已停止变动；等待协调者调度真实 Claude Code（CC）+ GLM 独立执行测试及验收。**

遵守用户最新角色纠正：Codex + gpt-6.1-sol 仅实施产品源码/测试代码；纠正之后没有执行任何测试、验收或 GUI 操作，仅进行了实现必需的编译检查。没有启动 CLI、其他 agent、Host 程序、GUI、截图、SSH；没有 worktree、commit/push、reset/checkout 或覆盖其他 worker 修改。

工作目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。先读取任务、CONTRACT 尾部及 README 最新角色规则，从旧 CLI 部分实现继续，没有重新实现/假设已完成。fixture、Python、根 Cargo、Host bin、capture、反馈/拓扑及设计/契约文档均未修改。

**下列编译成功不是测试通过/验收通过；新增测试没有执行。历史开发自检也不证明最终源码通过测试。**

## 实际接口与补齐内容

1. 延续旧实现：`PlanEvent::Button`、`InputEvent::Button` 带 `click_count: u8`；`NativeInput::button`、raw `Driver::button` 及独立 `Input::button` 都显式携带同一计数，无旧签名兼容 shim。有效范围 `1..=3`，非法计数在 native 派发前拒绝。
2. Runtime 的 Click N 编译为 N 对，计数 `1..N`；拖拽为 1；保留原 Runtime 的 90ms 多击间隔与取消检查。没有新增 driver sleep 或整段多击循环。
3. **补齐取消路径缺陷**：旧 `Plan::is_answered_release` 只判断“之前有任意成功 press”，使第二击取消/失败后可能发出尚未按下的第三击 release。现在按 button+click_count 查找同一对的前一事件，且只回答确实成功派发的 press；Key 分支同样按 key 匹配，不再借用另一 modifier 的 press。无需修改执行器/会话策略。
4. macOS 将未派发的 CGEvent 构造提取为生产与测试共用 `create_mouse_event`；按钮实际 `kCGMouseEventClickState` 使用传入 1/2/3；普通 Move click-state=0，dragged=1。新增无 post 的真实 CGEvent type/button/count/location/flags 字段读回测试，避免只测平行 Rust 元组。FFI getter 类型与本机 Apple SDK 头文件核对；测试不创建 Platform、不请求权限、不调用 CGEventPost。
5. Windows 在真实 SendInput planner 中校验 count 后构造**一个**真实 down/up EventSpec；X11 在真实 XTest 使用的纯 specification 中校验 count 后派发**一个**真实事件。无 count 原生字段的平台不使用 no-op 假成功。Windows planner 的纯测试也纳入非 Windows native crate 单测编译。
6. Host 仍为 raw Driver -> DriverInput -> BackendCore，未套独立 Input；BackendCore 是 Host 唯一清理 authority。取消/失败时保留最后 intended press 元数据，release 成功才 forget；移除 cleanup 在缺 metadata 时静默 fallback 为 1 的行为，改为明确错误。
7. 独立 Input 使用 `held_keys: HashSet<Key>` 与 `held_buttons: HashMap<Button,u8>`，让按钮只出现在元数据 map，消除旧混合 held 集里的不可达 Button/no-op 分支。重复按钮 press 更新同一 entry，uncertain press 先记录；失败 release/cleanup 保留原计数供重试；Drop 以确定顺序尝试每个仍持有的项。
8. 保留 Unicode scalar、CRLF、Return/Tab 及原文本节奏与 native 文本行为。FakeBackend 校验 count 与产品保持一致。

## 本接手实际改动文件

以下为相对接手基线的文件清单，而非整个共享脏工作区的 git diff：

- `crates/native-input/README.md`：必要 Button API/平台/无输入测试说明。
- `crates/native-input/src/lib.rs`：非 Windows host 编译 Windows 纯 planner 测试。
- `crates/native-input/src/session.rs`：分离 held key/button 状态及 metadata 重试/Drop 测试。
- `crates/native-input/src/macos/mod.rs`：无 post 构造 seam、move/drag 字段语义。
- `crates/native-input/src/macos/ffi.rs`：测试读回 getter/字段常量。
- `crates/native-input/src/macos/mouse_tests.rs`（新增）：实际未派发 CGEvent 字段读回。
- `crates/native-input/src/windows.rs`：count 交给真实 planner 校验。
- `crates/native-input/src/windows/builder.rs`：原子事件参数校验及纯测试。
- `crates/native-input/src/linux_x11.rs`：XTest 按钮 specification 校验及纯测试。
- `src/backend/dispatch.rs`：raw Driver seam 与 Host metadata/失败重试/Drop 行为测试。
- `src/backend/dispatch_core.rs`：保留旧 metadata 跟踪并移除 cleanup 默认计数 fallback。
- `src/runtime/plan.rs`：精确 release 配对及计数/间隔回归。
- `src/runtime/testutil.rs`：FakeBackend count 校验。
- `src/runtime/session/tests.rs`：挂载新增独立测试模块。
- `src/runtime/session/tests/multiclick.rs`（新增）：Runtime -> 真实 BackendCore -> recording NativeInput 集成测试。

另写本报告与 `.agents/runs/atomic-multiclick-sol/` 日志/清单。既有 `src/backend/mod.rs`、native `types.rs`、`src/mcp/worker/tests.rs` 及其它旧调用点机械适配从基线保留，本接手未再次改写。**没有授权写集之外的 Rust 机械适配。**

源码冻结 SHA-256：`.agents/runs/atomic-multiclick-sol/handoff-source-hashes.json`；接手基线：同目录 `baseline-hashes.json`。新测试模块拆分，不向已较大的 session 主文件堆逻辑；本接手文件均未超过 1000 行。

## 已执行：编译检查（不是测试/验收）

日志目录：`.agents/runs/atomic-multiclick-sol/`；机器可读结果为 `compilation-results.json`。所有命令在工作目录运行，测试二进制/Host 程序均未启动。

| 命令 | 精确结果 | 日志 |
|---|---|---|
| `cargo check --manifest-path crates/native-input/Cargo.toml --tests` | exit 0 | `check-native-mac-final.log` |
| `cargo check --tests` | exit 0 | `check-root-mac-final.log` |
| `cargo check --manifest-path crates/native-input/Cargo.toml --tests --target x86_64-pc-windows-msvc` | exit 0 | `check-native-win.log` |
| `cargo check --manifest-path crates/native-input/Cargo.toml --tests --target x86_64-unknown-linux-gnu` | exit 0 | `check-native-linux.log` |
| `cargo build --manifest-path crates/native-input/Cargo.toml --release` | exit 0 | `build-native-mac-release.log` |
| `cargo build --release --bin computer-host` | exit 0，生成 Mac release，未运行 | `build-root-mac-release.log` |

这些 check 包含测试代码的类型检查，不是执行测试；Windows/Linux 为 cross-target 编译检查，不是完整链接/目标机运行。

期间一次 native `cargo check ... --tests` 出现 E0425（将旧混合 held 集重构为 keys-only 时，mock 中局部 `item` 声明也被机械移除），exit 101；已定位并补回仅测试 mock 的声明，最终编译 exit 0。保留 `check-native-mac-transient-error.log`，它是**编译错误而非行为 red**。最终编译日志未产生 warning。

## 角色纠正之前执行的历史开发自检

明确标注为 **Codex 开发自检，非 CC+GLM 验收；运行在本轮最终源码变更之前**：

| 命令 | 精确结果 | 日志 |
|---|---|---|
| `cargo test --manifest-path crates/native-input/Cargo.toml` | exit 0；42 passed、0 failed、0 ignored；doc-tests 0 | `baseline-native.log` |
| `cargo test --lib` | exit 0；214 passed、0 failed、1 ignored；ignored 未运行 | `baseline-lib.log` |

`pre-role-correction-selfchecks.json` 保存分类与结果。用户纠正角色后没有继续执行测试；未执行 `cargo test --tests` 或任何最终源码测试。

## 已核查的旧合法 TDD 行为 red

未改写旧日志。定向读取旧 worker JSONL 恢复命令，而非整段输出巨大日志：

- `.agents/runs/atomic-multiclick/red1-plan-click-count.log`：`multi_click_pairs_carry_increasing_click_count` 断言 `[1,1,1,1,1,1] != [1,1,2,2,3,3]`；213 passed、1 failed、1 ignored。原 CLI 命令：`mkdir -p .agents/runs/atomic-multiclick && cargo test --lib 2>&1 | tail -40 > .agents/runs/atomic-multiclick/red1-plan-click-count.log; echo saved`。
- `.agents/runs/atomic-multiclick/red2-macos-click-state.log`：`button_event_spec_carries_click_count_and_rejects_invalid` 期待 2、实际 1；0 passed、1 failed、36 filtered。原 CLI 在 native crate 的命令：`cargo test macos::tests::button_event_spec 2>&1 | tail -15 > /Volumes/doc/workspace/datagrand/rpa/rpa-computer/.agents/runs/atomic-multiclick/red2-macos-click-state.log; echo saved`。

两者是可观察断言失败，不是缺字段/编译错误。旧命令通过 pipe+echo 掩盖 cargo exit；旧日志没有保存独立 cargo 数字退出码，不将其伪称为完整退出码证据。接手前两个原回归在历史开发自检中已显示通过，但**不是最终源码验收**。

新增取消配对/Move 语义等回归没有本角色新执行的 red/green。按最新分工，这部分行为验证交给 CC+GLM，不以编译成功冒充行为 green；也没有在共享源码中恢复旧代码来制造 red。

## 待协调者安排 CC+GLM 执行（本接手未执行）

### macOS 无真实输入回归：工作目录 rpa-computer

优先定位新增高风险行为：

```sh
cargo test --manifest-path crates/native-input/Cargo.toml macos::mouse_tests
cargo test --manifest-path crates/native-input/Cargo.toml session::tests
cargo test --manifest-path crates/native-input/Cargo.toml windows_builder_tests
cargo test --lib runtime::plan::tests
cargo test --lib runtime::session::tests::multiclick
cargo test --lib backend::dispatch::tests
```

然后正式完整纯测试及 release 核对：

```sh
cargo test --manifest-path crates/native-input/Cargo.toml
cargo test --lib
cargo test --tests
cargo build --release --bin computer-host
```

**不要加 `--ignored`/`--include-ignored`。** root tests 保留旧 ignored 真实输入测试。本模块所有新测试仅 recording mock/未派发 event 构造，不接触 GUI。

CC 应特别核查第二击取消不会出现第三击 release、失败 press/release 同一 count 清理重试、invalid 0/4 零派发，以及文本原回归没有改变。记录实际 red/green/退出码，任何修复交回 Codex，不让测试执行者顺带改产品源码。

### Windows / Linux 目标机

在既有目标机/工具链由 CC+GLM 编译/执行 native 纯测试（不启动 Host GUI）：

```sh
cargo test --manifest-path crates/native-input/Cargo.toml
```

Windows 还需要协调者安排完整 Host release 构建及 root lib/tests，cross-target cargo check 不替代该门禁。Linux 要有现有 X11/XTest 链接库方可执行全 native 测试；本接手没有安装工具/系统库。

### 真实 GUI

本任务没有执行或发起 GUI 验收。双击/三击/拖拽真实语义由协调者另行调度 CC+GLM；Windows RPAD/Executor 若使用，须按既有约束运行在交互式桌面会话。旧 GUI 统计/证据保持不动。

## 剩余风险 / 不作通过声明

- 最终完整纯测试、独立审查和 GUI 均待 CC+GLM；代码冻结可开始测试，但不是产品验收已完成。
- Windows/Linux 只有 native lib/tests cross-target check，尚无本轮完整链接/目标机运行结果；Windows Host 真构建待协调者。
- macOS 实际未派发 CGEvent 字段读回测试已写且编译，尚未执行；应用收到 post 后的 count/选择行为更不能由此推断。
- Windows/X11 多击聚合受目标 OS/应用时间、位置及配置影响。Runtime 当前 90ms 间隔不变，不保证任意系统阈值；真实应用按既有 fixture 另验，不自动调整系统设置。
- uncertain native 失败的具体物理状态仍可能未知；记录元数据并重试 release 不等于能证明最终 OS 状态。本实现与测试保持真实错误/Partial/CleanupOutcome，不把失败抹成成功。
- 显式低层 release 仍使用调用者提供的 count，调用者负责与 press 配对；release_all/Drop 使用最后记住的 intended press count。
