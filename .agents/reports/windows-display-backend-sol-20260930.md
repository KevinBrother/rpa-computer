# Windows display backend 实现冻结交接

日期：2026-09-30。任务：`.agents/tasks/windows-display-backend-sol-20260930.md`。
状态：**源码、测试源码、文档交付冻结；Windows target 编译检查通过；测试/真实 Windows 运行仍待 CC+GLM。**

## 1. 边界与执行事实

- 只写 `crates/windows-display/**` 和本报告。
- 未修改 root Cargo/src、冻结的 `display-topology`、native-input、macos-capture、fixture 或 renderer；root 接线交 owner 后续串行处理。
- 没有执行任何测试、test-exe、GUI/截图/注入、SSH、服务启动；没有新 agent、worktree、commit/push/reset/恢复脏树。
- Windows only。开发检查包括 Windows target all-targets check 和非 Windows `--lib` 条件编译检查；后者不是 Mac 测试，也没有新增 Mac/Linux backend。
- 用户告知旧 topology Windows CC 已 22/22 通过；本报告不将该成绩转移为新 Windows backend 的运行证据。
- 当前无已知剩余编译阻塞，**不需编译协助**。尚未做 Windows 链接/no-run test-exe 构建，留 CC 使用既有 cargo-xwin。

## 2. 精确交付路径

| 路径 | 职责 |
| --- | --- |
| `crates/windows-display/Cargo.toml` | 独立 workspace，path 依赖冻结 topology；Windows 官方 windows 0.58；image 0.24 仅测试独立解码。 |
| `crates/windows-display/Cargo.lock` | 自身依赖锁，无 root lock 改动。 |
| `crates/windows-display/.gitignore` | 排除自身 `/target/`。 |
| `crates/windows-display/src/lib.rs` | 公开 API、WindowsDisplay/attach、unsafe/missing_docs 编译边界。 |
| `crates/windows-display/src/error.rs` | 结构化 native/HRESULT/topology/DPI/预算/像素/cleanup 错误。 |
| `crates/windows-display/src/dpi.rs` | process+thread PMv2 共同满足的纯判定。 |
| `crates/windows-display/src/mode.rs` | 官方 mode 的物理尺寸/负原点/scale/rotation 事实验证；CCW→CW 显式转换。 |
| `crates/windows-display/src/identity.rs` | 精确 HMONITOR/device/adapter/interface identity → 有界 ASCII opaque token；无短 hash。 |
| `crates/windows-display/src/owner.rs` | 真实资源使用的单 owner RAII primitive 和纯生命周期测试。 |
| `crates/windows-display/src/native/mod.rs` | 真正 process/thread DPI 查询、GdiProvider、每源前重新枚举/身份核验，!Send/!Sync。 |
| `crates/windows-display/src/native/enumerate.rs` | EnumDisplayMonitors/GetMonitorInfo/EnumDisplayDevices/EnumDisplaySettingsEx/GetScaleFactorForMonitor 真实枚举。 |
| `crates/windows-display/src/native/capture.rs` | screen HDC + owned top-down DIB + memory HDC、SelectObject/BitBlt/GdiFlush、actual DIB 验证与 checked cleanup。 |
| `crates/windows-display/src/frame.rs` | packed RGBA、BGRA/top-down/alpha 转换、长度/stride/地址溢出检查。 |
| `crates/windows-display/src/budget.rs` | 全源预检、source permit、canvas+2×source / canvas+PNG 峰值预算。 |
| `crates/windows-display/src/engine.rs` | 长期 topology tracker、逐 tile 独立合成、捕获前后完整枚举、结果整体提交/丢弃。 |
| `crates/windows-display/src/png.rs` | 精确大小的 RGBA8 PNG stored-DEFLATE encoder，无隐式整图压缩暂存。 |
| `crates/windows-display/src/output.rs` | **只读 `estimate_encoded_size`**，同一 encoded_len 来源的 PNG/base64 bytes。 |
| `crates/windows-display/src/unsupported.rs` | 非 Windows 明确 UnsupportedPlatform，不假 capture。 |
| `crates/windows-display/tests/pipeline.rs` | 15 个 provider/合成/PNG/拓扑前后变更/预算/失败回退测试源码。 |
| `crates/windows-display/tests/pixels.rs` | 5 个颜色/行序/stride/overflow/PNG+base64 精确预算测试源码。 |
| `crates/windows-display/README.md` | 完整 API、官方依据、ROOT 后续接线示例、内存和消息预算、限制、CC 命令。 |
| `.agents/reports/windows-display-backend-sol-20260930.md` | 本报告。 |

