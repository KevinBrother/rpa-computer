# rpa-windows-display

独立 Windows 显示器 backend；仅实现 Windows 原生枚举和截图，portable 部分供纯测试/合成使用。自己的 `[workspace]`、lockfile、target；依赖冻结的 `../display-topology`，不修改它，也不依赖/接线 root Host、renderer、fixture、输入库。

**真实 OS 路径已写入并做 Windows target 编译检查，但没有执行过本任务的测试或真实枚举/截图。** 不宣称双屏、截图排除、root 接线已验收。默认测试无 OS 枚举/DPI/GDI/桌面/网络/子进程调用，没有 live 或 ignored 用例。测试执行交 CC+GLM，先 Windows，Mac/Linux 测试不安排。

## 公开 API

| API | 契约 |
| --- | --- |
| `attach(MemoryBudget) -> Result<WindowsDisplay>` | 验证调用进程和当前线程**均为 PMv2**，不设置 process DPI、不安装 thread override、不枚举或截图。保持实例在创建线程。 |
| `verify_pmv2()` | 仅核验；不符合时返回含 process/thread 结果的 `DpiContextMismatch`。无法取得上下文则报告 API 错误。 |
| `WindowsDisplay` | `DisplayBackend<GdiProvider>` 别名。真实 provider 是 `!Send + !Sync`；没有默认 mock/fallback。 |
| `DisplayBackend::new(provider, memory)` | 仅构造纯状态，建立长期 `TopologyTracker`；不调用 provider 或修改 DPI。允许纯 frame provider 测试。 |
| `DisplayBackend::snapshot()` | 完整真实枚举 → ASCII/≤128 ID 校验 → topology 的 `PhysicalPixels` 快照/代次。异常不返回旧快照。 |
| `DisplayBackend::observe(selection, CaptureBudget)` | 枚举、plan、全部源/峰值内存预检、逐源 capture/合成、PNG 编码、再次完整枚举/代次比较；任一步失败不返回 partial desktop。 |
| `CapturedObservation` | 同一代次的 `topology`、`plan`、`mapping`、真实组合 `rgba` 与精确尺寸 `png`。不是缓存主屏伪装 Desktop。 |
| `FrameProvider::{enumerate,capture}` | 信任边界内 provider seam。真实实现核对身份和 permit 后才能分配；测试提供内存帧，不 fake Win32。 |
| `GdiProvider::attach()` | 核验 PMv2 的底层真实 provider。实际调用 enumerate/capture 时再次核验；不设置任何系统显示设置。 |
| `SourcePermit::{size,bytes}` | engine 发出的私有字段、只读授权，真实 GDI 在 native DIB 分配前核对。最多一个 DIB 和一个返回 RGBA 源缓冲。 |
| `MemoryBudget::new(source_bytes,total_bytes)` | 显式非零单源/峰值图像 payload 限制，与 final `CaptureBudget` 同时生效。没有隐藏“无限大”默认值。 |
| `RgbaFrame::{new,size,pixels,into_pixels}` | 验证长度的 packed top-down RGBA8，宽×高×4 checked 后才接受。 |
| `RgbaFrame::from_bgra32(...,RowOrder)` | 检查 tight stride/长度，原位 BGRA→RGBA、alpha=255；可原位翻转 bottom-up。真实 GDI 始终申请 top-down。 |
| `estimate_encoded_size(PixelSize) -> EncodedSize` | 只读无分配，复用 encoder 的 encoded_len，返回 PNG bytes 与 padded base64 bytes；不含 JSON/MCP/TLS overhead，不自行执行传输准入。 |
| `DisplayError` | 保留 topology 结构化错误、Win32/HRESULT operation/code、DPI 不符、native 身份变化、预算/overflow、帧格式、allocation 和 cleanup 等原因。 |

非 Windows 上纯逻辑可编译，真实 `attach/verify_pmv2/GdiProvider` 明确 `UnsupportedPlatform`，不生成假截图。

## 真实枚举与身份

