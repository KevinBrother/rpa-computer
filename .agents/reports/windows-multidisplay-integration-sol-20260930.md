# Windows 多屏 root StageB Tasks2–4 — SOURCE_FROZEN

日期：2026-09-30。角色：Codex 实现/Windows compile-only；实际测试交给 CC。

## 状态与范围

**SOURCE_FROZEN；没有宣称 Windows GREEN 或 GUI 验收通过。** root Windows provider → Runtime → MCP → feedback adapter 已实现，非只补 describe metadata。保留原 6 RED 断言；本轮新增 23 项纯 seam 测试源码，尚未运行。

已依任务读取计划、contract amendment、两冻结库 README 及 CC 六 RED 报告/raw。CC 已记录的 0pass/6fail/native101 是 StageA 证据，不是本轮结果。renderer/focus/取消测试 owner 的运行结果不计入本报告。

- 不运行 tests、test exe、应用 CLI、Host、SSH、GUI、截图或输入；不做 Mac/Linux check/test。
- 不启动 agent/CC；不 commit/push/worktree/reset。
- 只在专属新 target 做 `cargo check`。**没有写 `target/x86_64-pc-windows-msvc/release` 两 bin，也没有删除/移动/覆盖任何旧 fixed exe/release。**
- 未改冻结 crates、desktop-feedback renderer、fixture/analyzer、focus 写集、test-runner、portable TLS/remote test assets 或新增取消测试 owner's 文件。
- 当前共享工作树包含之前任务/他人的修改。下面清单按本轮开始时 `before.json` 与明确独占写集归属，不把整个 git diff 冒充本轮贡献。

## 实现内容

1. **真实 Windows provider**：`DesktopBackend` 在 owning thread 完成既有 PMv2 prepare 后 attach GdiProvider；持有持续存在的 root DisplayProvider/库 DisplayBackend/tracker。真实枚举、primary/exact ID/desktop capture，删除 Windows 旧 primary screenshots 函数调用路径。库/worker/native-input 未改。
2. **工具/会话**：8 tools 不变；严格解析 open selector/额外字段，缺 ID 不 fallback。describe 查询真实拓扑、不截图；完整 authority+revision generation 贯穿 session、observation、input、resume，包括未选显示器变化。`display_topology` 与 StageA 字段名同时提供；mock 明确声明 unit/tracker，不根据 target cfg 冒充 OS。
3. **映射/输入**：真实库 CapturePlan/ObservationMapping 对应实际 PNG；混合 scale/竖屏/负原点/gap 逐 region 映射，不用总 bbox 比例派发。用户 waypoints 不准在 gap，内部 DragPlan samples 允许跨 gap。每 atomic event 查当前 generation，每 drag move 校验最新库 sample。查询失败或改变时停止后续输入、release，诚实保留 partial/cleanup failed。
4. **取消/deadline**：guard 后重查 cancel 与 input deadline，避免慢枚举后越期派发。deadline 在首个 inject 之前耗尽时明确 not_started；已经成功或可能部分发生的 inject 仍 partial。既有 worker quarantine/idempotency/cache ledger 保留，feedback 永久撤销不能 reopen/resume。
5. **观察/预算**：捕获 pre/post generation 校验和 publication cancel gate；失败无旧图 fallback。源64MiB、engine192MiB、单PNG16MiB、session缓存32MiB；请求轴上限4096未提高，按真实 CapturePlan 缩小且保留正确mapping。库 estimator 预检 + 实际 PNG 字节复检 + base64 admission + 最终 serialized JSON counting gate；64MiB输出cap/1MiB输入cap不变。输出超限保留 dispatch/observation/cleanup/error 事实，只 withheld 图像交付。
6. **feedback**：FactBackend 转发新增能力、selection、topology、mapped capture 与错误。全桌面 surface ID 专用 `desktop`、bounds 为真实选中 native union；单屏真实ID/bounds。gap transit 不发布 ring 新目标；confirmed dispatch 才更新指针。wire v1 不变，capture-exclusion 没有冒充 verified。
7. **模块化**：observation 从 session.rs 拆出；session.rs 最终955行（原约993），runtime.rs718行。新增 provider/metadata/budget/single、display/display_input、bounded_output 独立模块。

## 公共 API 告知

`Backend` 新增有默认实现的 `display_selections`、`display_snapshot`、`select_display`、`capture_display`。已有 backend/mock 实现不用立即增加方法；默认明确 primary-only legacy。真实多屏 backend 必须返回完整 snapshot 和 mapped capture，不得用 None 隐藏错误。新增 `backend::display::DisplayCapture`。