库内纯测试：identity 4、mode 3、owner 3、DPI 1，共 11。合计 **31 个测试源码**（11 unit + 15 pipeline + 5 pixels），**均未执行**。无 live/ignored 用例；最大产品源码文件 242 行，最大测试文件 419 行。

## 3. 关键能力与安全条件

### 真实 Windows，不是新 mock 库

默认 `attach()` 连接真实 `GdiProvider`，constructor 不改 DPI、不枚举截图；要求已有 process 与 thread 实际 PMv2。旧 backend 的 `width*scale` 不迁移到新实现。Windows 原生 bounds、mode size 和 GDI 源统一 physical pixels；scale 仅记录成功获取的 OS 百分比事实。

真实枚举完整 active monitor 集合，检查 primary、mode 有效字段、物理位置/尺寸一致性、rotation/scale/interface identity。不用失败时的默认 scale，不造第二屏，不 silent primary。完整 identity 排序精确比较，runtime token 为 24 字符 ASCII；观察到断开、身份变更或枚举失败恢复时不复活旧 handle token。计数 checked。

截图前每源重新枚举核对 identity 和完整 descriptor，避免盲用旧 HMONITOR。GDI source 用全局物理坐标（包括负 origin），申请 top-down 32-bit BI_RGB；GetObjectW 核验实际尺寸/stride/format/pointer/字节数，GdiFlush 后复制，BGRA/BGRX→opaque RGBA。不能把 DPI 再乘到来源像素上。

原生资源恢复 old selection → 删除 memory DC → 删除 DIB → 释放 screen DC；错误分支 RAII 同序尽力释放，正常返回前显式检查全部清理结果。unsafe 局限在 `src/native/**`。

### plan/mapping 与预算

仅调用冻结 topology 的 Tracker/Selection/CapturePlan/ObservationMapping，不复制布局/映射公式。用户指定副屏消失报错。最终 canvas 由逐 tile 独立 nearest-neighbour 缩放整源形成，gap/padding 为 opaque black，映射拒绝其点击；没有 whole-canvas filter 后只改 metadata。

全部真实源在第一次 capture 前做 source/total byte 预检，final pixel budget 不替代 source budget。控制的图像 payload 峰值为 `C+2*S` 或 `C+P`；顺序释放源，PNG 无整图 filtered/compressed 中间副本。预算不是 process RSS，不涵盖 OS/driver/allocator bookkeeping、调用方历史 observation 或 transport/base64 copies。

完成全部 capture/encode 后再次完整枚举比较代次；失败不发布部分图像或陈旧 PNG。DPI/rotation/primary/位置/分辨率/身份等变化均进入 topology 的 exact facts。

## 4. 官方核查要点

已只读核查缓存 Microsoft `windows-0.58.0` 的实际 API/feature/union 类型及在线 Microsoft Learn 官方说明；完整链接列在 README：