- 使用官方 `windows 0.58`：`EnumDisplayMonitors(NULL,NULL)`、`GetMonitorInfoW(MONITORINFOEXW)`、`EnumDisplayDevicesW`、`EnumDisplaySettingsExW(ENUM_CURRENT_SETTINGS)`、`GetScaleFactorForMonitor`。
- 完整处理当前 active monitors；绑定 HMONITOR、adapter device name/ID、active monitor interface IDs（`EDD_GET_DEVICE_INTERFACE_NAME`）。不保存可盲用的悬挂 handle。每个源截图前再次完整枚举，按 runtime ID 找到完全相同 descriptor 才使用其全局物理 bounds。
- 原生 bounds 与 `DEVMODE` 的位置/物理宽高必须相符；要求有效 `DM_POSITION/DM_PELSWIDTH/DM_PELSHEIGHT/DM_DISPLAYORIENTATION`。source size 是这些物理像素，不做 `width*scale`。
- scale 来自成功的 `GetScaleFactorForMonitor` 百分比，**不是推算 raw DPI**；HRESULT 失败绝不采用该 API 的默认输出，也不退 scale=1。Windows reported scale 变化进入精确 topology facts。
- Windows `DMDO_*` 是**逆时针**；topology `Rotation` 是**顺时针**，因此 DMDO_90→Deg270，DMDO_270→Deg90。只转换事实，已在全局显示轴方向排列的 GDI 源不再旋转。
- ID 为 `win-gdi-<16位十六进制计数>`，ASCII 24 字符。按完整 identity 排序、精确比较，不短 hash；同一 provider lifetime 内枚举顺序变化不改 ID。已观察到拔插/身份改变则分配新 ID；任何枚举错误使旧 active identity token 退休，恢复后不复活旧 handle token。计数 checked，不能跨 provider/Host 重启持久复用。
- 空列表、重复 handle/设备、缺失/多个主屏、错误/缺失 mode、scale、interface identity 都明确拒绝。镜像 pseudo-driver 不支持；普通 Desktop 原生 overlap/mirror 按 topology 错误拒绝，不 silent primary。单次枚举上限 64 monitors、256 adapter/interface records，超过返回 `EnumerationLimit`，不截断列表当成功。

## GDI 与资源生命周期

选择 `GetDC(NULL)` + `CreateDIBSection` + `CreateCompatibleDC` + `SelectObject` + `BitBlt(SRCCOPY|CAPTUREBLT)` + `GdiFlush`，不是旧 screenshots primary capture 路径，也不启动外部命令。

1. 先核验 PMv2、来源 identity/mode 和 source permit；LONG/DWORD 边界必须适合 GDI。
2. 创建 32-bit `BI_RGB`、负 `biHeight` 的 top-down DIB。`GetObjectW(DIBSECTION)` 核对 actual bitmap 尺寸、packed stride、32-bit/plane、数据 pointer、header 格式和实际字节数。
3. 初始化 DIB，避免读取未定义初始 storage；按 monitor 的全局物理 x/y（可负）执行 BitBlt。没有 DPI 二次乘法。
4. GdiFlush 成功后归还 old selected object，再复制有界 DIB 到唯一源缓冲，原位 BGRA/BGRX→RGBA，undefined alpha 统一 255。
5. RAII release 顺序：恢复 selection → DeleteDC(memory) → DeleteObject(DIB) → ReleaseDC(screen)。错误分支也按逆序析构；bitmap 在 memory DC 前创建，保证恢复失败时先销毁 DC，再删 bitmap。stock old object 只归还不 delete。
6. 正常返回前显式检查全部 cleanup；任何失败不返回成功观察。失败路径 Drop 尽力归还，不能承诺 OS cleanup 调用本身永不失败；Host 仍需隔离/故障处置。

所有 unsafe 仅在 `src/native/**`，使用官方 windows-rs ABI；pure 模块 `deny(unsafe_code)`。`unsafe_op_in_unsafe_fn` 也为 deny，FFI callback 内指针访问有局部安全说明，callback 不分配内存。

## 合成、像素及内存预算

### 唯一 plan/mapping

`rpa-display-topology` 的长期 tracker、selection、CapturePlan、tile rect 和 ObservationMapping 是唯一几何来源。显式副屏不存在不 fallback。

逐 tile 对**整源**独立 nearest-neighbour 采样直接写入目标 canvas；不分配完整 resized tile，不跨屏滤波，不把全图缩小后仅重贴 metadata。gap/量化外侧 padding 为确定的 opaque black `[0,0,0,255]`，映射仍拒绝点击。最终 PNG 就是 plan.size，没有后续任意 crop/letterbox/整体滤波。

