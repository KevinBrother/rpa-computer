# 原生输入库与双平台 GLM 实测（2026-09-29）

状态：原生库与宿主集成已落盘；跨平台构建/纯测试、Windows 受控 GUI 诊断已完成；macOS 真实 GLM 截图发现锁屏，动作测试等待用户手动解锁。不是完整验收通过。

## 架构交付

- 独立 `crates/native-input` / `rpa-native-input`：公共类型、原生 Driver、可选 Input 持有状态包装器；不依赖宿主/MCP/LLM/截图/网络。
- 宿主使用 raw Driver，BackendCore 是宿主唯一按键持有状态管理者。Runtime 编排、取消、超时、图片坐标映射与文本节奏不进入可抽取库。
- Windows：windows-rs 0.58；macOS：CoreGraphics + 系统框架 FFI；Linux：直接 libX11/libXtst，明确拒绝 Wayland，无 Enigo/xdotool/libxdo。
- 修复过 Windows Unicode wVk 字段、Mac FFI 宽度/参数/常量、按键释放缓存、整段非法文本前置验证、错误码保留、X11 错误映射策略等；失败构建日志保留。

## 原生验证

- macOS 原生 crate：35 passed；Windows 原生测试 exe 在 acer-win：47 passed；Linux 容器链接及执行：31 passed。
- 根项目最终全套测试：279 passed、2 ignored、0 failed（首次 `native-root-final-all-20260929.log`；20:19 前重新执行 `native-root-recheck-20260929.log` 仍为同样结果，native crate 复跑 35 passed）。
- Mac release 以及 Windows MSVC release 已构建；Windows 无 Rust 工具链，使用本机交叉编译后复制，不是声称 Windows 本机 cargo build。
- Linux 根项目 `cargo check --all-targets` 成功；有 screenshots 依赖 future-incompat warning，不等同未来编译器保证。
- 门禁复核：`native-input-windows-gate-20260929.md`、`native-input-macos-gate-20260929.md`（在 `.agents/reviews/`）。初始审查中的旧问题由新报告逐项复核，不能把初始状态误认为最终状态。

## GUI 测试约束

真实 Claude CLI，指定 sonnet 路由，实际响应模型从转录核对 GLM。内建工具禁用，仅 8 个 computer MCP 工具；GUI 操作串行。Windows Host/CLI/Fixture 均 Interactive Session 1；SSH 只用于部署、服务/测试进程编排与取回证据，输入走 Client→TLS→Host，不走 SSH 输入。

Windows 测试没有 OS 文件隔离证明，只能称受控 GUI 诊断，不能称通过完整黑盒隔离门禁。视觉抄写与指定字符串输入是不同测试：前者不向 agent 提供样例答案，后者故意提供期望输入以隔离 OCR 与注入层。不得合并成同一成功率。

## 已知边界（非本轮通过）

- X11 文本只支持当前 keymap 的非 Shift 基础层可映射字符；未映射中文/emoji 明确失败。没有物理 Linux GUI 验收。Xlib 断连错误可能使进程退出，阻塞调用没有硬时限。
- macOS 当前 click-state 固定为 1；双击/三击不应宣称已正确实现或验收。GUI 嵌入者须满足 TIS 主线程要求；库内锁不能约束第三方 TIS 调用。
- SendInput count / CGEventPost 调用成功只代表派发，不保证目标文本。释放只面向本会话记录的输入，但不保证与用户同时操作同一个物理键时完全互不干扰。
- 未覆盖完整每平台每动作 10 次门禁、多显示器/全部 DPI、真实拖拽/滚动/长按取消；不以 mocks 替代这些结论。

## 最终 GUI 结果与清理

### Windows（受控 GUI 诊断）

- 视觉抄写矩阵：计划 10 次，20 分钟超时前完成 9 次。随机目标点击 **9/9 HIT**，逐码点文本 **0/9 MATCHED**；没有最终 result，策略审计失败，这组不能冒充完整通过。转录显示 GLM 在调用 text_input 时已将 emoji、全角标点等识别成其他码点；协调者检查截图发现单色 emoji / 全半角存在视觉歧义，而非黑图或图片传输损坏。
- 第一次指定文本对照：**0/3 expected matched**，但 **3/3 应用 actual_text == 模型 text_input 参数**。进一步定位到 Windows runner 的 `StandardInput.Write` 使用默认 ibm850，把任务里的中文/emoji 在送达 Claude 之前损坏。与 native SendInput 重复注入不是同一问题。
- runner 改为 UTF-8 无 BOM bytes 写入 stdin BaseStream 后：**3/3 actual_text == model payload == expected_text**。1217 bytes 任务 SHA256 `adba1d24b2aea64392b305dbcc2134672336e7bcddcd9a66f30196f5bc3642a8` 与本地一致，15 calls/16 turns，审计 17 checks passed。
- Calculator 可见按钮清零→10×20=：**3/3 显示 200**，30 calls/31 turns，审计 17 checks passed；协调者复核截图，不用心算代替。
- 模型均由响应标签确认 `glm-5.3-flash`。证据前缀分别为 `native-win-fixture-*`、`native-win-known-before-utf8-*`、`native-win-utf8-*`、`native-win-calc-*`，见 `.agents/runs/`。
- 本轮 fixture、backdrop、agent ScheduledTasks、TLS Host 均已通过身份核验清理，8399 无监听；保留 evidence 和部署产物。用户原有 Notepad PID24332 仍在 Session1。清理日志 `native-win-final-cleanup-20260929.log`。

### macOS（锁屏阻塞，未执行动作案例）

- 20:12 首轮及20:16重试在 MCP 连接阶段失败，无GUI工具调用，不能称通过。Host 返回 `no_display: no primary display found`。
- 原生只读 probe：2 个 online display，0 个 active display，二者 `IsAsleep=true`；当前probe进程的 ScreenCapture/AX preflight 均 true。sandbox / 非 sandbox 同样失败。
- `caffeinate -u` 暂时唤醒后 active count恢复2，非sandbox Host启动成功；短时assertion到期后显示器重新休眠。随后有时限的 `caffeinate -u -d -i -t1260` 保持唤醒，**不修改系统设置、不解锁**。
- 20:17 第三轮真实 Claude CLI（响应 `glm-5.3-flash`）连接到全部8个computer工具，执行 `computer_open → computer_observe → computer_close`。实际截图为macOS密码锁屏；协调者已亲自检查同一截图，未将其误判为fixture或黑屏。Agent没有点击、没有输入，10次目标/文本案例均为 **0次执行**；后续已知文本与Calculator任务未启动。
- 该轮sandbox负向probe通过，精确profile及SHA保留；17项策略审计通过，3 calls/4 turns/30.6s，**只证明工具策略和截图链路，不证明键鼠功能通过**。close返回 `cleanup_outcome: released`。
- 证据：`.agents/runs/native-mac-locked-transcript-20260929.jsonl`、`native-mac-first-frame-20260929.png`、`native-mac-display-probe-20260929.json`、`native-mac-display-after-wake-20260929.json`、`native-mac-matrix-wakeheld-launch-20260929.log`。原始sandbox/evidence在 `/private/tmp/computer-native-mac-20260929/evidence/20260929T121747Z-76693/`。
- 本轮macOS fixture/backdrop按PID+启动时间+exe核验后退出，临时caffeinate进程已清理；无本轮发布Host残留，不改用户窗口或安全设置。日志 `native-mac-cleanup-20260929.log`。
- 待用户手动解锁后，重启自有fixture/backdrop，依次运行视觉矩阵、指定Unicode文本3次、Calculator3次。保留以上失败/阻塞证据，不能覆盖成成功。


