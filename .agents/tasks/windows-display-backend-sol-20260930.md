# Windows 多屏真实 backend：独立可接线模块

Codex+gpt-6.1-sol仅实现源码/测试源码/构建配置。执行验证统一CC+GLM。独占写集 `crates/windows-display/**` 与 `.agents/reports/windows-display-backend-sol-20260930.md`；不要改root Cargo/src、display-topology、native-input、fixture、renderer。根Host源码冻结供CC测试中。直接编辑共享目录，不worktree/commit/push/reset，不启动其它agent，不SSH/GUI/截图/注入/执行测试。必要 cargo check/metadata编译允许，测试仅写代码。Windows only，不写Mac/Linux backend。

用户已批准的多屏设计在 docs/superpowers/specs/2026-09-30-complete-computer-actions-design.md 第4节与 acceptance cases multi01..12。本任务推进真实backend，不止再写一个mock库；你已熟悉纯库 crates/display-topology（Windows22测试已通过）。先读其真实API/README、现有src/backend/{capture,windows}.rs理解旧primary路径，不把旧width*scale公式盲移新实现。

交付独立Rust workspace `rpa-windows-display`，用Windows官方windows-rs声明真实OS接口（复用当前0.58版本可避免新下载），新库依赖 ../display-topology；image可用现有0.24 PNG功能做encode/resize。不得复制第三方输入库或启动外部命令。官方文档/已缓存binding核实API和资源生命周期，记录关键依据。

必须能力：
1. 明确PMv2契约：不在导入/构造纯数据时改变全局DPI；显式prepare/initialize并核实实际thread/process上下文后才enumerate/capture，或明确要求调用方已准备、验证不满足返回错误。不要silent DPI fallback，避免thread override与process设定矛盾。Windows native bounds和最终GDI来源像素均物理像素，不再按dpi比例双乘。单位PhysicalPixels。
2. 真实枚举所有active监视器，runtime opaque ID确定性唯一（字符串适合后续wire token≤128 ASCII），primary、物理bounds、source实际像素尺寸、scale/DPI与rotation等事实；缺失/重复/无primary/无显示器/异常OS错误准确拒绝。处理负origin，不猜第二屏，不把错误默认成scale1或primary。ID不承诺跨重插，HMONITOR/设备身份核对防悬挂。若某字段无法可靠读取，明确错误/unsupported而非虚构值。
3. 使用rpa-display-topology长期Tracker选择Primary/Display(id)/Desktop与capture plan，产出真实原始RGBA帧以及PNG+精确mapping（按库公开API选易接线接口）。显示前后完整枚举对比generation，任何变化都丢弃capture，显式副屏消失不fallback。纯plan/mapping保持唯一来源，不复制实现。
4. 真实逐屏截图优先受控GDI BitBlt/GetDIBits等Windows-rs接口，所有HDC/HBITMAP/select old object生命周期RAII、失败分支归还。不依赖截图后涂抹排除renderer。Win affinity requested只平台标识，本库不声称GUI实际排除通过。GDI来源像素方向(BGRA/top-down/alpha)明确定义，验证实际字节长度/stride/尺寸；拒绝overflow/大于预算的source再分配，不仅限制最终PNG。CaptureBudget约束最终、另显式source-byte与total-byte预算约束中间图像/组合内存。禁止任意超大稀疏布局分配。
5. 真正逐tile独立缩放/合成，空隙确定性背景、整源尺寸核对，最终PNG尺寸与mapping完全相符。不能把全图缩小后仅重标region，也不能混合DPI套全局width比。资源/截图失败不返回partial desktop或复用陈旧pixels。真实GDI同步可能阻塞，不承诺不存在的可取消deadline；保留Host隔离/超时/quarantine上层职责。
6. 单独纯provider seam供CC无GUI测试：合成/前后拓扑变更/缺屏/尺寸不符/预算溢出/负origin/混合DPI/rotation元数据/空隙/颜色通道/top-down/资源owner设计等。默认cargo tests绝不调用OS枚举/DPI/capture/真实desktop；若live测试源码一定显式ignored且交协调者另授权。尽量无需fake OS capture，提供pure frame provider即可。不用同一个函数计算expected自测。

模块化每文件合理长度，遵守#!unsafe边界局部WinFFI不扩散。库在非Windows可编译纯逻辑/明确信息，实际backend返回unsupported，不假capture。不要运行Mac/Linux tests。

交付README公开API及ROOT下一步接线示例、准确限制（root未接/未真实多屏/未截图排除验收）、编译原始结果。用开发check --target x86_64-pc-windows-msvc --all-targets无需实际运行；link问题报告给CC用已配置cargo-xwin。交付后冻结，待CC Windows tests。不要为“编译通过”删除测试/弱化协议。
