# Windows multidisplay — Task 1 baseline RED 测试源码交接

日期：2026-09-30。实现：Codex；真实 Windows 执行：CC+GLM。

## 状态 / 停止点

**仅 Task 1 测试源码已完成、Windows compile-check exit 0，源码冻结。未运行测试，真实 RED 尚待 CC。Task 2–4 未开始，等待明确 GO。**

已完整读取多屏 integration plan、`.agents/CONTRACT.md` 末尾 multiscreen amendment、`crates/display-topology/README.md` 和 `crates/windows-display/README.md`。依据现有 `Runtime::new/call`、`FakeBackend::new/set_geometry`、`tool_definitions` 和内存 PNG API 编写测试。

没有生产 stub、没有修改生产实现/testutil/Cargo、没有接入新库来掩盖 baseline RED。没有执行 tests、应用 CLI、Host、SSH、GUI、Mac/Linux 测试；没有 commit/push/worktree/reset；没有改动已有 fixed exe/release。B2 fixture、renderer、feedback v1 wire 均未修改，保留 `Surface.id = "desktop"` 仅用于 fullDesktop 的约束。

用户告知的 baseline 244/0/6、portable/native0、windows-display31 green 是先前 CC 证据，本报告不把它们计作本次执行结果。

## 精确源码写集

1. 新增 `src/runtime/runtime/tests/multidisplay_contract_red.rs`，229 行、6 个测试。
2. `src/runtime/runtime/tests.rs` 仅末尾追加如下注册，原有 bytes 不变：

```rust
// Windows-only Task 1 contract tests; no native backend or production changes.
#[cfg(target_os = "windows")]
mod multidisplay_contract_red;
```

3. 本报告 `.agents/reports/windows-multidisplay-red-sol-20260930.md`。

编译输出/只读前后 hash/注册前 bytes 留在独立忽略目录 `.agents/runs/windows-multidisplay-red-sol-20260930/`。该目录不是 fixed artifact 部署目录。

对本轮开始时的 `src/`、库/renderer/fixture/test 源码和 Cargo 文件只读 hash 快照做比对，结束时唯一改变的既有文件是上述 `tests.rs`；新文件只在本任务指定测试模块。注册文件原有前缀逐字节一致。

## 六项代表性契约与预期 baseline 首个失败点

以下是**源码推断，未宣称真实运行 RED**。所有断言都要求未来应有的正确行为，没有 `#[should_panic]`、ignore、skip、“预期缺失所以通过”等反向处理。

统一精确模块 filter：

```text
runtime::runtime::tests::multidisplay_contract_red::
```

每项完整 test name 为上述前缀加下表函数名；单项执行加 `--exact`。

| 测试函数 | 正确行为 | 原 baseline 预期首个失败点 |
|---|---|---|
| `describe_lists_backend_display_facts_not_static_primary` | describe 给出 backend 所提供显示器的实际 ID、主屏标记、带负原点 native bounds；不得硬编码默认 primary 或捏造第二屏 | `displays` 缺失，不是 array |
| `describe_declares_native_unit_separately_from_image_coordinates` | image 坐标系与 native unit 独立声明 | `native_unit` 缺失 |
| `describe_full_generation_is_stable_but_distinguishes_fresh_authorities` | 同一 authority 不变事实保持 token；全新独立 Runtime/backend 即使 geometry/version 相同也不得共享完整 generation | 第一次 describe 的 `topology_generation` 缺失 |
| `explicit_primary_is_accepted_by_runtime_and_advertised_open_schema` | Runtime 接受 `display:{kind:"primary"}`，且公开 schema 真的包含 optional、kind 区分、拒绝额外字段的 primary 分支；保留 8 tools | Runtime 当前会忽略 display 并成功，但 schema 没有 `properties.display`；测试在这个缺口失败，避免“忽略未知字段”的假绿 |
| `observe_reports_selected_region_using_actual_resized_png_and_native_bounds` | 实际 PNG downscale 为160×90；region覆盖该图，native bounds仍为640×360、原点(-640,120)，并列出选中ID | 旧观察PNG/width/height本应先通过，随后 `mapping_regions` 缺失失败 |
| `observe_generation_matches_describe_and_survives_unchanged_recapture` | describe/两次观察/后续describe的完整 generation一致；不同 observation ID不能冒充不同topology generation；单位一致 | 第一张观察的 `topology_generation` 缺失失败，不接受 null==null |

预期 filter 选中 **6 项、6 项失败**，通常 libtest native exit **101**；这些数量/退出码只是假设，必须由 CC 原始结果确认。若提前因 PNG/既有 open 路径等失败，应报告真实首因，不能直接记为预期契约 RED。若选中 0 项也不是成功。

### 本批测试使用的 metadata 投影

现有 amendment 锁定语义，未逐字定义新增 JSON key。为使测试可执行，本模块明确采用如下候选字段拼写，供后续 root owner 实现时统一审阅，**不是声称生产 baseline 已有这些字段**：