`Worker`、`McpService` 现有公共调用 API **未改变**，取消 flag/调用方式不变。取消 barrier owner 可继续使用现有 API；本轮不修改它的 `tests/windows_cancel_contract.rs`、子目录或 matrix。`Observation` 新增 regions，meta 新增 display_metadata；现有 session 内部测试仅机械补两处 regions literal，未削原断言。

## 精确修改文件（33 个；另本报告）

```text
Cargo.lock
Cargo.toml
docs/windows-multidisplay.md
src/backend/desktop.rs
src/backend/display/budget.rs
src/backend/display/metadata.rs
src/backend/display/mod.rs
src/backend/display/single.rs
src/backend/display/test_support.rs
src/backend/display/tests.rs
src/backend/mod.rs
src/backend/windows.rs
src/feedback/backend.rs
src/feedback/display_tests.rs
src/feedback/mod.rs
src/mcp/bounded_output.rs
src/mcp/jsonrpc.rs
src/mcp/mock_backend.rs
src/mcp/mod.rs
src/mcp/server.rs
src/runtime/display.rs
src/runtime/display_input.rs
src/runtime/error.rs
src/runtime/execute.rs
src/runtime/mod.rs
src/runtime/runtime.rs
src/runtime/runtime/tests.rs
src/runtime/runtime/tests/multidisplay_integration.rs
src/runtime/session.rs
src/runtime/session/observation.rs
src/runtime/session/tests.rs
src/runtime/testutil.rs
src/runtime/tools.rs
```

## 编译及静态检查：实际执行的证据

专属目录：`.agents/runs/windows-multidisplay-integration-sol-20260930/`。

最终命令（加载 CC 已解决的 xwin/rust-lld 环境，立刻覆盖其 target 变量至本轮目录）：

```sh
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR="$PWD/.agents/runs/windows-multidisplay-integration-sol-20260930/target"
cargo check --target x86_64-pc-windows-msvc --tests --locked --offline
cargo check --target x86_64-pc-windows-msvc --lib --bins --locked --offline
```

| 检查 | 原始记录 | 结果 |
|---|---|---|
| 最终 Windows tests 类型检查 | `check-final-tests.log` / `.exit` | exit0；1.40s；无 warning |
| 最终 Windows product 类型检查 | `check-final-product.log` / `.exit` | exit0；0.68s；无 warning |
| 限定本人文件 `git diff --check` | `diff-check.log` / `.exit` | exit0 |

仅对明确文件执行 `rustfmt --edition 2021 --config skip_children=true`，不递归格式化其他 owner/冻结模块。原始 `format-files.txt` 有全部 Rust 文件列表。

早期 `check-1.log` 因抽出 observation 函数时 wait_ms/cancel 参数顺序写反失败，已修回原调用签名；该记录保留。check-2 exit0 有旧函数 dead_code warning，删除旧 Windows capture 函数后 check-3 exit0。最终以上两轮发生在所有生产代码/测试源码更改之后。首次 dependency resolution 只新增两个本地冻结库包到 Cargo.lock；最终使用 locked/offline。

没有 cargo test/no-run/build/release 的本轮执行证据，**cargo check 不证明链接、测试运行或真实 Windows native 成功**。

## 源码测试清单 / 精确 CC filter

| filter（lib test 名的子串） | 项数 | 主要内容 |
|---|---:|---|
| `runtime::runtime::tests::multidisplay_contract_red::` | 原6，未改 | describe metadata/unit/full generation、primary selector、真实PNG resize后的mapping |
| `backend::display::tests::` | 新6 | 生产provider+内存帧的compose/负原点/竖屏/scale/gap/edge；ID失败；authority；源和PNG预算；capture变化无fallback |
| `runtime::runtime::tests::multidisplay_integration::` | 新11 | 严格selector零查询、describe无capture、非法目标零input、region/dedup、完整stale/reopen/resume、跨屏drag、途中query/change/cancel、cleanup失败、capture error与cache预算 |
| `feedback::display_tests::` | 新3 | 转发及错误；desktop surface/bounds/gap；永久Stop禁止open/resume |
| `mcp::bounded_output::tests::` | 新2 | 输出超限保留dispatch/cleanup/error；实际JSON转义字节限额 |
| `runtime::execute::tests::topology_guard_cannot_dispatch_after_input_deadline` | 新1 | 慢guard之后不越期inject，not_started/not_needed诚实 |

共 **23 新 + 6 原 = 29 项**，为源码静态计数，不是已通过数量。部分单测试内部循环覆盖多个失败/cleanup组合，不额外虚增 test 数。

