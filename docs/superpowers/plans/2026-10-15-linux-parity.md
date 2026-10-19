# Linux/X11 功能对齐实施与验收计划

> 执行：subagent-driven，互不重叠模块并行；协调者集成审查与环境编排。用户已要求开始，无 worktree/commit/push。

**Goal:** Linux/X11 达到 Windows 已提供能力的等价语义，并修正审查缺陷，获得独立验收证据。
**Architecture:** 共用 Runtime/MCP/display-topology，在原生输入、显示、剪贴板、控制和反馈边界增加 Linux 适配；Docker 仅验证。
**Tech Stack:** Rust、X11/XTest/RandR、现有拓扑合成引擎、Claude CLI 独立验证。

## 执行规则
- 开发者先写回归测试，独立 CC 执行 RED；再实现并由 CC 执行 GREEN。编译检查可由开发者执行，不当验收证据。
- 同一轮共享文件只有一名 owner。各 worker 不 SSH、不启动 GUI、不发送真实输入；协调者统一安排独立 CC。
- 每项实现后独立 spec/code review，反馈退回原 worker；测试失败不扩大成功声明。

## 完整需求矩阵（所有条目都保留）
| ID | 交付 | 实现/证据状态 |
|---|---|---|
| D1 | root-pixel 坐标、HiDPI/分数缩放不误点 | root-pixel 真实 Xvfb 与 96/144/192 DPI 专项通过；不代表物理多屏/全部分数缩放已验收 |
| D2 | primary/id/desktop、多屏/变更、无 primary 回退 | 真实 Xvfb 逻辑多屏、primary/id/desktop、同 atom 变更、stale 拒绝及恢复通过；物理设备覆盖仍待验收 |
| D3 | 采集前预算与错误报告 | 真实 GetImage/decode 前预算拒绝测试通过；并非完整内存峰值/所有异常路径证明 |
| I1 | 原生 Unicode/键位规划与恢复 | 安全读取已有真实Xvfb bridge记录。A typed-target+B per-key journal独立源审查无阻断，实际15/6/5/16/7分组及完整191PASS+1ignored；不重复累计。D真实生产dispatch/release提取后16项实测4断言RED/12对照，设备绑定发送修复中；D360限定源码审查通过，独立16+3及全native210PASS/1ignored（10个旧C模型显式排除，分组不叠加）；XACE真实key-up仍HOLD。C原10模型失实已撤回，修正14模型获源码复审且实测4断言RED/10对照。新增writer边界17实测12指定断言RED/5对照；有限public checked/flags0/action-empty writer364已通过独立源码审查和实际map_wire16/partial2/xcb协议peer5、D16/3；map_backend17实际16PASS+唯一groups断言FAIL513。原组有重叠不累计，非真实X writer验收；groups/retirement仍未解决。完整残留归因与真实应用Unicode仍未完成；10月16日新增prevention366真实7项=2PASS/5指定FAIL，原17仍16PASS/groups531FAIL；10月19日共享preflight两文件实现冻结366并通过独立spec/quality源审；独立执行因acer-win SSH不可达尚未开始（10月19日15:55复查为Operation timed out）。groups责任退休设计+addendum及tests-only367新增7项已通过独立spec/quality源审；actual RED未运行、两处remove生产修复未实施 |
| I2 | 明确 clipboard paste、所有权与资源清理 | 真实 Unicode/多行/重复粘贴断言通过；收尾曾出现 fixture 心跳停滞，单次诊断未复现，不算稳定性关闭 |
| I3 | 按键/拖拽/滚轮/hold 与取消清理实测 | 未完成 |
| C1 | X 桌面 writer 所有权及并发测试 | 真实 X11 lease 6 项（含1 helper）+5 protocol 通过；其余生命周期/端到端边界仍保留 |
| C2 | 环境识别一致/初始化期限/remote 恢复 | helper两项真实expiry回归已通过，原26+12+9+原生9仍通过；双ABI源码审查无新增阻断。foreground permit修复源审查通过，原真实admission1、新tracker6、正常8次acquire（1外层测试）及原回归均通过。creator3/witness/实际TIDreuse/消费者迁移仍未完成；raw publication391 A–D源码审查+8实际record/FD回归+x86最终ELF核验通过，creator3仍FAIL但均actualreaps1/资源未退；原permit/expiry回归仍PASS。noGO388实测FAIL、preprctl1PASS、真实TIDreuse未取得NOTQUALIFIED；fixture R1–R3 controls11实测7RED/4对照+delayed1RED；fixture-only396修复候选已审出跨cutoff负采样G1可假RED；397追加actualwait单项已独立执行目标assertionRED204（calls1，23us内false→true，210031us返回旧false，cutoff200000us），已授权最小fixture修复，尚未关闭资格门，也未修产品noGO；后续G1最小fixture398已独立13PASS、日志hash主核，仅fixture资格修正；10月19日对同398安排noGO/delayed-start/preprctl，scp/SSH在远端staging前因Host is down失败，三项均NOTRUN；原资格/产品缺口继续保留；noGO最小leaf设计已独立审查，C1 early-EOF有限分类范围已明确但未实现，C2等待后及时GO正控tests-only399已冻结并通过三目标compile-only，独立源审通过有限门且执行合同日志修正已落地，实跑未完成；均不属于已通过验收 |
| C3 | 全局急停热键 | 安全decoder51+57+3通过记录保留。真实canonical Stage1首个positive因映射有效性撤销而失败，0/116；Stage2真实停服disarm200533µs、refs1、恢复后四grabs消失/精确基线通过。两阶段owned cleanup确认。正向取消/negative replay仍未完成；一次不改行为的真实诊断确认首个失效为NewKeyboardNotify（device3/old3、changed0x3），不证明内容等价；有界重验证382已源码审查并实测新26=25FAIL/1PASS+aux8FAIL（与真实NKN历史分开）；N2追加4实际RED后完成394源码候选（40项及旧回归尚未执行）；源审发现prefetched FIFO额外press绕过candidate检查B1，398实际3项=2PASS/目标FAIL153且原40全PASS；最小400候选已独立源码审查，独立CC三项+40/旧51/57/指定socket3实际154个唯一测试全部PASS（主核原始日志）；仅B1源码/单元阻断关闭，未获live资格。不更改默认map或弱化撤销 |
| F1 | Linux 反馈 renderer 与截图排除 | 产品76原子请求修正已有独立4+117PASS和真实Linux ELF；focus fixture46修正已有独立13+27+13+Rust5=58PASS。10月16日唯一一次默认10case私有Xvfb GUI实际10PASS/rc0，全部focus guard在spawn之前、cleanup无错误，48PPM取回；主核证据archive和100个远端产物hash并查看4张预览（非全量视觉验收）。历史失败保留，正常renderer/observer复用已验证ELF而非新链接。capture exclusion仍unsupported/Host closed；无合成器修改授权，不由GUI通过推导真实截图排除或完整parity |
| Q1 | describe/schema/docs 对齐实际支持 | 未完成 |
| Q2 | Linux 独立单元/契约/真实 GUI/异常验收 | 未完成 |
| Q3 | TLS 端到端与隔离真实应用黑盒 | 未完成 |
| Q4 | Windows 构建/受影响回归、macOS 编译 | 未完成 |

