# Linux/X11 computer-use 功能对齐设计

## 已批准目标与边界
用户已批准按 Windows 功能清单推进 Linux 实现/修复/验收，明确 Docker 只是测试环境，与产品职责分离。本设计记录前述讨论，不把任务缩减为冒烟或只修坐标。

基线：913b9ce（工作区干净）。不 worktree、不 commit/push、不修改其他项目/全局模型配置。X11 为本轮会话支持范围；Wayland/XWayland 明确拒绝，不伪装为支持。平台能力语义对齐，不复制 Windows OS API。

## 设计选择
采用现有 Runtime/MCP、输入生命周期与 display-topology 公共契约，增加 Linux 平台适配；不创建容器专用后端，不把环境脚本纳入产品。相比另造 Linux host，这能共享取消/错误契约；相比只宣布 primary-only 支持，这保留完整功能对齐目标。

1. **显示**：新增 Linux 原生 RandR 枚举和 X11 采集 provider，复用已存在的纯拓扑/合成引擎。原点、尺寸和输入一律 root physical pixels；Xft DPI 不参与输入换算。支持 primary/id/desktop、稳定显示身份、变化代际、单屏无 primary 回退、多屏歧义明确错误。预算在读取/分配源画面之前检查。
2. **文本**：native 输入支持 Unicode，不把任意字符串默默改成粘贴；按键语义独立。动态键盘映射若采用，必须保全/恢复映射、限定副作用并处理失败。另实现明确请求的 clipboard paste，遵循 README 当前 Windows 契约：显式覆盖剪贴板，不读取/恢复用户原内容，不隐式 fallback；保全所有权/焦点校验、限时与失败报告，说明 X11 selection 在 owner/clipboard manager 生命周期上的实际差异。空 Wayland 变量判断一致。
3. **执行权/生命周期**：同一个 X 桌面必须有可验证的单 writer 所有权，不能仅依赖 UID/TMPDIR。锁的建立不得注入输入。初始化有 deadline，失败不遗留可继续操作的后台执行；清理未知不能报 released。检查 remote 初始化失败分类及 quarantine 恢复。
4. **急停/反馈**：Linux 本地全局急停与 Windows 同等用户控制语义；不能影响普通组合键输入。反馈保持现有协议和可选模式，必须核实 capture exclusion，而非隐藏窗口后假称不会入镜。
5. **能力描述**：实现、支持声明、工具 schema、README 同步，不用接口存在冒充平台实现。
6. **验收**：单元/契约、真实 X11 场景、异常/并发/缩放/多屏、远程 TLS、真实应用及隔离黑盒均需证据。Windows 构建和相关回归不可被 Linux 实现破坏。macOS 编译检查，既有未授权 GUI 测试不启动。

## 完成判定
上述功能逐项具有实现、独立审查和适配范围的测试证据。仅编译或基础 Xvfb smoke 不算完成。任何能力不满足则保留未完成，不删除矩阵项。真实 GUI 由独立 Claude CLI 串行执行，仅自有测试资源；实际模型按响应记录，不修改全局配置冒充历史模型。

## 第一轮接口决议（实施中）

### X11 桌面所有权
- MCP lock 通过 X server 级 selection `_RPA_COMPUTER_DESKTOP_WRITER_V1` 原子获取所有权，私有 owner window 位于 first root；不同 screen 不得绕过同 server 的 writer 排他。保留文件 acquire_at 给其他平台/测试，不用临时路径授权 Linux 桌面。
- native-input 定义显式 Clone `X11Lease`：selection atom、owner window、16字节随机 nonce、共享不可逆 revoke 标志。nonce 属性 `_RPA_COMPUTER_DESKTOP_WRITER_NONCE_V1` 在 owner window 上，STRING/format8/16字节。不是对恶意 X 客户端的安全边界。
- CLI 获取 lock 后提取 token，通过 `BackendFactory::Desktop { lease }` → `DesktopBackend::new_with_lease` → `native_driver_with_x11_lease` 传递。禁止环境变量授权、全局隐式安装、backend 依赖 MCP 或生产无 lease 降级。
- 上述 token 强制要求针对 computer-host 的生产路径。独立复用的 native-input 原始库 API（`native_driver()` / `Input::new()`）按已批准的边界保留无 lease 用法，不把它们本身认定为 host 绕过；必须验证 host 的实际构造链没有调用这些无授权入口。
- 每个 native 原子输入/映射变更/清理操作在**同一个 Xlib 连接**的短 server grab 内校验 token、owner、nonce；确认失败永久 revoke，不继续输入。监控 SelectionClear/连接错误是辅助，不能代替原子输入门禁。
- 不在 server grab 内等待其他客户端、睡眠或执行整段长动作；嵌套 helper 不得提前释放外层 grab。

