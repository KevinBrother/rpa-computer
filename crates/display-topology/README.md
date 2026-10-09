# rpa-display-topology

独立纯 Rust 显示器拓扑、预算化截图计划与逐屏坐标映射基础库。仅依赖 `std`，自己的 `[workspace]` / lockfile，不依赖 Host、MCP、GUI、截图或 native-input。

**这是逻辑基础库，不是多屏端到端完成。** 当前没有接入真实枚举、逐屏 capture、像素合成或 Runtime。纯测试不证明双屏真机可用，也不能把现有主屏静态截图称为多屏实现。

## 公开 API

所有公开 Rust 项目附 rustdoc；crate 启用 `deny(missing_docs)` / `forbid(unsafe_code)`。

| API | 职责与安全边界 |
| --- | --- |
| `DisplayDescriptor` | 运行期不透明 `id`、主屏、`NativeRect`、实际 `capture_size`、有限正 `scale`、`Rotation`。库不解析 ID，不承诺重插稳定。 |
| `NativeUnit::{Points, PhysicalPixels}` | 调用方明确选择统一原生输入单位。macOS 通常 points；Windows 必须先建立 PMv2 awareness 再提供 physical pixels。库不猜系统，不混用单位。 |
| `TopologyTracker::new/update/latest` | 校验非空列表、ID、唯一主屏、尺寸、原生范围、scale；按 ID 排序后逐字段精确比较，包括 scale 的 `to_bits()`，不是短 hash。仅完整事实变化时 checked 增代。失败不提交状态。 |
| `TopologySnapshot::{generation, unit, displays}` | 不可修改的全部拓扑事实。快照可以包含镜像；安全选择/合成能力由 plan 明确判断。 |
| `Generation::revision` | 首次成功枚举为 1。token 同时包含不可伪造的进程内 tracker identity；不能只比较 revision。重建 tracker 不会复用旧 token。进程重启后不可持久复用 token/旧观察。 |
| `Selection::{Primary, Display(id), Desktop}` | 主屏、精确指定 ID、全桌面。显式 ID 缺失立即 `DisplayNotFound`，绝不 fallback。 |
| `CaptureBudget::new(max_width, max_height, max_pixels)` | 三项非零；checked 验证最大轴乘积，轴限制必须适合 u32；实际 plan 同时满足宽、高、总像素限制。单位是像素，不是字节。 |
| `CapturePlan::new` | 根据选择与预算产生不可修改的 canvas、逐屏 `CaptureTile`、可选 `DesktopLayout` 与 generation。只计算，不截图、不分配像素。 |
| `CapturePlan::require_generation/validate_source` | backend 在截图前/发布前核验最新 generation；逐源核验真正解码后的宽高。不是像素、显示器身份、rotation 的真实性验证。 |
| `CapturePlan::{size, tiles, generation, unit, selection, budget, desktop_layout}` | 只读公开合成协议；tiles 按 ID 排序。 |
| `ObservationMapping::new(plan, actual_size, content_rect)` | 基于实际解码 PNG 尺寸，重新量化每屏区域。`None` 为整图，`Some(PixelRect)` 为完整布局在 PNG 中的 letterbox 区域。不支持裁剪/重排。 |
| `ObservationMapping::map_image` | 检查最新 generation、有限值及 image 半开边界；拒绝 gap/padding，再逐屏映射到原生整数坐标。 |
| `ObservationMapping::map_native` | 原生用户点也必须命中 selection 中的某块真实屏；Desktop gap 不可点击。 |
| `ObservationMapping::{size, content_rect, regions, generation, unit}` | 可供 compositor/metadata 消费的真正观察区域。 |
| `DragPlan::from_image/from_native` | 整体预检每一个用户 waypoint；不能把用户 gap 点当内部插值接受。至少两个点。 |
| `DragPlan::{segments, generation, sample}` | 每个内部 sample 验证最新 generation；有理整数插值允许合法跨屏端点之间经过物理 gap。无 sleep/输入/时序/取消。 |
| `Error` | 具名结构化 variant 与上下文；generation、selection、坐标、预算、源尺寸、量化歧义分别诊断，无 silent fallback / 非法请求修正。 |

### 最小调用流程（示意）