捕获全部选中源后编码 PNG，然后再次完整枚举与 generation exact 比较。源错误、缺屏、尺寸不匹配、枚举失败或代次变化都丢弃整次暂存 pixels/PNG，不返回部分图像、不重用旧源。

### 可核算的峰值

`C = canvas.width * canvas.height * 4`；每个源 `S = width * height * 4`。所有乘法/转 usize/地址长度先 checked，**全部选中源在第一次 OS capture 前预检**。

- 每个源要求 `S <= max_source_bytes`，即使 final PNG 已被缩小也不豁免真实源预算。
- capture 峰值：`C + 2*S`（canvas + native DIB + CPU RGBA）。顺序处理并释放源，不同时保留所有 source buffers。
- encode 峰值：`C + P`（canvas + PNG）。没有全图 filtered buffer、压缩结果副本或每行堆分配。
- 两阶段峰值均不得超过 `max_total_bytes`，包括 PNG 编码阶段；`try_reserve_exact` 分配失败返回结构化错误。
- 该预算明确限制本库管理的**图像 payload**，不是 OS process RSS：不包含 allocator bookkeeping/rounding、GDI/driver 内部开销、descriptor metadata、调用者保存的旧 observations 或 transport/base64。第三方 provider 必须遵守 permit，恶意 provider 不属于沙箱保证。

### PNG 路径的明确取舍

为了不对第三方 PNG/compressor 内部临时 Vec 做未经证明的内存承诺，生产代码使用小型有界 RGBA8 PNG encoder：filter=None、zlib stored DEFLATE blocks、CRC32/Adler32、一个 IDAT；`image 0.24` **仅 dev-dependency**，用于 CC 纯测试独立解码和像素校验。

设 `F=C+height`、`B=ceil(F/65535)`，编码后精确 `P=57+2+F+5*B+4`。预先限制 IDAT≤`2^31-1`，预留 P；中间不复制整图。它是合法无损 PNG，但**不做压缩率优化，文件通常接近 RGBA 大小**，CPU 编码时间/传输体积尚未实测。Host 必须另设输出/base64预算；将来替换压缩器必须重新证明峰值内存协议，不能只换 metadata。

### MCP / TLS message 预算是独立的必须接线项

`max_pixels` / `max_source_bytes` / `max_total_bytes` **不足以保证消息可发送**。本库没有隐式默认图像大小；若 root 采用 README 示例的 1920×1440 上限并确实输出该尺寸，stored PNG 体积如下（这些是公式计算，不是性能测试）：

| 实际输出尺寸 | PNG 精确 bytes | padded base64 精确 ASCII bytes | 尚未包含 |
| --- | ---: | ---: | --- |
| 1920×1080 | 8,296,178 | 11,061,572 | JSON envelope/metadata、framing、TLS overhead |
| 1920×1440 | 11,061,548 | 14,748,732 | 同左 |

纯接口 `estimate_encoded_size(plan.size())` 返回同一 encoder 的 `EncodedSize { png_bytes, base64_bytes }`；`base64_bytes = checked(4 * ceil(png_bytes/3))`，含标准 padding，不含引号/data-URL prefix。这避免 root 再复制一套 PNG 长度算法。

Root 接线必须：

1. 在 plan 确定后先估算，加入该 MCP reply 的 JSON envelope、regions/topology metadata、文本和传输 framing，满足**真实已配置的 message/frame 限制**；必要时降低 `CaptureBudget` 并重新生成 plan（可能触发 tiny-tile/gap 拒绝），或直接报 output-budget 错误。不能只捕获后等远端断开。
2. 预算以协议明确的字节层为准：应用层 JSON/frame 上限与 TLS ciphertext/framing 上限不能混为一谈；TLS record 分段能力也不代表 MCP 单条 message 无上限。
3. `observe` 会重新枚举，预估 plan 可能不再是最终 plan；收到结果后，在**分配 base64/序列化之前**，用实际 `png.len()` 与 `observation.plan.size()` 再核验，再对最终 serialized message 实际字节数做硬门禁。预检查不是绕过最终长度检查的理由。
4. 把 base64、JSON serialization、transport copies 的额外峰值内存记入 Host 预算；本库的 `C+P` 未包括它们。不要无上限把 PNG/base64/完整 JSON 同时 clone。
5. 若 root 要求“传输预算拒绝发生在任何 OS capture 之前”的强保证，后续 owner 应把输出准入谓词/limit 带进同一次 `observe` 的 plan preflight，或绑定并消费同一个不可变 plan；当前只读 estimator **不宣称已完成该原子传输准入接口**。