- describe：`displays[]`（`id`、`is_primary`、`native_bounds:{x,y,width,height}`）、`native_unit`、`topology_generation`。
- observation：`topology_generation`、`native_unit`、`selected_display_ids[]`、`mapping_regions[]`（`display_id`、`image_rect:{x,y,width,height}`、`native_bounds`）。
- 完整 generation 接受非空 opaque string 或 object；不锁定内部 token 算法或 tracker 字段拼写。通过重复读取相等、独立 authority 不等验证语义，不允许裸数字 revision。单次 opaque 非空本身不证明完整性，所以必须结合跨 authority 比较。
- 当前 FakeBackend 的平台是 `fake`，旧 API 没有 native unit authority。本批只要求明确合法的 `points`/`physical_pixels`，**不根据 Windows 编译 target 伪称 fake 是真实 Windows physical pixels**。实际 Windows backend 必须 physical_pixels 的断言应在 GO 后新枚举接口的测试中补充。

字段命名若需协调，应在 Task 2 GO 前明确；不得通过删除语义断言或加入生产返回 stub 把本轮 RED 变成表面 green。

## Mock / 无桌面副作用说明及覆盖限制

- 唯一 backend 是现有 `crate::runtime::testutil::FakeBackend`，通过原有 `set_geometry` 设置一个合成显示器事实：ID `mock-display-negative-origin`、bounds(-640,120,640,360)，内存PNG源320×180。
- 没有 `DesktopBackend`、WindowsDisplay/GdiProvider、native attach/DPI/GDI、OS枚举、输入注入、renderer、worker、进程、网络或文件fixture读取调用。
- 只调用 describe/open/observe/close；没有 computer_step。PNG encode/decode/resize 在内存执行；close 的 release_all 也只进入 FakeBackend 的内存记录，不会调用真实 release。
- 使用 `Runtime::new`，不是 feedback 构造路径；无可选 renderer 启动。带会话的测试在预期新契约断言前显式 close。
- 当前旧 Backend API 只能提供单个 Geometry；本批代表性测试验证“backend事实必须通过Runtime发布”及新schema/metadata缺口，**并不能真实表示或验证两个显示器的完整枚举**，更不构成多屏GUI证据。没有为测试新增虚假的多屏生产接口。
- 多屏实际列表、单位的真实 Windows 来源、tracker revision变化、失败枚举不可用、gap/padding拒绝、热拔插、drag逐步generation、消息预算和 fullDesktop反馈仍属于 Task 2–4及后续验证，不在本轮抢跑。

## 已执行：仅 Windows compile-check

独立输出目录，不改旧产物：

```bash
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR="$PWD/.agents/runs/windows-multidisplay-red-sol-20260930/target"
cargo check --target x86_64-pc-windows-msvc --tests --locked
```

结果：**exit 0，Finished dev profile，13.39s**。日志：

- `.agents/runs/windows-multidisplay-red-sol-20260930/windows-check.log`
- `.agents/runs/windows-multidisplay-red-sol-20260930/windows-check.exit`

该检查证明 Windows 测试源码可编译，未链接/运行本次 root 测试 exe，不证明 baseline 真实 RED 或任何测试通过。

## 交 CC：编译和 Windows 真实 RED（待执行）

使用新唯一 run 目录。构建机仅 cross-build，本实现者没有执行以下命令：

```bash
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
RUN_DIR="$(mktemp -d "$PWD/.agents/runs/windows-multidisplay-red-cc-20260930.XXXXXX")"
export CARGO_TARGET_DIR="$RUN_DIR/target"
cargo test --target x86_64-pc-windows-msvc --locked --no-run --lib \
  --message-format=json > "$RUN_DIR/compiler-artifacts.jsonl" 2> "$RUN_DIR/build.log"
# 记录真实构建退出码。根据本次 compiler-artifact/profile.test=true/
# target.name=rpa_computer/executable 精确选 lib test exe，不 glob 选旧产物。
```

CC 将该精确 exe/hash 部署到新 Windows 目录，校验SHA256，只运行本模块，不运行应用 CLI/Host、其他 suite 或 ignored tests。该filter无native调用，不需要桌面交互槽。

```powershell
# $TestExe 是本次 JSON 精确选出且已核对哈希的 Windows lib test exe。
# $RunDir 是 CC 新建的 Windows 证据目录；不要指向此前 fixed/red 目录。
& $TestExe 'runtime::runtime::tests::multidisplay_contract_red::' `
  --test-threads=1 --nocapture `
  1> "$RunDir\multidisplay-red-stdout.log" 2> "$RunDir\multidisplay-red-stderr.log"
$nativeExit = $LASTEXITCODE
$nativeExit | Set-Content "$RunDir\multidisplay-red-native-exit.txt"
# 预期非零；保存首次原始日志，不因RED覆盖源或重试抹掉首败。
```

单项复核精确 filter 示例：

```powershell
& $TestExe --exact `
  'runtime::runtime::tests::multidisplay_contract_red::explicit_primary_is_accepted_by_runtime_and_advertised_open_schema' `
  --test-threads=1 --nocapture
```

完整六项 exact filters 另存于本任务 runs 目录 `exact-test-filters.txt`；所有命令由 CC 决定并执行。CC 必须记录实际选中数、每项首败、通过/失败/ignored数、native exit、exe及源码hash。真实 RED 后由协调者给 Task 2–4 GO；本实现者到此停止。

## 冻结 SHA256

```text
bb8a325981f6ed5f5596f8502fdd2aa3453ee6752339aebbeb938895290bdba5  src/runtime/runtime/tests.rs
79c30152b52c3f742cf974f81c5a2471e10539391dbb967ed796eee3c26cb3e1  src/runtime/runtime/tests/multidisplay_contract_red.rs

```