```rust
use rpa_display_topology::*;

fn prepare(
    tracker: &mut TopologyTracker,
    enumerated: Vec<DisplayDescriptor>,
    unit: NativeUnit,
    actual_png_size: PixelSize,
) -> Result<(CapturePlan, ObservationMapping), Error> {
    let snapshot = tracker.update(unit, enumerated)?;
    let plan = CapturePlan::new(
        &snapshot,
        Selection::Desktop,
        CaptureBudget::new(1920, 1440, 2_764_800)?,
    )?;
    let mapping = ObservationMapping::new(&plan, actual_png_size, None)?;
    // Backend must really compose all selected sources into mapping.regions().
    // A separate fresh enumeration must agree before publishing the PNG.
    Ok((plan, mapping))
}
```

`actual_png_size` 必须来自真实编码/解码结果，不是预期输出尺寸；示例不执行任何截图。

## 几何、DPI 与 compositor 协议

1. 所有原生 bounds 与 pixel rect 都是**半开区间**。`NativeRect` 最后有效整数位置必须适合 i32，但 exclusive end 可为 `i32::MAX + 1`。全桌面 bounding span 可为 `2^32`，因此 `DesktopLayout` 用 u64；中间乘除用 u128，绝不把负原点 cast 成无符号再计算。
2. 单屏以该源实际像素宽高优先，预算够则完整保留；不足时预算化缩小。`native_bounds` 与 `source_size` 始终分开。scale/rotation 是拓扑事实，不能拿 `native * scale` 猜截图尺寸。
3. Desktop 优先 density 为所有屏 `source_width/native_width`、`source_height/native_height` 的最大值。因此低 DPI 屏可能上采样，不因此创造细节。最长原生轴为 `L`，选择最大的预算可容纳整数 `k`，统一 density = `k/L`。canvas 两轴为 `ceil(span*k/L)`，每块 tile 全部边缘为 `floor((native_edge-min_origin)*k/L)`。整数二分最多 32 轮；不按稀疏原生面积分配或遍历像素。
4. 统一 density 保留屏间相对位置/空隙，而每屏映射独立。共享边用同一 floor 规则；末端 ceiling 最多产生不足一个 density 单位的外侧 padding，padding 不可点击。正原生 separating gap 若量化为零像素，返回 `CollapsedGap`；任何零宽/高 tile 返回 `CollapsedRegion`。提高预算或选择单屏，不悄悄删屏/填 gap。
5. Desktop 原生重叠/镜像返回 `OverlappingDisplays`。显式单屏仍可用，因为单独的 source/native 映射没有歧义；这是用户明确的选择，不是自动退主屏。旋转源必须已在全局原生输入轴方向排列，库不旋转原图；枚举 scale/rotation/尺寸变化都会失效旧观察。
6. backend 必须对每个源真实捕获、检查 source ID/解码尺寸，将**整个已正确朝向的源图**独立重采样到相应 `composite_rect`，裁剪 filter 支持范围在 tile 内；空白 gap/padding 保持背景，不延伸屏内容、不覆盖邻屏。
7. 最终观察另有缩放/letterbox 时，`ObservationMapping` 将 plan 所有边缘按 `floor(edge*content_axis/plan_axis)+content_origin` 计算。backend 必须按 `mapping.regions()` **重新逐 tile 合成/缩放**整源图；不能全图任意滤波跨边界后仅更改 metadata。不同 rect 的舍入会改变局部比例，因此 mapping 采用最终 region 的真实宽高，而不是全图宽高比。裁剪/reflow/单独挪动 tile 不符合这个协议，必须另建计划。
8. 合成所有被选中的源才能发布，解码确认最终 PNG 宽高确实等于 mapping.size；`validate_source` 只验证一个源的宽高，不能证明 source 集合完整、真正抓到了屏或排除了反馈层。library 不分配 RGBA；backend 还必须 checked 验证 `width*height*channels`、stride、字节预算和 OS capture 错误。

## 坐标与 drag 的不同授权

`ImagePoint` 是连续 pixel-edge 坐标，合法范围 `[0,width) × [0,height)`。先判定有限值、边界及 region；再按当前 region 的各轴比例 **floor** 到 native unit，最后仅对已合法点的浮点末位量化做 `min(size-1)` 防越界。不能把越界/非有限/gap/未选屏点 clamp 成可点击目标。右/下边界属于下一 region 或 gap；不会因 round 到屏外/邻屏。`NativePoint` 是 i32 原生整数。

用户 image/native waypoints 全部预检后才能准备 drag 和按下按钮。两个不同屏的合法端点允许跨屏，`DragPlan::sample(segment, step, steps, current)` 才能产生 gap 中的**内部 dragged move**。`step=0` / `step=steps` 精确返回端点，中间按有符号有理数运算向起点量化。它不是 click mapper，不能拿 gap sample 再授权任意 click，也不能把用户指定的 gap waypoint 当合法插值。

