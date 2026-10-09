# 独立多显示器拓扑 / 区域映射基础库实现报告

日期：2026-09-30。任务：`.agents/tasks/display-topology-core-sol-20260930.md`。
工作目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。

## 角色、写集与真实状态

- 本批交付源码 / 测试代码 / 必要本地编译；测试执行、独立验收仍交 **CC+GLM**。
- 已完整读取任务、多屏设计 §4、`multi-01..12` 规范；只读现有 `Geometry` / `CoordMap` 及已有 Windows 构建记录以确定接口和交接命令。
- 只写新独占目录 `crates/display-topology/**` 与本报告。不修改 root Cargo.toml / Cargo.lock / src、native-input、macos-capture、feedback、renderer、fixture。
- 不 worktree、commit、push、恢复脏树、启动其他 agent；没有测试执行、GUI 输入、截图、SSH、服务启动或 Windows cross-build。
- 用户最新平台顺序：**先 Windows**；本地 Mac 只有允许的编译检查，没有 Mac 测试，Linux 测试也不安排。完整多屏设计未缩小。
- **编译通过不等于测试通过；所有 22 个新纯测试运行结果仍 pending。没有多屏端到端 / 真机验收结论。**

## 交付文件

| 文件 | 内容 |
| --- | --- |
| `crates/display-topology/Cargo.toml` | 独立 `[workspace]`，包 `rpa-display-topology`，仅 std，无外部依赖。 |
| `crates/display-topology/Cargo.lock` | 自身 lockfile，只包含自身包；不是 root lockfile。 |
| `crates/display-topology/.gitignore` | 仅忽略该 crate 的 `/target/`，防止编译缓存混入交付。 |
| `crates/display-topology/src/lib.rs` | 公开 API 导出、unsafe 禁止、missing_docs 编译约束。 |
| `crates/display-topology/src/geometry.rs` | native/image/pixel 类型，半开边界、宽整数 checked 算术。 |
| `crates/display-topology/src/error.rs` | 具名结构化 topology / selection / budget / mapping / drag diagnostics。 |
| `crates/display-topology/src/topology.rs` | Descriptor、显式单位 / rotation / selection，精确比较 tracker、不可变快照与 checked generation。 |
| `crates/display-topology/src/capture.rs` | 预算化单屏 / Desktop 计划、统一密度、显式 tiles、gap/overlap 诊断，源尺寸与代次校验接口。 |
| `crates/display-topology/src/mapping.rs` | 实际 PNG + letterbox region 缩放，逐屏 image/native 映射，gap/padding/stale 拒绝。 |
| `crates/display-topology/src/drag.rs` | 所有用户 waypoint 整体预检、可跨 gap 的内部 native segment 采样，无时序 / driver / sleep。 |
| `crates/display-topology/tests/support/mod.rs` | 数据构造辅助；不产出 expected。 |
| `crates/display-topology/tests/topology.rs` | 4 个独立拓扑 / selection 测试。 |
| `crates/display-topology/tests/capture.rs` | 7 个独立 capture / budget / gap / DPI 测试。 |
| `crates/display-topology/tests/mapping_drag.rs` | 10 个独立 mapping / padding / 旧代次 / drag 测试。 |
| `crates/display-topology/README.md` | 公开 API、真实像素合成协议、backend/Runtime 接线职责、Windows 优先待 CC 命令与限制。 |
| `.agents/reports/display-topology-core-sol-20260930.md` | 本报告。 |

另有 `src/topology.rs` 库内 1 个 generation 耗尽 / 事务不提交测试。总计 **21 integration + 1 unit = 22 个测试已编写，未运行**。最大产品源码文件 411 行；测试拆为独立主题，无千行单文件。

## 关键设计与任务对应

| 要求 | 实现与边界 |
| --- | --- |
| Selection 不 fallback | 缺失显式 ID 返回 `DisplayNotFound`；空列表、重复 ID、无主屏、多主屏分别报错。 |
| 原生 bounds 与截图尺寸分离 | descriptor/tile 同时保存 native bounds、source pixel size；调用方明确 Points / PhysicalPixels，Windows 必须是 PMv2。scale 不用来猜像素尺寸。 |
| Generation 精确事实比较 | 按不透明 ID 排序，比较所有字段及 scale bits；单纯枚举顺序不递增。坐标/尺寸/capture/scale/rotation/primary/插拔/单位变化 checked 递增，非法更新/耗尽不提交。 |
| Tracker lifetime | token 含进程内独立 tracker identity + revision；不可只比较 revision，重新创建 tracker 不会冒充旧 generation。跨 Host 重启禁止复用观察。 |
| 混合 DPI Desktop | 统一 native 布局密度，逐 tile 保留真实 source size + native bounds + composite pixel rect；不是整个截图单宽高比。低 DPI 可能上采样；单屏保留自身像素优先。 |
| 像素预算与稀疏布局 | 构造预算非零与 checked product，宽/高/pixels 三约束整数二分，最多 32 轮。不分配像素、不按稀疏原生面积遍历。源字节预算仍由真实 backend 验证。 |
| 零 tile / overlap / 镜像 | Desktop 原生 overlap/镜像明确 `OverlappingDisplays`；量化零 tile `CollapsedRegion`，正 separating gap 消失 `CollapsedGap`，不会静默删屏/抹 gap。明确单屏选择在镜像下允许。 |
| 实际观察 mapping | PNG 解码宽高 + 可选 content rect，严格 rescale 每屏边缘；非法输入、非有限、gap、padding、未选 native 点、旧 generation 都拒绝。合法点最后 floor / 末位防越界不逃到邻屏。 |
| 合成协议不假完成 | 必须真正逐源 capture、验证 decoded source size、整源独立缩放到最终 region，tile filter 不跨界；不能任意全图滤波/裁剪后只修改 metadata。发布前复查 generation 与最终 PNG。 |
| 合法跨屏 drag | 用户指定的每个 waypoint 必须命中屏幕；合法跨屏端点允许，`DragPlan::sample` 内部可经过 gap。click/native mapper 仍拒绝 gap。每次采样要求最新 generation；Runtime 时序/取消/释放未接线。 |

