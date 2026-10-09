# Optional desktop feedback renderer (v1)

本目录提供独立可执行 renderer：macOS **Swift/AppKit** 与 Windows **C#5/WinForms**。
它不接管输入、不取消内核动作、不截图、不监听网络；只消费 Host 的私有 stdin NDJSON 并在 stdout 发控制请求。
是否启用及 renderer 启动、失联后的取消/清理策略由 Host 实现。**输入内核没有本目录的构建依赖。**

> 当前交付是源码与编译检查，不是 GUI/截图验收完成。按最新分工，测试与验收由协调者安排 **Claude Code + GLM** 执行。
> **Mac 当前 `capture_exclusion=unsupported`，不能保证模型截图不含本层；Host capture 另行改造。**

## 构建（只编译，不启动窗口）

项目根为 `rpa-computer/`。

macOS 13+、Xcode Command Line Tools / Swift 5.7+，编译本机架构：

```sh
./desktop-feedback/build-macos.sh
# 可选显式输出路径： ./desktop-feedback/build-macos.sh /absolute/path/renderer
```

输出 `desktop-feedback/build/desktop-feedback-macos`。源码通过 AppKit/CoreGraphics 链接；不跑 Cargo、不依赖旧 fixture。

Windows x64、.NET Framework v4、C#5 compiler，PowerShell：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\desktop-feedback\build-windows.ps1
```

编译器固定为 `%WINDIR%\Microsoft.NET\Framework64\v4.0.30319\csc.exe`，不自动寻找 dotnet/Mono 替代品。
输出 `desktop-feedback\build\desktop-feedback-windows.exe` 和对应 `.exe.config`；构建脚本不运行它们。
若本机 Framework compiler 不支持 `/langversion:5`，明确报编译失败，不能悄悄降级编译器。

`check-csharp-compile.sh` 是开发用 **Mono C#5/.NET4 静态编译检查**，不是 Windows 原生 Framework64 构建，且不执行生成物。

## Host 启动约定

Host 在**被控端用户交互桌面**创建子进程，用私有 stdin/stdout 管道连接。Windows Session 0 会被拒绝；Host 还需负责选择正确的用户桌面/session。
不要单独双击 renderer：stdin 的 EOF 会让它退出，且没有 Host 就没有有效 Stop。
正常模式必须先收到真实 GUI 初始化后的 `ready`，随后 Host 才能依赖 UI 存在；每秒从 UI 主线程定时器发 `heartbeat`。
UI 未初始化成功只有 `error` 和非零退出，不伪造 ready。stdout 只含协议，stderr 只含静态诊断及 Win32 错误号。

参数只支持：

```text
--accent '#RRGGBB'  # 默认 #2563EB，作用于品牌点和 pointer ring
--label 'AI control' # 默认 AI control，非空、最多48个UTF-16单元
```

label 不接受控制、换行、Unicode format/bidi 字符；作为普通文本绘制，不作为 HTML、快捷键或脚本解释。
未知、重复或缺值参数报错。Stop 文案不受品牌 label 替换，避免失去停止语义。

## 视觉与交互

- 深色紧凑状态条，品牌点 + 两行清晰文字；初始显示 `Waiting for Host`。
- Stop 是**独立 88×48 逻辑单位命中区**，与装饰条分开。Windows 按当前窗口 DPI 缩放控件大小；输入坐标仍使用原生像素。
- 状态条和 ring 点击穿透；Mac nonactivating panels / Windows layered+transparent+noactivate HWND；Stop 窗口不透明穿透。
- 不更改全局光标，不请求输入注入权限，不把动画当成动作。只显示 Host 已确认派发的位置；move/drag ring 350ms，click ring 650ms。
- 无 session、closed 或同代次已发 Stop 时禁用按钮；新 session/generation 才重新启用。请求 Stop 不会本地显示“已安全停止”。
- 窗口关闭意图也转成当前 session/generation 的 stop。保持 UI 等 Host 通知或 EOF；不是“只关窗口继续输入”。
- `cleanup=failed/unknown/pending` 的告警优先于 phase；只有 Host 的 closed+released/not_needed 才显示完成事实。
- Mac `macos:<CGDirectDisplayID>` 和 geometry 必须匹配当前真实屏幕。Windows geometry 与当前 PMv2 屏幕匹配；不重算 Host opaque surface id。
- 拔屏/几何不匹配隐藏 pointer，状态提示 `Display unavailable`，保留 Stop 并报 `surface_geometry_unavailable`。每秒重查布局。
- `surface.version` 是 opaque 非空字符串，**允许逗号**：`d1:o0,0:i1512x982:c3024x1964:r0`，不套 session-id token 白名单。

## 协议与有界性

严格遵守 `.agents/tasks/desktop-feedback-v1-wire-20260930.md` 的消息字段与 snake_case。
所有 snapshot 字段必须存在，session/surface/pointer 可为 null。拒绝未知/重复字段、非 UTF-8、无效枚举、不合法 JSON 和非有限坐标。
version/sequence/generation 严格无符号整数，不接受 `1.0`、布尔或越界值；UInt64 保持精确，不经过 Double。
sequence 全进程单调，旧/相同序号不回滚 UI，latest mailbox 本身也维护 high-watermark。
Stop 只带 session+generation，不用 sequence 授权。

- 每帧 **16KiB（含最终 LF）**；内容最多 16383 bytes，与 Host codec 一致。CRLF 的 CR 也计算在上限内。
- stdin 在专用后台线程以 4096-byte chunk 有界读取，超限立即诊断，不等换行、不扩展缓冲。
- pending snapshot 只有一个槽位；错误只有一个槽位。主线程 30ms 取最新状态，读线程不操作窗口。
- stdout 后台写线程 FIFO，队列最多32条；排队过载/写失败导致明确失败退出，不静默丢 Stop 或阻塞 UI。
- malformed/oversized/truncated frame 终止本连接，静态 `error`，不回显 payload。
- stdin EOF 退出自身，不注入输入、不宣称 Host 已清理；stdout 最多300ms退出宽限，避免管道堵住留下常驻窗口。

## 截图排除能力（非验收结论）

详见 [平台 API 与 Host 缺口调查](docs/capture-exclusion.md)。

| 平台 | ready 值 | 含义与限制 |
|---|---|---|
| macOS | `unsupported` | 当前官方文档不能支持把 sharingType.none 当排除保证；现有 Host CoreGraphics 全屏路径没有显式排除本 renderer。需要另接 Host capture。 |
| Windows | `requested` 或 `unsupported` | 只有 Win10 2004+、DWM、三个顶层 HWND 设置/回读 WDA_EXCLUDEFROMCAPTURE 全部成功才 requested；失败附 error。仍待真实 GDI/model capture 验证。 |
| Linux | 无默认 renderer | 反馈层首版不支持；不更改内核及其 Linux 能力。 |

绝不以透明/穿透替代排除，不用像素涂抹，不用隐藏窗口后截图的竞态策略，不在旧 Windows 静默回退 WDA_MONITOR。

## 待 CC+GLM 执行

以下命令本次 **未执行**，不是已通过测试：

```sh
./desktop-feedback/build-macos.sh
desktop-feedback/build/desktop-feedback-macos --self-test
```

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\desktop-feedback\build-windows.ps1
& .\desktop-feedback\build\desktop-feedback-windows.exe --self-test
```

WinExe 的交接 runner 必须等待进程并检查退出码，例如 PowerShell `Start-Process -Wait -PassThru`（仅 self-test 模式），不要把即时 shell 返回当测试完成。
`--self-test` 在 GUI/DPI/API 初始化之前分支；不创建窗口、不发 ready、不注入输入。成功摘要写 stderr，退出0。
测试覆盖见 `macos/SelfTests.swift` / `windows/SelfTests.cs`；测试正文先于核心实现编写，因用户分工变更，本 agent **未执行 RED/GREEN**。

协调者还需实机验证：焦点不变、跨进程点击穿透、Stop 首次点击/可达性/代次拒绝、窗口关闭Stop、EOF、超大/坏帧、heartbeat、慢消费、混合DPI/负原点/拔屏、真实 Host 截图排除。
这些不是纯 self-test 或编译能够证明的。尤其 **Mac feedback-13 保持缺口**。