Runtime 必须自己控制每个 move 的节拍/时长、取消、原生 dragged 语义和按钮释放。每次动作前及 drag 每个 move 前用真实最新拓扑代次检查；变化或枚举失败即停止并尽力释放，报告 partial/unknown 与清理结果。库无 driver/sleep，不声称已接好取消清理。

## 后续 backend / Host 接线

- 真实平台枚举适配器构造全部 descriptor，明确单位和 rotation/source 朝向；不能从现有单屏 `Geometry/CoordMap` 捏造第二个显示器。
- 一个持续的 tracker 作为拓扑 authority；observe 捕获前枚举，捕获/合成后再枚举并 `require_generation`，任何变化/错误丢弃这次观察。枚举失败时不能拿 `latest()` 当真实桌面仍有效的证据。
- 会话绑定明确 selection、完整 plan/mapping 和完整 `Generation`，协议序列化由 Host 完成。本库未添加 serde/network；不能只发送 revision 代替完整 session 生命周期绑定。
- source identity/尺寸校验、所有选中 source 完整合成、最终 PNG 解码一致性都由 backend 负责；Runtime 消费 region mapping，替换旧单屏总宽高比。
- 拔屏/DPI/rotation/primary/单位变化强制重观察；显式 ID 消失拒绝，不 fallback。跨屏 drag 变化停/清理。OS 输入支持、镜像限制、feedback capture exclusion 与双屏真机验证仍是后续任务。

## 测试交接：Windows 优先，执行者 CC+GLM

实现者只编写测试及执行 `cargo check --all-targets`，**没有运行任何测试，也没有 Windows cross-build、SSH、GUI 或截图**。当前按用户最新要求，Mac/Linux 测试暂不执行；完整设计不因此缩小。

纯测试分为 `tests/topology.rs`、`tests/capture.rs`、`tests/mapping_drag.rs`，以及 tracker exhaustion 的库内测试。它们只操作 std 数据，没有 OS API、GUI、截图、网络、SSH、子进程或 ignored 用例。expected 为手算常量，不调用待测 mapper/plan 自产 expected。

### CC 本机 cross-build（待执行）

使用已存在的 cargo-xwin/rust-lld 配置，参考仓库 `docs/remote-connection.md`。以下只是交接命令，未由实现者执行；缺工具/目标时报告环境阻塞，不擅自安装。

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
# 检查退出码与完整 build.log；--no-run 只构建，不是 Windows 测试通过。
```

从**本次** Cargo JSON 的 `reason=compiler-artifact`、`profile.test=true`、非空 `executable` 且 `.exe` 后缀提取目标，不 glob 随机找旧 exe。应有四个 target：`rpa_display_topology`（库内 exhaustion）、`topology`、`capture`、`mapping_drag`。逐个记录路径与 SHA256，构建日志也归档。

### CC 在 acer-win 上运行纯测试（待执行）

1. SSH `acer-win` 查询 `$env:TEMP`，创建新唯一目录 `display-topology-windows-cc-20260930-<nonce>`；只传上述四个本次 `.exe` 与所需运行依赖。此库仅 std；若 Windows 加载报缺依赖，报告真实缺项，不将编译成功当运行成功。
2. 本地 `shasum -a 256 <exe>`；远端 `Get-FileHash -Algorithm SHA256 -LiteralPath <exe>`，对四个文件逐一核对。
3. 每个 exe 用 PowerShell 原生执行，保存 stdout/stderr 与真实 `$LASTEXITCODE`；**不加 `--ignored` / `--include-ignored`**，不启动 Host、fixture、RPAD、Executor 或 GUI。

```powershell
# <本次路径> 从步骤1的精确产物清单填入，不从目录挑旧文件。
& '<本次路径>\rpa_display_topology-<本次hash>.exe' --test-threads=1 --nocapture
$exit = $LASTEXITCODE
if ($exit -ne 0) { exit $exit }
# topology-*.exe、capture-*.exe、mapping_drag-*.exe 同样逐一执行并保存各自 exit。
```

纯测试没有桌面操作，直接 SSH 执行即可；不要求交互 Scheduled Task，不更改用户显示设置。CC 报告必须分开记录 cross-build 与 Windows 真实执行的通过/失败/ignored 数、hash、路径、命令、原始日志。失败交回源码实现者修复。所有测试执行结论当前仍 pending；纯测试即使通过也不是 multi-01..12 真机验收。
