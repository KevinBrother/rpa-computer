# rpa-macos-capture

独立 Rust 库，提供 **macOS 14+ 原生 ScreenCaptureKit 过滤截图**。按 Host 提供的可信 renderer PID，排除该进程所属 `SCRunningApplication` 的全部窗口，再由 ImageIO 编码 PNG。

**交付状态：源码实现与开发编译检查，不是功能/GUI验收。** 根据 2026-09-30 最新优先级，先由协调者测试 Windows；本库的 Mac/ Linux 测试与 Host Mac 接线排后，不能阻塞 Windows。

不修改 root Cargo/src；不依赖 native-input、feedback 库或 renderer；不注入输入、不绘制/隐藏窗口、不涂抹截图、不更改 TCC、不监听网络。
`CaptureClient::new()` 只构造 Rust 状态，不访问屏幕、不预检权限、不创建线程、不弹窗。

## Host 接口（后续由 Host owner 接线）

```rust
use rpa_macos_capture::{CaptureClient, CaptureRequest};
use std::time::Duration;

// 缓存于 Host capture lane 的生命周期中，不要每次超时重建。
let mut client = CaptureClient::new();
// 以下仅为调用接口示例，本任务未执行任何 capture。
let image = client.capture(CaptureRequest {
    display_id: owned_display_id,             // u32，真实 CGDirectDisplayID
    excluded_process_id: owned_renderer_pid, // u32，从 Host 实际 child 获取
    width: output_pixels_width,              // u32，请求的输出像素而非 points
    height: output_pixels_height,
    timeout: Duration::from_secs(5),
})?;
// image.png: Vec<u8>; image.width/height: 实际 CGImage/PNG 一致的尺寸
```

- `CaptureClient::new() -> CaptureClient` 无 native 初始化副作用。
- `CaptureClient::capture(&mut self, CaptureRequest) -> Result<CapturedImage>` 同步有界等待；**不得在 UI/main thread 调用**，主线程调用返回 `InvalidRequest/Preflight`。
- `CaptureClient::is_busy() -> bool` 标识仍未确认完成的系统 flight，包括已 timeout 的 flight。
- `is_supported() -> bool` 仅检查 OS version；不预检 TCC、不截图。
- PID 是信任边界：只能来自 Host-owned、仍存活且通过 feedback 握手的 renderer child，不能从 Agent 请求传入。库不验证该 PID 的父子关系/授权，也不按名称、bundle ID 或窗口标题猜测 ownership。生命周期与 PID 复用问题由持有 child 的 Host owner 管理。
- 关闭反馈模式仍使用原有 capture 路径；不要在 off 模式为本库做 discovery/permission 请求。

## 真实 API 与 availability

1. Foundation 检查 OS>=14.0；旧版本返回 `Unsupported/Availability`，不退回旧截图。
2. CoreGraphics `CGPreflightScreenCaptureAccess()`；false 直接 `PermissionDenied/Preflight`，没有调用 `CGRequestScreenCaptureAccess`。
3. `SCShareableContent.getShareableContentExcludingDesktopWindows(false, false, completion)` 获取可捕获内容。
4. 只读 `SCDisplay.displayID` 和 `SCRunningApplication.processID`，唯一匹配 requested display 与 exact PID；匹配缺失或歧义失败关闭。
5. `SCContentFilter.init(display:excludingApplications:exceptingWindows:)`：只传该 exact PID 的单一 application，`exceptingWindows=[]`，不将 renderer 窗口作为 exception 重新包含。
6. `SCStreamConfiguration` 设置输出 width/height、scalesToFit；不采集音频、不包含全局光标；不裁切/绘制像素。
7. `SCScreenshotManager.captureImage(contentFilter:configuration:completionHandler:)` 抓一帧（该 API macOS14.0 起）。
8. ImageIO `CGImageDestination` 原生编码 PNG；从 CGImage 读取实际尺寸、stride 并验证 PNG IHDR 尺寸一致。

所用声明全部来自维护者发布的 `objc2`/`objc2-*`；没有手写 selector 或 C FFI 签名。
参见 [API调查与安全边界](docs-api.md)。

本组件的支持基线为 macOS14，不提供旧 capture fallback。维护者 ScreenCaptureKit binding 是 framework 链接；若后续 Host 需要发布到尚未包含该 framework 的系统，Host packaging 必须处理可选链接/部署边界，不能只凭运行时 availability guard 宣称整包可在所有旧 macOS 启动。
本次没有修改原有 Host 部署基线或 off 模式。

## Deadline、迟到回调与 flight 有界性