- `GetDpiAwarenessContextForProcess(NULL)` 查询当前进程；必须另查 `GetThreadDpiAwarenessContext`，不能只通过 process 就忽略 thread override。
- `EnumDisplaySettingsExW` 不参与 DPI virtualization，返回 physical pixels。
- **DEVMODE 的 DMDO 值逆时针；topology Rotation 顺时针**，DMDO_90→Deg270 / DMDO_270→Deg90。已按官方文字修正映射并写手工 expected 的测试源码，不重复旋转实际 GDI 图像。
- `GetScaleFactorForMonitor` HRESULT 失败仍可能提供默认输出，本库只使用成功结果。
- `CreateDIBSection` 直接内存与 GDI 访问需要 GdiFlush 同步；HDC/HBITMAP 生命周期使用对应 ReleaseDC/DeleteDC/DeleteObject。
- 核查 GetDIBits 的 top-down/selected-bitmap 限制后，选择直接受控 DIB + pointer copy，而不是违规读取仍选中的 bitmap。
- PNG/W3C 与 RFC1951 的 chunk、CRC、stored block 规则；无第三方 encoder 代码拷贝。

## 5. 编译原始结果（不代表测试或链接通过）

开发命令：

```sh
cargo fmt --manifest-path crates/windows-display/Cargo.toml
cargo fmt --manifest-path crates/windows-display/Cargo.toml -- --check
cargo check --manifest-path crates/windows-display/Cargo.toml --target x86_64-pc-windows-msvc --all-targets --offline
cargo check --manifest-path crates/windows-display/Cargo.toml --lib --offline
```

最终 format check exit **0**、无输出。最终 Windows check exit **0**，完整输出：

```text
    Checking rpa-windows-display v0.1.0 (/Volumes/doc/workspace/datagrand/rpa/rpa-computer/crates/windows-display)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.93s
```

最终 non-Windows lib 条件编译 check exit **0**：