## 当前证据索引（2026-10-16，未完成）
- 原生读取：`.agents/reports/2026-10-16-linux-native-keymap-read-green-1.md` 与 `2026-10-16-linux-native-target-safe-bridge-1.md`。后者保留首次runner参数错误（未启动X/测试）及修正后唯一实际live结果，不能称首次全绿。
- 凭据：`2026-10-16-linux-credentials-launcher-green-1.md` 与 `2026-10-16-linux-credentials-launcher-green-review.md`，测试通过不抵消源码审查缺口。
- 急停：`2026-10-16-linux-hotkey-qualified-runners-1.md`、`2026-10-16-linux-hotkey-wire-decoder-red-1.md`、`2026-10-16-linux-hotkey-wire-red-review.md`。旧Stage2精确abort调用点无backtrace，不认定已定位。
- 反馈：`2026-10-16-linux-renderer-process-eof-red-1.md`、`2026-10-16-linux-renderer-process-eof-green-1.md`、`2026-10-16-linux-renderer-eof-green-review.md`；链接、纯测试与真实UI/capture exclusion分开验收。
- 完整调度、冻结SHA和模型标签审计：`.agents/reports/2026-10-15-linux-parity-progress.md` 最新checkpoint及其引用的原始日志。除明确不变的测试/源码外，不把旧快照通过转移给新快照。

## 第一轮并行边界
### D — 显示 provider（实现 worker）
拥有 src/backend/linux.rs、src/backend/desktop.rs、src/backend/capture.rs、src/backend/mod.rs、新增 src/backend/linux_display/**、Cargo.toml/Cargo.lock（仅所需 Linux 依赖）。不改 dispatch、clipboard、native-input、mcp。
先加入物理几何、primary fallback、选择/预算/拓扑变化回归，通知协调者运行 RED。再接入实际 X11/RandR provider 与现有 DisplayProvider。编译命令：cargo check --locked --all-targets；Linux 编译另由测试侧执行。

### I — 原生文字（实现 worker）
拥有 crates/native-input/src/linux_x11.rs 及拆分子模块、crate 自身测试/manifest（如需依赖先请求，不抢 Cargo.lock）。不改根 backend/mcp。
先加入 Unicode/键位复原/非法文本/空 Wayland 环境单测，通知协调者运行 RED；再实现，避免长时间锁住 X server 或修改其他客户端输入状态。语义上 native 不隐式粘贴。

### 协调者（同时推进）
核实基线、建立本规格与矩阵，检查独立 CC 与 Linux 构建资源，明确后续 clipboard/lock/hotkey/feedback 接口与所有权，安排 RED/GREEN 和独立审查。不重复 worker 实现。

## 后续轮次（仍属完整目标）
- Clipboard：src/backend/dispatch.rs + linux_clipboard 模块 + native clipboard 专项测试。
- Control：src/mcp/lock.rs、worker 初始化、remote 失败分类；同 display 跨临时目录/不同 display/进程退出专项。
- Hotkey：src/mcp/hotkey.rs 的 Linux 适配和注册失败/释放/触发验证。
- Feedback：沿现有 renderer 协议增 Linux 平台，真实 screenshot exclusion + 用户可见交互验收。
- QA：独立 scripts/测试 fixture/README，最终矩阵逐项回填原始日志和结论，不能仅凭 worker 总结完成。

后续具体接口在前置检查后写入对应任务文件；未实施/未验证项不移出范围。