### 原生文字与清理
- stable mapping lease 不在逐 scalar 之间复用到其他字符；只用当前未占用/未按下/无动作或 modifier 作用的键位，耗尽明确报错，不暗中 paste。X11 server 接受事件不代表应用已经消费。
- Driver 新增可失败、可重复的 `cleanup`，默认无资源的平台返回成功；上层 NativeInput/BackendCore/Input release_all 在释放自有按住输入后调用，并合并错误。没有按住输入也不能漏掉映射清理错误。Drop 仅 best effort，不能用它制造 released 成功。
- 恢复前复核所有权；外部修改不覆盖；自己的 key release 未确认时不贸然恢复映射。保留失败状态供明确重试。

### 初始化
生产 native init 使用命名的约10秒预算，测试注入较短期限；超时/stop 关闭工作路径、取消且保留 cleanup unknown。迟到的构造结果不能执行排队 open/resume。已确认从未输入的 setup 失败与不确定失败使用不同退出分类。

### Linux 反馈仍需实施，不移出范围
独立设计审查否定了“XComposite overlay 放上去就自然不会被 GetImage 截到”的捷径。完整反馈还需要 capture/provider 排除设计和实际证明；仅 renderer 返回 requested 不够。报告：`.agents/reports/2026-10-15-linux-feedback-design-review.md`。此项仍未实现，不以测试环境特例宣称对齐。

## 2026-10-15 实际验证后的设计修订（仍在实施）

- Linux 急停固定组合改为 **Ctrl+Alt+Shift+Escape**；Windows 仍为 Ctrl+Alt+F12。目标是同等本地急停/取消语义，不是跨平台复制可能被 XKB VT 切换动作消费的按键组合。保留原 F12 的冻结源码、默认映射失败与审查证据；修订相关测试必须显式记录，不能删除错误历史或用修改测试环境的键盘映射换取通过。
- 新组合仍须检查实际 XKB type/action/control、相关映射变更与就绪状态竞争；注册成功不等于实际按下后必定取消。共享 modifier bit 必须基于实际按键身份判定并正确重放非匹配组合，不能吞掉普通输入。
- 鉴权文件读取属于端到端初始化时限，不能通过把 HOME/XAUTHORITY 的 open/stat/read 移出计时范围来宣称有界。采用待审查的**仅凭据读取 helper**方向：它不连接 X server、不注入输入；凭据经有界私有 IPC 返回；超时只终止自己的 helper，并如实处理内核不可中断 I/O 下未确认退出/后续回收。不得用另一个 detached 读线程或无限 join 兜底。跨 lock/hotkey/clipboard 的共享实现与所有权需单独协调；此项尚未实现或验证。
- 首轮真实 GUI 只验证冻结阶段产物。基础点击/ASCII/截图通过不覆盖后续 native/clipboard 改动；96 DPI 显示场景的 fixture 心跳停滞目前根因未明，192 DPI 尚未执行，不能宣称跨 DPI 已通过。

### 2026-10-16：原生输入读取安全与映射所有权澄清

- 已确认测试环境 libXi `2:1.8.1-1build1` 的 `XGetDeviceKeyMapping` 分配/读取 CARD32 原始字节，而当前产品按 LP64 `KeySym` 数组读取，存在错位及越界。不能用升级测试容器依赖、改变默认键盘映射或放宽设备兼容性检查掩盖此产品缺陷；旧只读探针因复用了同一路径而失效，其从属设备 core-map 推论全部撤销。
- 修复方向为同一 Xlib Display 上借用公开 Xlib/XCB bridge，通过明确的 XI 协议 CARD32 reply 读取。保留 Xlib 的事件队列所有权，不关闭借用的连接，不新建一个会被 server grab 阻塞的读连接，不使用版本猜测的指针强转。新增 Linux 平台依赖须在产品依赖说明中明确，构建镜像只提供正常构建依赖。
- 对显式 XTEST slave 目标的写入，必须把已验证的设备身份、生命周期、descriptor/index/range 与原始快照绑定；选择器输出不能绕过真实 writer 的匹配检查。安装、完整恢复及 explicit-only 恢复均不得退回 master SetMap。
- master projection 的恢复遵循原有“只恢复仍属于本次操作的修改，不覆盖外部新状态”的原则：不能强行恢复已被合法物理输入更新的历史整张 master 映射，也不能只恢复 XTEST、忽略仍存在的本次操作残留。无法证明归属或清理完成时，保留状态并明确报告 conflict/unknown，不报告成功、不丢弃租约。
- 纯 server 模型 RED 是回归基础，不是实际设备热插拔或应用文字接收的证明；target/descriptor 对应、完整租约生命周期、active/inactive master projection、失败清理和实际应用验收仍需独立证据。