```text
    Checking rpa-windows-display v0.1.0 (/Volumes/doc/workspace/datagrand/rpa/rpa-computer/crates/windows-display)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

Windows `--all-targets` 覆盖 native 真实代码和全部测试源码类型检查，但**不链接 test-exe、不运行测试、不调用 Windows API**。未执行 cargo test/cross-link/GUI。首次 Windows check 离线锁定 37 个依赖包，使用 windows 0.58.0、image 0.24.9；生产 PNG/composite 不依赖 image。

只读 `cargo metadata --manifest-path crates/windows-display/Cargo.toml --no-deps --format-version 1 --offline` 输出确认：

```text
workspace_root: /Volumes/doc/workspace/datagrand/rpa/rpa-computer/crates/windows-display
target_directory: /Volumes/doc/workspace/datagrand/rpa/rpa-computer/crates/windows-display/target
workspace_members: 1
targets: [('rpa_windows_display', ['lib']), ('pipeline', ['test']), ('pixels', ['test'])]
```

没有借 root workspace 编译或修改 root/冻结 topology 文件；自身 target 已忽略。

## 6. 协调明确的两项接线阻塞

### renderer single-surface 限制

按协调者提供的当前事实，`Windows.cs` 要求 surface 严格等于某单屏 bounds。Desktop composite 不能直接塞入该单屏协议。后续 root/renderer owners 必须联合串行定义逐屏 surface/presentation；本任务未修改 renderer，也不把 Desktop screenshot 计划当成多屏反馈已接通。

### PNG/base64 与 MCP/TLS message gate

本轮 `png.rs` 使用 stored DEFLATE（无压缩率优化），PNG 通常接近原始 RGBA 体积。新增只读 `estimate_encoded_size(PixelSize) -> EncodedSize`，与生产 encoder 共用 encoded_len，给出精确 `png_bytes` 和 `base64_bytes`，无像素分配或截图。

设 C 为 RGBA bytes、F=C+height、B=ceil(F/65535)：`PNG=57+2+F+5*B+4`；`base64=checked(4*ceil(PNG/3))`。

| 实际尺寸 | PNG bytes | base64 bytes，不含 JSON |
| --- | ---: | ---: |
| 1920×1080 | 8,296,178 | 11,061,572 |
| 1920×1440 | 11,061,548 | 14,748,732 |

这些是**公式估算，不是性能运行结果**。本 crate 没有默认图像尺寸；若 root 使用 README 的 1920×1440 示例上限并确实输出该尺寸，就需承担约 14.75 MB（十进制）base64 payload，还未包含 envelope/metadata/framing/TLS overhead。

Root 必须按实际 MCP/TLS/application-frame 限制预检估算值 + JSON/metadata/framing，必要时收紧 CaptureBudget 或拒绝；最终 PNG 返回后、分配 base64 之前重检实际 `png.len()`，最终 serialization 后还需 message 字节数硬门禁。TLS record 可分段不代表 MCP message 无限制。Host 还需单独计入 PNG/base64/JSON/transport 副本内存。

**当前 estimator 不是与 capture 同事务的传输准入 gate**；若 owner 要求拒绝发生在任何 OS capture 前，后续应把 output limit/准入谓词带入同一次 observe plan preflight，或绑定并消费同一 immutable plan。此任务未改 root/MCP/TLS，不宣称性能、吞吐、CPU deadline 或编码耗时已验收。

## 7. 待 CC+GLM 执行：Windows only

完整命令和安全边界见 README；以下均未由本实现者执行：

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

从本次 JSON compiler-artifact、profile.test=true、非空 .exe 提取 `rpa_windows_display`（11 unit）、`pipeline`（15）、`pixels`（5）三份 exe，不 glob 旧产物。分别记录本地 SHA256、上传 acer-win 新唯一 TEMP 目录、核对远端 SHA256，再由 CC 执行并保存每份原始 stdout/stderr/exit：

```powershell
& '<本次精确测试exe路径>' --test-threads=1 --nocapture
$code = $LASTEXITCODE
if ($code -ne 0) { exit $code }
```

纯测试不要求交互 Scheduled Task；不启动 Host/renderer/fixture/RPAD、不枚举/截图/改显示配置、不加 --ignored/--include-ignored。交叉编译/链接与 Windows 目标机实际运行分别记结果。失败交回源码实现者，不自行弱化测试或预期。

## 8. 冻结时未验收边界

- 31 个新测试运行结果全部 pending；先在 Windows 由 CC+GLM 验证。旧 topology 22/22 不覆盖此库。
- 真实 Windows attach/enumeration/GDI screenshot、资源释放实机情况、混合 DPI/旋转/热插拔、多屏布局/capture/drag 均未运行验证。
- source/size/identity 正确性依赖实机 Win32 返回契约；GDI 成功并不证明 protected/secure desktop、overlay、独占全屏、HDR/ICC、鼠标光标均可捕获。库不设置/验证 affinity；`win_affinity_requested` 只能表示上层请求，不是截图排除通过证据。
- 前后枚举不是多屏同一时刻原子截图，也不能发现中间发生又恢复的未采样瞬态变化；更强保证需 Host OS notifications/隔离设计。
- GDI、合成、PNG 同步执行，没有可取消 deadline。Host timeout/isolation/quarantine、Runtime input 时序/取消/清理、transport gate、renderer Desktop surface 协议都未接线。
- 本任务实现范围内无已知待补项；冻结后等待 CC 编译/纯测发现的具体缺陷，不扩写 root/renderer 或其他平台。

## 9. SOURCE_FROZEN — 2026-09-30

当前有界实现冻结；不继续扩大功能。无实现/compile 阻塞，不需编译协助。

交接前再次执行以下检查，整个命令 exit 0；fmt 无输出，Windows check 原始输出为 `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.08s`（缓存检查）：

```sh
cargo fmt --manifest-path crates/windows-display/Cargo.toml -- --check
cargo check --manifest-path crates/windows-display/Cargo.toml --target x86_64-pc-windows-msvc --all-targets --offline
```

未执行测试、cross-link、SSH、GUI 或截图。31 个测试的运行结果仍为 pending。

CC 执行以 `.agents/tasks/windows-display-backend-cc-20260930.md` 为准：使用其指定的已验证 build-env、新 CARGO_TARGET_DIR、`--locked --tests --no-run`，仅交叉生成 Windows exe，再在 acer-win 跑纯测试；第 7 节为参考，不替代 CC 任务的证据/超时要求。

### 冻结 SHA-256

包含本 crate 全部 21 个非 target 文件（源码、测试、README、Cargo.toml/Cargo.lock、.gitignore）；不包含本报告，避免自引用。以下清单按路径排序，每行 `SHA256 + 两空格 + 相对路径 + LF`；清单整体 SHA-256：

`8119844f82ac1229c3a45536042b61b528547cf1298b177533cc6597db1366b7`

```text
306fd52e74fca6746e12acc750f232de15e71da917756a1270d74c91f5eb7368  crates/windows-display/.gitignore
5fb7f711c6f2364531a8060e3b6d4bc06bfc4862e61b8d5ff6ef86312b6c38f1  crates/windows-display/Cargo.lock
98be6275610be3a53ce7333c23ba22b960954554b218e9e45f65fe7c3985ae52  crates/windows-display/Cargo.toml
7894e8e2cd9306ac63062c8ae60a0c0660f12bf7af7ca1347d01cf3aa3e9abe9  crates/windows-display/README.md
f519c67a998c69ba49f81d284402e424b503ad46f5471743e7741e1a3d25bad6  crates/windows-display/src/budget.rs
87cbbcce0a3ad62e3f1c0cc107effc6e077aba7caa5499a1f919cb5d54a7b564  crates/windows-display/src/dpi.rs
5938b51fca0970dd64716f1ea189479931ab60d3b9c99f44bddf9f1b3a6ccd4f  crates/windows-display/src/engine.rs
0e8fece20e66a70c02d3eadb3e4d19931723ebdeed87ec9514cea66438e48827  crates/windows-display/src/error.rs
d5e2897b19f70c36f0fcbe35366861d69837a16dca45ac12d1f4b239129ea24b  crates/windows-display/src/frame.rs
7d548b03a2941f7d24ddd4bec989bd57ac7a100e5fa3b926e39972eb589d56ee  crates/windows-display/src/identity.rs
259736bbb1da26409bf771e001aec0f6bf6560ef2699c2a93a04e8edcf7bf211  crates/windows-display/src/lib.rs
c45570a19e081843d2011721a201f824478b1a32303c5dc8b530aa5e4979977c  crates/windows-display/src/mode.rs
1a20bb93981a0df4212cc0a4dd8423a3c83a18090162eea0710dac30966b530d  crates/windows-display/src/native/capture.rs
f920b0ca03292533f57105c2acdc8cdc1c6e8d91313cbc61084b6fa00ff7ba06  crates/windows-display/src/native/enumerate.rs
674ffec204de0cdd1c15dd600e3fb3fcd40f20771f1bce44d4353eb8668de88f  crates/windows-display/src/native/mod.rs
c711da2bbbb0b04a5ff1f7baa1f88a35b883d142e6c2f22bd421708313743fe9  crates/windows-display/src/output.rs
1606652c03412a6a9232c50b6db5e1592249f74c7c4a8d2fb363986dda3a224e  crates/windows-display/src/owner.rs
0b3112c778a606644092d45f65aeb2d39c098198ff6f18b9b139134853c7d7ce  crates/windows-display/src/png.rs
255d8e2ab5fc2b2eb17a475ebaf952553cedf5e574539e830858c62f88b0f800  crates/windows-display/src/unsupported.rs
cff322b70eee87b3fccfad85669be0b563c6b8281c71d73d6673e8e6db581041  crates/windows-display/tests/pipeline.rs
8cf2a84b14b0a7ce7059858f1e86cb473caf89c4bb17c42cfcff6e45c76a24d9  crates/windows-display/tests/pixels.rs
```

CC 构建和运行前后应逐文件比对上述 SHA；本报告最终 SHA 另在交接消息给出。root、renderer、MCP/TLS 准入、真实多屏/GDI/性能验收仍按第 6、8 节留给后续 owner，本冻结不宣称其完成。