当前未接 root 的 MCP/TLS message gate，也未压测 PNG encoder、CPU 时间、网络吞吐、超时或内存 RSS。这里没有“默认图像性能已验收”的结论。

## Root owner 后续串行接线（本任务不修改）

root 当前冻结，以下只是下一 owner 的接入说明，不是本任务已实施的改动：

```toml
# Root owner 在 CC/root 当前阶段结束后再加入适合的平台依赖表。
rpa-windows-display = { path = "crates/windows-display" }
rpa-display-topology = { path = "crates/display-topology" }
```

```rust
use rpa_display_topology::{CaptureBudget, Selection, ImagePoint};
use rpa_windows_display::{attach, WindowsDisplay, MemoryBudget, DisplayError};

// 由 Host 在长期 Windows capture worker 线程准备/manifest PMv2。
// 此方法不创建 GUI，不替 Host 修正 DPI；不符合就失败。
fn attach_prepared_worker() -> Result<WindowsDisplay, DisplayError> {
    attach(MemoryBudget::new(64 * 1024 * 1024, 192 * 1024 * 1024)?)
}

fn observe_then_resolve(backend: &mut WindowsDisplay) -> Result<(), DisplayError> {
    let observation = backend.observe(
        Selection::Desktop,
        CaptureBudget::new(1920, 1440, 2_764_800)?,
    )?;
    // 保存 observation 的 plan/mapping/generation 绑定；输出 png 与 regions。
    // 真实动作前再次枚举；枚举失败同样禁止注入，不能复用旧快照。
    let current = backend.snapshot()?;
    let _native = observation.mapping.map_image(
        ImagePoint { x: 10.0, y: 10.0 }, current.generation(),
    )?; // 如坐标是 gap，明确拒绝；示例不注入任何输入。
    Ok(())
}
```

- **保持实例长期存活**，不得每次 observe 新建 tracker 或只比较 revision。用 session/observation opaque token 绑定完整 mapping；替换旧 Geometry/CoordMap 单宽高比，不靠旧字段假装 Desktop。
- root Runtime 继续管理 drag 每个 move 的代次核验、时间、取消、释放、partial/unknown 报告；本库没有输入注入或可取消 driver deadline。
- **renderer 集成阻塞项：现有 Windows.cs 要求 surface 严格等于某单屏 bounds。Desktop 组合区域不能直接按单屏 surface 传入。后续必须由 root/renderer owners 联合串行定义逐屏 presentation/surface 映射；本任务未修改 renderer，也不宣称 Desktop 反馈已可用。**
- 本库不设置、不验证 capture exclusion affinity，不截图后涂抹。上层 `win_affinity_requested` 只能表示请求，不是 GUI 排除通过证据。

## 未验收/不承诺

- 本任务没有真实 Windows 枚举、GDI capture、双屏/混合 DPI/旋转/热拔插/跨屏 drag 或截图排除运行证据；root 和 renderer 尚未接线。
- GDI 同步可能阻塞；库没有可取消 deadline。Host 的 worker/process 隔离、超时、quarantine 属后续职责；CPU 合成/PNG 也没有取消回调。
- 前后完整枚举不能证明两次枚举之间未发生又恢复的瞬时变化，也不产生同一时刻的多屏原子画面；真实源按顺序截图。若 Host 需要严格热插拔事件失效，还须订阅 OS display notifications。
- 不创建窗口/改显示设置/自动设置 DPI；PMv2 尚未准备、invalid mode/interface、pseudo mirror 等明确失败。当前 API 需要支持 `GetDpiAwarenessContextForProcess` 的 Windows 版本（微软文档 minimum client Windows 10 1803）。
- 8-bit GDI 路径不承诺 HDR/ICC 色彩保真、protected/secure desktop/硬件 overlay/独占全屏可见性或捕获鼠标光标。BitBlt 成功不等于这些内容实际可见；不以 black pixels 伪造权限成功的验收结论。

## 编译与 CC 测试交接（Windows only）

