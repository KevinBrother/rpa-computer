# Primary API audit / ownership — 2026-09-30

只读调查本机 Xcode SDK 与维护者发布的官方绑定；没有调用 capture、permission preflight 或其它 GUI API。

## 官方依据

- [Apple SCScreenshotManager.captureImage](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager/captureimage(contentfilter:configuration:completionhandler:))：macOS14.0 起，按 filter/configuration 抓一帧 CGImage。
- [Apple SCContentFilter excludingApplications](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/init(display:excludingapplications:exceptingwindows:))：macOS12.3 起，display/app/exception 三阶段过滤；empty exceptingWindows 不能 re-include renderer。
- [Apple SCShareableContent](https://developer.apple.com/documentation/screencapturekit/scshareablecontent)。
- [Apple SCRunningApplication.processID](https://developer.apple.com/documentation/screencapturekit/scrunningapplication/processid)：过滤使用原生进程ID，不从名称反推。
- [Apple CGPreflightScreenCaptureAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightscreencaptureaccess())：本库只预检，不调用 request API。
- [Apple CGImageDestinationCreateWithData](https://developer.apple.com/documentation/imageio/cgimagedestinationcreatewithdata(_:_:_:_:))、[AddImage](https://developer.apple.com/documentation/imageio/cgimagedestinationaddimage(_:_:_:))、[Finalize](https://developer.apple.com/documentation/imageio/cgimagedestinationfinalize(_:))：本库用原生 encoder，不做overlay像素编辑。
- 维护者：[objc2 0.6.4](https://docs.rs/objc2/0.6.4/objc2/)、[block2 0.6.2](https://docs.rs/block2/0.6.2/block2/)、[objc2-screen-capture-kit 0.3.2](https://docs.rs/objc2-screen-capture-kit/0.3.2/objc2_screen_capture_kit/)、[objc2-image-io 0.3.2](https://docs.rs/objc2-image-io/0.3.2/objc2_image_io/)。

同源 Apple DocC JSON 已用于核对 captureImage/filter 的 availability，结论与本机 SDK 头文件一致。

## 本机 SDK 交叉核对

SDK路径由 `xcrun --show-sdk-path` 读取，不硬编码到构建：
`/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk`。

- `ScreenCaptureKit.framework/Headers/SCScreenshotManager.h`：`captureImageWithFilter:configuration:completionHandler:` 标为 `API_AVAILABLE(macos(14.0))`；后续15.2/26专用 screenshot API不被本库调用。
- `SCShareableContent.h`：displayID为CGDirectDisplayID、application.processID为pid_t，completion对象可nullable。
- `SCStream.h`：官方 `initWithDisplay:excludingApplications:exceptingWindows:` 签名及所用 configuration setters。
- `SCError.h`：UserDeclined=-3801，MissingEntitlements=-3803，NoDisplayList=-3814，NoCaptureSource=-3815。**-3810是AttemptToConfigState，不是NoDisplay**；测试/生产mapping已经使用核实后的值。
- `CoreGraphics.framework/Headers/CGWindow.h`：preflight可用，不调用request入口。
- `ImageIO.framework/Headers/CGImageDestination.h`：CreateWithData/AddImage/Finalize的真实CF签名。

Rust通过maintainer-generated声明调用，没有自造method selector、`msg_send!` 或裸C函数定义。`NSError` 的domain和数字code来自官方声明；不复制描述信息。

## 异步所有权审阅

1. Client持有 `FlightSlot<Option<Arc<Flight<CapturedImage>>>>`，只容许一个 outstanding native flight。
2. Discovery `RcBlock` heap capture：Copy request+Arc flight；不capture client、栈引用或栈channel。
3. Borrowed `SCShareableContent` 仅在callback内使用并通过官方 `Retained::retain` 保留；getter arrays 返回Retained。
4. Native filter、configuration、shareable content及两组initializer数组由screenshot block持有；configuration dispatch后不可变。对象不从Rust裸指针跨等待线程返回。
5. Screenshot callback的nullable CGImage/Error只在有效callback范围解引用；原生PNG encode完成后仅把Rust-owned Vec/尺寸存进flight。
6. 使用所有callback共同采用的production claim/poll/complete gate；timeout/重复/旧callback不能覆盖新flight，callbackcomplete以前Busy保持。
7. 超时后的迟到callback先判断deadline/abandon，不再做下一阶段或encode；已经dispatch的OS操作不能被本API强制撤销，只保持有界隔离。
8. Native pools/encoder对象在通知caller前释放；Rust panic fence转无敏感静态错误。此处不是对Objective-C异常的通用catch保证。

## 支持边界与待证实事项

- API功能最低macOS14；Foundation OS guard 先于14专用selector，无旧capture fallback。
- `objc2-screen-capture-kit` 用framework链接。组件支持基线14不意味着任意旧Host安装包都可启动；若Host owner要覆盖没有该framework的旧系统，需要在packaging层处理optional/weak linking。没有擅改root/发布基线。
- Preflight拒绝时本库不启动discovery，也不请求TCC授权；权限运行中变化通过NSError失败处理，不修改TCC。系统自身可能采取额外策略，不承诺以本库绕过系统确认。
- 没有真实验证NSPanel/CLI renderer是否出现在SCK apps列表；找不到 exact PID 就失败关闭，不按名称或放弃过滤。
- 没有真实验证同一时刻“人可见、filtered图像不可见”、多屏/DPI、颜色/方向/实际尺寸；cargo check及pure tests不能证明这些。
- 最新优先级：Mac测试和Host Mac接线延后；此代码交付不阻塞Windows先测。
