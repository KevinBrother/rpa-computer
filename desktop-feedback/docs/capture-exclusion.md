# Screenshot exclusion investigation — 2026-09-30

只使用官方平台文档和项目/依赖的实际源码。没有截图或 GUI 实测；以下区分 API 意图、renderer 请求结果及 capture 后端效果。

## macOS：明确 unsupported，需要后续 Host capture 工作

官方依据：

- [NSWindow.SharingType.none](https://developer.apple.com/documentation/appkit/nswindow/sharingtype-swift.enum/none)：当前页面描述为 macOS 不再使用的 legacy 常量。官方 DocC 同源数据 `https://developer.apple.com/tutorials/data/documentation/appkit/nswindow/sharingtype-swift.enum.json` 的引用也给出此描述。不能从 setter 不抛错推断现代 macOS 截图排除有效。
- [NSWindow.sharingType](https://developer.apple.com/documentation/appkit/nswindow/sharingtype-swift.property) 讨论其它进程对窗口内容的访问级别，不能将其扩展为所有 capture backend 的排除保证。
- [SCContentFilter](https://developer.apple.com/documentation/screencapturekit/sccontentfilter) 提供 capture 方过滤机制。
- [init(display:excludingWindows:)](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/init(display:excludingwindows:)) 可由捕获方排除具体 SCWindow。
- [init(display:excludingApplications:exceptingWindows:)](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/init(display:excludingapplications:exceptingwindows:)) 可由捕获方排除 renderer 应用进程的窗口。

本次 renderer **不调用 sharingType.none 并宣称成功**；真实 AppKit 初始化后发送 `capture_exclusion:"unsupported"`。NSPanel.nonactivatingPanel、ignoresMouseEvents 和透明背景仅用于交互/外观，不用于宣称排除。

### 现有 Host 的实际路径（只读调查，没有修改）

- `Cargo.toml`: `screenshots = "0.8"`；本机已解析 dependency 为 `screenshots-0.8.10`。
- `src/backend/macos.rs::capture_screen`: `screenshots::Screen::new(...).capture()`。
- `screenshots-0.8.10/src/darwin.rs::capture`: `CGDisplay::screenshot(rect, kCGWindowListOptionOnScreenOnly, kCGNullWindowID, kCGWindowImageDefault)`。
- `core-graphics-0.23.2/src/display.rs::CGDisplay::screenshot`: 实际调用 `CGWindowListCreateImage`。
- 当前路径没有 renderer PID/窗口过滤配置，也没有 SCContentFilter。

**因此，当前 Mac renderer 不能保证它不进入实际发给模型的图像。此项未实现、未验收，不应勾选 feedback-13。**

后续建议（由 Host owner 单独设计/实现，不是本次修改）：

1. 在 Host capture 方采用官方 ScreenCaptureKit 显式过滤 renderer 的进程/窗口。
2. Host 已知自己 spawn 的子进程 PID，可用其匹配 capture 的应用集合；避免为了传窗口列表扩展本次冻结的 NDJSON schema。
3. 每次窗口/屏幕变化正确维护 filter，保留 Host 原生 points 与截图 backing pixels 的分离。
4. 用真实“人可见、Host 原始capture及最终model图像不可见”的同时态证据验收；过滤失效应诊断，而非像素抹除/隐藏截图。
5. renderer.ready 的 `unsupported` 是本地排除 API 能力事实；Host 自己成功实现过滤，不应据此伪造 renderer API requested。能力策略需要在后续集成中明确。

## Windows：实际 API 成功才 requested；GDI 排除仍待实测

官方依据：

- [SetWindowDisplayAffinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)：只用于当前进程顶层 HWND；依赖 DWM；WDA_EXCLUDEFROMCAPTURE 从 Windows 10 Version 2004 支持。旧版本传0x11会等效 WDA_MONITOR，所以不能只检查 setter 返回TRUE。
- [GetWindowDisplayAffinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowdisplayaffinity)：用于回读窗口设置，不是截图证据。
- [DwmIsCompositionEnabled](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmiscompositionenabled)。
- [GetVersionExW](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-getversionexw)：版本信息受 app manifest 影响；本 renderer 嵌入 Windows10 supportedOS manifest，再检查 build>=19041。
- [SetProcessDpiAwarenessContext](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext)、[GetDpiForWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow)：初始化前设置并核实 PMv2；不静默回退 system-DPI。
- [Window Features / Layered Windows](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#layered-windows)：layered 窗口加 WS_EX_TRANSPARENT 才有此处所用的跨下层鼠标穿透行为。不是把一般 extended TRANSPARENT painting-order 文档当鼠标保证。
- [Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)：WS_EX_NOACTIVATE；Stop 不加 WS_EX_TRANSPARENT，另处理 WM_MOUSEACTIVATE 的 MA_NOACTIVATE。

实现：在消息循环首个 tick（真实 bar/Stop HWND 可见，ring HWND 已分配）检查 Win10 2004+ 与 DWM，对三个顶层 HWND 逐一 SetWindowDisplayAffinity(0x11) 且 GetWindowDisplayAffinity 回读0x11。全部成功才发 `requested`。
失败发 `unsupported`+具体静态 `error`，stderr 提供 Win32 数字错误；部分设置失败时请求撤回所有 affinity 到 NONE，并明确记录撤回失败。**没有 WDA_MONITOR fallback。**

Host 当前 `src/backend/windows.rs` 调用 screenshots.capture；`screenshots-0.8.10/src/win32.rs` 用 GDI CreateDCW / StretchBlt(SRCCOPY) / GetDIBits。
官方文档只说明特定公开 capture API 范围，不保证所有后端；**requested 不是该 GDI capture 或最终 model capture 已验证排除**。
若协调者实机发现当前路径仍含提示/黑块，应由 Host owner 调整捕获后端或明确拒绝该能力，不能涂抹、假称安全或静默继续。

## 验收边界

本 agent 没有调用 AppKit/WinForms GUI 入口、SetWindowDisplayAffinity、截图、输入事件或 SSH。以上 API 调用只存在于 renderer 正常模式源码。
纯 self-test 不运行这些 API，且本次连 self-test 也未执行。GUI真实性、焦点/穿透、捕获效果全部留给协调者的 CC+GLM 实机验证。