上述 filters 没有真实桌面副作用：只构造 FakeBackend/MemoryDesktop/MemoryFrames，生产纯 mapping/compose engine 处理内存像素，input vector、release counter。没有 GdiProvider::attach、OS display 查询、真实输入、屏幕捕获、socket、进程、窗口或 renderer。deadline 新测试仅进程内约21ms sleep。不要据此推断全 root（特别是 ignored/native/其他owner新增tests）都无副作用。

## 交给 CC 的独立验证清单（以下未执行）

1. 核对本报告 SOURCE_FROZEN、source manifest、原6RED hash；确认没有其他 owner 并行改动同文件。
2. 只在**全新唯一** CC target 交叉链接 test exe；从本次 JSON 提取 target `rpa_computer` / profile.test=true / executable 的精确路径与SHA，不按glob捡旧exe。

```sh
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
CC_RUN="$(mktemp -d "$PWD/.agents/runs/windows-multidisplay-stageb-cc.XXXXXX")"
export CARGO_TARGET_DIR="$CC_RUN/target"
cargo test --target x86_64-pc-windows-msvc --locked --offline   --no-run --lib --message-format=json   > "$CC_RUN/compiler-artifacts.jsonl" 2> "$CC_RUN/build.log"
# CC 保存上条原始数字退出码，再由 JSON 精确取本轮 exe；此处不提供猜测 exe 路径。
```

3. 由 Windows runner owner 按既有有界流程复制/校验同一SHA到Windows新目录。逐个上表filter执行，仅Windows，不加ignored；保存stdout/stderr、numeric native exit、pass/fail/ignored、最终状态。单次运行形状如下（路径和filter由CC从本次artifact及上表填写）：

```powershell
& '<本次哈希核对后的精确lib测试exe>' '<上表精确filter>' --test-threads=1 --nocapture
$code = $LASTEXITCODE
# 按既有runner协议原样保存数字退出码；不得把stderr空/启动成功当GREEN。
exit $code
```

4. 分开验收原6RED转GREEN和新23 seam tests；再跑受影响原有 runtime/session/execute/feedback/worker/MCP/remote 纯回归。全root结果区分focus或其它owner的intentional RED，不混作本任务通过/失败证据。
5. release如需产物，CC同样用新target构建 `--release --bins`，保存本次精确exe hash；不要覆盖18:09等任何旧fixed产物。本报告没有release hash。
6. fixture/renderer/focus/取消barrier各由独立owner/CC按其冻结清单验收；无需等待它们才编译本任务，不改其runner。
7. 真实多屏GUI必须协调授权后另排：真实enumeration和physical units、混合DPI/负坐标/竖屏/gap、单屏ID与desktop真实PNG、热插拔/旋转中途drag停止/释放、实际cleanup failure、Stop不能reopen/resume、capture exclusion。纯mock/renderer selftests不得替代这些证据。

## 已知边界 / 仍未验证

- Windows仅类型编译通过，未链接新产物、未执行23/6/root tests、未做任何真实GDI/input/GUI验证。
- 同步provider调用不可抢占；既有worker quarantine保留；采样式generation检查不能证明瞬间变化又恢复未发生，尚无OS topology通知订阅。
- 预算是分层allocation/admission，不是RSS/延迟实测上限；额外decode/cache/base64/JSON副本明确存在。
- generation opaque token当前依赖冻结库完整Debug格式；相同tracker内稳定，不作为持久格式承诺。后续如需库正式序列化API应协调库owner，不在root越权改crate。
- selected display过大源frame或不合法layout会明确失败；不承诺所有硬件组合都可捕获。GDI HDR/protected/secure desktop/overlay等库限制不变。
- Mac/Linux保持primary-only显式限制且暂停测试；Mac capture crate本轮未接入。反馈窗口截图排除依然要真实证据，requested不算verified。
- 没有发现必须越权修改冻结库才能继续的具体编译阻断；这不是库在实机无缺陷的声明。

## 冻结记录

`source-freeze.sha256` 包含33个本轮源码/文档文件以及原6RED文件（34条，原6RED只是pin，不是本轮修改）。

- Source manifest SHA256：`946e8ef40adc542e613c6f099458183db0cb2558508e87e962466b4a536582d9`
- Compile evidence manifest SHA256：`b1beb5c5841464f0e0e515b70426eb808fd9baea56d5cb340fb984eea5465fe2`
- 原6RED文件 SHA256：`79c30152b52c3f742cf974f81c5a2471e10539391dbb967ed796eee3c26cb3e1`

两个manifest均在本轮runs目录；每文件hash可独立核对。本报告自身hash另写 `report.sha256`，避免自引用。交接后停止源码写入，等协调者/CC反馈。
