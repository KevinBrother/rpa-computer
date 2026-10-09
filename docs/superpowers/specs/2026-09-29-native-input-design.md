# 项目内原生键鼠后端：范围与接口决策

状态：2026-09-29 用户确认Linux首版X11；要求Windows复用windows-rs官方接口定义。此文件记录设计，不表示实现完成。不得commit/push，不创建worktree，不修改父目录其他项目。

## 目标与边界

移除Enigo及公共接口中的Enigo类型。在rpa-computer内维护跨平台键、按钮、事件构造、发送、持有状态与释放逻辑。不调用xdotool/ydotool，不依赖libxdo，不将Enigo源码整包vendor。必要的操作系统API/协议绑定允许使用，不将“原生实现”解释为手抄平台ABI。

- Windows：使用Microsoft windows-rs项目的`windows` crate，仅在Windows target依赖，并按需启用Win32 features。复用SendInput、INPUT/KEYBDINPUT/MOUSEINPUT、常量与相关窗口/键盘接口定义。不手写这些Win32结构布局和extern声明；策略、语义与错误处理仍由项目负责。
- macOS：CoreGraphics CGEvent，保留输入权限检测、坐标映射与本进程持有状态管理。
- Linux：首版仅X11，通过X11/XTest底层接口实现。Wayland原生会话暂不支持；不能因为存在DISPLAY/XWayland便冒充能够控制整个Wayland桌面。不引入默认root/uinput服务，不通过外部命令操作桌面。系统X11/XTest接口依赖应明确列出。

## 保留的执行契约

Runtime/MCP/Host-Client协议不因后端替换而随意变化。仍须校验动作后再输入、检查过期观察、保留坐标变换、取消/超时、幂等及输入partial/unknown结果。发送系统事件与任务完成分开。新增Linux须一起检查截图、几何与桌面会话检测，不能只把compile_error删除便宣布支持。

按键名与鼠标按钮改用项目自有类型。文本处理明确Unicode scalar到各平台事件的转换；Win32代理对、换行/Tab、按下/松开顺序均须有事件级测试。字符间隔应由可取消执行计划负责，禁止在整串底层调用中睡眠到无法及时取消。最终默认间隔/事件顺序须经失败样例对照验证，不预设1ms必然可靠。

只释放本会话已成功注入且尚未松开的输入；不全局释放用户物理按键。部分SendInput、资源建立/释放失败与错误码需分别建模，不能把全部计数接收当作目标文本匹配。

## 验证与实现分工

产品源码、测试、构建/部署脚本继续由真实本机Claude CLI（配置GLM）编写；协调者负责设计、调度、审查和执行验证。源码改动不覆盖已有未提交变更。先写失败测试、验证失败、再实现；独立审查后真机串行验证。Windows服务/测试必须处于交互桌面，不是SSH服务会话。

Windows保留现有Unicode失败样例，加入BMP、非BMP、CR/LF/Tab与短文本、取消/partial测试。macOS须构建并验证自有窗口。Linux需真实X11构建与运行证据；没有可用Linux桌面时明确未验收，禁止用mock证明支持。不得以剪贴板或UIA赋值替代原生输入验收。

## 当前源码事实（不是新实现）

Cargo.toml仍直接依赖Enigo。src/backend/dispatch.rs仍委托Enigo。部分项目Win32代码仍为手写extern。因此“新实现使用windows-rs”是本次确认的实现方向，不是声称当前替换已经完成。

## 用户追加确认：可独立提取的库（2026-09-29）

用户明确要求后续能提取为类似Enigo的独立工具库。采用独立path crate `crates/native-input`（包名`rpa-native-input`），只依赖平台系统接口，不依赖rpa-computer/MCP/LLM/截图/网络。宿主负责动作规划、截图映射、取消deadline、串行桌面使用；库负责类型、事件构造、原生调用与自身持有状态的尽力清理。Windows使用windows0.58官方绑定（当前本地已存在，可按准确接口编译）；Linux仅X11，系统libX11/libXtst允许，不依赖libxdo/xdotool或第三方自动化封装。测试仍由Claude CLI+GLM执行，真实桌面串行。

## 实施状态补充（2026-09-29）

上述“当前基线”描述的是实施前状态。当前工作树已经以独立 `rpa-native-input` path crate 替换 Enigo；Windows 的输入及宿主 DPI 接口均使用 windows-rs 0.58。库可独立编译、测试，不依赖宿主、MCP、截图或模型。

实施中进一步明确的边界：
- 宿主使用 raw `Driver` + 原有 `BackendCore` 作为唯一按键持有状态管理者；独立库使用者可选 `Input` 包装器，避免宿主叠加两个状态管理者。
- 文本在整个动作执行前拒绝 NUL、其余 C0 和 DEL（保留 CR/LF/TAB）；运行时负责 CRLF 归一化、逐 Unicode scalar 的可取消间隔。1ms 是策略，不是输入成功保证。
- X11 首版不临时改写全局键盘映射，不支持 Wayland；文本仅接受当前映射中能安全找到的非修饰键、非 Shift 层字符。未映射的中文、emoji 等明确报错，不能宣传为 X11 完整 Unicode 支持。
- macOS TIS 查询在库内串行；嵌入 GUI 应用时仍须由使用者满足主线程约束。CGEventPost 没有接收端处理确认；当前鼠标 click-state 固定为 1，不能将单击测试外推为双击验证。
- 真实桌面验证必须分别记录“工具派发”“模型实际参数”“应用实际结果”；受控诊断测试不是已经满足全部隔离、稳定性与动作覆盖门禁的正式验收。

独立复核见 `.agents/reviews/native-input-{initial,windows-gate,macos-gate}-20260929.md`；以较新的平台门禁复核覆盖 initial 中已修复的旧行号与结论。测试原始日志留在 `.agents/runs/`，最终结果另行汇总。