- 校验开始即创建一个绝对 deadline，discovery、截图、编码与交付共用，不给第二阶段重置 timeout。
- 调用线程只通过 Mutex/Condvar 等待结果；没有每次 `spawn` 专属线程，没有 stream 常驻 worker。
- 每 client 仅一个 flight。timeout 后保留该 flight，直到 callback 明确 complete；后续请求先返回 `Busy`，不再调用权限/discovery/capture。
- callback 永不回到达时，该 client 始终 Busy，保留至多一个 flight；没有 reset/unpoison 接口。
- 必须复用 client；不要在 Busy/Timeout 后循环创建新 client，因为 ScreenshotManager 没有本实现可调用的 cancellation API。
- 所有 escaping `RcBlock` 只持有 heap-owned Arc/Retained/Copy request，不持有 client、栈变量或栈 channel 引用。丢弃 client 后迟到回调仍有自身 Arc。
- discovery 和 image completion 各有 claim gate，意外重复回调不会再发第二次 capture 或并发编码；first completion 不被覆盖。
- 超时回调不会启动下一阶段/编码，也不把图片返回给 caller；截图派发与 caller timeout 的竞态最多仍是同一个已保留 flight，不能派生无限 capture。
- 原生 `CGImage` 仅在其回调有效期内同步编码；不把未 retained 的借用 image 指针送到等待线程。
- filter/configuration/content/exclusion arrays 被 screenshot block retained 到完成/释放；dispatch 后不改 configuration。回调有 autorelease pool，Rust panic 边界转静态错误。
- 当编码超出 budget 时 caller 仍可在 deadline 返回 Timeout；native encoder 无强制取消，但 client 保持 Busy 到 encode/callback 完成。不承诺硬实时调度或强制中断 OS encoder。

## 资源预算与实际 PNG

| 项目 | 上限 / 规则 |
|---|---|
| width / height | 1..16384 |
| width × height | ≤33554432 pixels，乘积以u64校验 |
| timeout | >0、≤30秒 |
| PID | 1..i32::MAX，匹配原生 pid_t |
| shareable displays / apps | ≤256 / 16384，否则 CaptureFailed |
| 实际 CGImage raster | stride×height checked_mul，≤MAX_RAW_BYTES（256MiB+1MiB） |
| PNG | ≤MAX_PNG_BYTES（128MiB+1MiB） |

request 尺寸是请求值，不是结果承诺；输出 `width/height` 必须来自实际 CGImage 并等于 PNG IHDR。允许平台返回不同的、仍在预算内的实际尺寸，Host 必须据此计算截图映射，不能贴上请求尺寸。

ImageIO 直接处理 native CGImage 的 color space、stride 和方向；不手工拆 BGRA、不翻行、不用透明像素或区域修补隐藏 overlay。PNG header guard 是可信 ImageIO encoder 的后置检查，不是任意不可信 PNG 文件的完整解码器。
实际色彩、方向和过滤效果尚未验收。

## 结构化错误

`CaptureError { kind, stage, native_domain, native_code }`，实现 `std::error::Error`。`code()` 为静态字符串；error/Debug 不含 native localizedDescription/userInfo/app/window 信息。CapturedImage Debug 只显示尺寸与字节数，不 dump PNG。

| kind | code | Host 含义 |
|---|---|---|
| Unsupported | capture_unsupported | 非macOS或有效API不可用；不能降级成含反馈截图 |
| PermissionDenied | capture_permission_denied | preflight失败、SCK UserDeclined/MissingEntitlements |
| NoDisplay | capture_no_display | selected display缺失，或SCK NoDisplayList/NoCaptureSource |
| ExcludedProcessMissing | capture_excluded_process_missing | 无法确认要排除的renderer app；不改成空过滤 |
| InvalidRequest | capture_invalid_request | 尺寸/PID/timeout越界或main-thread调用 |
| Busy | capture_busy | 上一系统flight仍未完成，不应继续派生新client |
| Timeout | capture_timeout | 总deadline耗尽；可能仍有未完成flight |
| CaptureFailed | capture_failed | 歧义、null result、其它native error、内部bridge错误 |
| EncodingFailed | capture_encoding_failed | ImageIO destination/finalize失败 |
| InvalidImage | capture_invalid_image | 实际图像/PNG尺寸或budget不合法 |

Native error仅保留固定枚举domain分类与数字code；不会导出NSError文本。是否终止当前控制权由Host现有feedback安全策略处理，库不注入输入或自行stop。

## 开发编译 / 延后 CC 测试

独立 `[workspace]`，命令显式指定 manifest，不写 root Cargo。

```sh
cargo check --manifest-path crates/macos-capture/Cargo.toml --all-targets --locked
```

本次只执行编译，没有测试/capture/权限调用。测试代码位于 `src/tests.rs`，使用生产 deadline/flight/selection/image guard seam；不是平行 oracle。

**下列命令仅记录交接；按最新优先级，先测Windows，目前不要执行Mac/Linux测试：**

```sh
# 后续经协调者重新安排，由 Claude Code + GLM 执行纯测试。
cargo test --manifest-path crates/macos-capture/Cargo.toml --lib --locked
```

没有 `#[ignore]` 实机 capture 测试或会意外发起截图的 executable；也没有本任务的真实截图 runner。后续由 Host owner 接线、协调者单独允许后再作真实 filtered capture 验收。