测试代码覆盖负左原点/右屏/上下及 L 形空白、混合 DPI、边缘 round 越屏风险、实际尺寸/letterbox、gap 消失诊断、缺屏不回退、旧代次、精确浮点最低位、排序、拔插/rotation/primary/unit 变更、极大坐标/预算乘法溢出、合法跨屏 image/native drag 与用户 gap waypoint 拒绝。expected 使用手写值，不用 mapper 或 plan 自算预期。源码及测试无 GUI/OS API/网络/SSH/子进程调用。

## 允许的开发编译证据（非验收）

本地编译工具链：

```text
rustc 1.98.1 (48a229cea 2026-09-01)
cargo 1.98.1 (797e8a9bc 2026-08-05)
host: aarch64-apple-darwin
```

最终源码状态执行：

```sh
cargo fmt --manifest-path crates/display-topology/Cargo.toml
cargo fmt --manifest-path crates/display-topology/Cargo.toml -- --check
cargo check --manifest-path crates/display-topology/Cargo.toml --all-targets --offline
```

format check exit **0**，无输出。cargo check exit **0**，完整输出：

```text
    Checking rpa-display-topology v0.1.0 (/Volumes/doc/workspace/datagrand/rpa/rpa-computer/crates/display-topology)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
```

`--all-targets` 编译检查 lib、库内测试及三个 integration test 的类型/语法，**不链接并执行测试**。本任务未执行 `cargo test`、doctest、clippy、GUI 或 Windows 编译；不把编译检查称为验收。git 状态限定查询显示该新目录文件均为新增未提交；编译缓存由目录内 `.gitignore` 排除。已有脏树与并行 agent 文件未恢复/改写。

## 待 CC+GLM：Windows 优先命令

完整过程在 crate README。先使用本机既有 cargo-xwin / rust-lld，仅 cross-build 测试 exe：

```sh
cd /Volumes/doc/workspace/datagrand/rpa/rpa-computer
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/display-topology-windows-cc.XXXXXX")"
eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"
export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export AR_x86_64_pc_windows_msvc="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
export ARFLAGS_x86_64_pc_windows_msvc='-flavor link /lib'
cargo test --manifest-path crates/display-topology/Cargo.toml \
  --target x86_64-pc-windows-msvc --no-run --offline \
  --message-format=json > "$RUN_DIR/compiler-artifacts.jsonl" 2> "$RUN_DIR/build.log"
```

**上述命令没有执行。** CC 核对真实 exit 和日志后，从本次 JSON `compiler-artifact` / `profile.test=true` / `.exe` 提取四个 target：`rpa_display_topology`、`topology`、`capture`、`mapping_drag`。不能 glob 挑旧 exe。逐个本地 SHA256，上传 `acer-win` 新唯一 TEMP 专用目录，核对远端 `Get-FileHash`，再逐个执行：

```powershell
& '<本次精确exe路径>' --test-threads=1 --nocapture
$exit = $LASTEXITCODE
if ($exit -ne 0) { exit $exit }
```

CC 保存四份 stdout/stderr、exit、pass/fail/ignored 数、文件 hash 和目标路径；不加 `--ignored` / `--include-ignored`，不启动 Host/fixture，不改显示设置。纯测试可直接 SSH 执行，不需交互 Scheduled Task。cross-build 成功和 Windows 目标机执行成功必须分开报告；任何失败交回实现者，不能自动修产品/改预期。

Mac/Linux 测试顺序已暂缓；不列本地 `cargo test` 作为本轮建议执行项。

## 尚未接线 / 验收限制

1. 没有真实 Windows/macOS/Linux display enumeration，不从旧主屏 Geometry 伪造枚举列表。
2. 没有真实逐屏 capture、rotation source 朝向验证、像素合成/过滤/编码解码接线；metadata 本身不证明这些事实。
3. 没有 root Host/Runtime selection、generation、跨屏输入节拍或取消/清理接线；这些属于下一任务的独占写集。
4. Desktop overlap/mirror、量化后不可表达的小 tile/gap 明确拒绝；不缩小为“自动主屏”。需要提高预算/明确单屏，或未来定义独立无歧义协议。
5. 没有任何本批纯测试执行证据，更没有 `multi-01..12` 双屏真机/拖拽证明。Windows 纯测试第一轮、之后真实双屏环境与其他平台验证分别仍 pending。