实现者仅执行 format、Windows target all-targets **check**，以及 non-Windows `--lib` 条件编译检查（不是 Mac 测试）；具体原始输出见自身报告。没有执行 cargo test、SSH、GUI、截图或 test-exe。

CC 使用既有 cargo-xwin/rust-lld 配置，不改源码，先只编译：

```sh
cd /Volumes/doc/workspace/datagrand/rpa/rpa-computer
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/windows-display-cc.XXXXXX")"
eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"
export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export AR_x86_64_pc_windows_msvc="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
export ARFLAGS_x86_64_pc_windows_msvc='-flavor link /lib'
cargo test --manifest-path crates/windows-display/Cargo.toml \
  --target x86_64-pc-windows-msvc --no-run --offline --message-format=json \
  > "$RUN_DIR/compiler-artifacts.jsonl" 2> "$RUN_DIR/build.log"
```

从本次 Cargo JSON `compiler-artifact` / `profile.test=true` / `.exe` 提取三个精确 target：`rpa_windows_display`、`pipeline`、`pixels`，不要从目录随意取旧 exe。逐个计算 SHA256、复制到 `acer-win` 新唯一 TEMP 目录并核对远端 SHA256，再仅由 CC 执行：

```powershell
& '<本次精确测试exe路径>' --test-threads=1 --nocapture
$code = $LASTEXITCODE
if ($code -ne 0) { exit $code }
```

三份 exe 都是纯测试，不调用 `attach/verify_pmv2` 的真实 Windows 查询，也不调用 `GdiProvider` 的 OS 枚举/截图；不需要交互 Scheduled Task，不启动 Host/renderer/RPAD/fixture，不加 `--ignored` 或 `--include-ignored`。分别保存 cross-build 退出码、哈希、目标机每份 test-exe stdout/stderr/退出码与 pass/failed/ignored 数；编译通过不是目标机运行通过。真实枚举/截图另需协调者授权的后续 Windows 验收任务。

## 官方 API / 绑定核对依据

实际源码核查：缓存官方 `windows-0.58.0/src/Windows/Win32/Graphics/Gdi/mod.rs`、`UI/HiDpi/mod.rs`、`UI/Shell/mod.rs` / `Shell/Common/mod.rs`、`UI/WindowsAndMessaging/mod.rs`。例如 EnumDisplayMonitors/EnumDisplaySettingsExW/GetDC 等位于 `Graphics::Gdi`，GetScaleFactorForMonitor 位于 `UI::Shell`，MONITORINFOF_PRIMARY/EDD_GET_DEVICE_INTERFACE_NAME 常量位于 `WindowsAndMessaging`；本任务按真实 binding 编译，不手写 extern ABI。

本轮实际读取的官方说明（关键语义，不复制第三方实现）：

- [GetDpiAwarenessContextForProcess](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiawarenesscontextforprocess)：NULL 查询当前进程；与 thread query 联合核验。
- [GetThreadDpiAwarenessContext](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getthreaddpiawarenesscontext)：当前线程的实际 DPI 上下文。
- [EnumDisplayMonitors](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaymonitors)：NULL DC / clip 完整枚举，包含需明确处理的 pseudo monitors。
- [EnumDisplayDevicesW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaydevicesw)：adapter/monitor 两阶段、active flags、interface-name 标志。
- [EnumDisplaySettingsExW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaysettingsexw)：不参与 DPI virtualization，返回物理像素。
- [DEVMODEW](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-devmodew)：dmFields 有效性，DMDO 旋转明确为逆时针。
- [GetScaleFactorForMonitor](https://learn.microsoft.com/en-us/windows/win32/api/shellscalingapi/nf-shellscalingapi-getscalefactorformonitor)：成功才使用百分比；失败时的默认输出不能冒充真实 scale。
- [CreateDIBSection](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-createdibsection)：bitmap/CPU pointer 生命周期；访问 GDI 写过的 bits 前需要 GdiFlush；无 ICM。
- [GetDIBits](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-getdibits)：核对 top-down 与 selected-bitmap 限制后，本实现选择直接受控 DIB，不调用 GetDIBits。
- [PNG specification](https://www.w3.org/TR/png-3/)、[RFC 1951](https://www.rfc-editor.org/rfc/rfc1951.txt)：PNG chunk、CRC、zlib stored-block 编码规则；实现未拷贝第三方 encoder。
