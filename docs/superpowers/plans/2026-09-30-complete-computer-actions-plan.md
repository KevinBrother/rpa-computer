# 通用动作、多屏及可选桌面反馈实施计划

> 执行方式：主工作区，不worktree、不commit/push；所有源码/测试代码/构建部署脚本由Codex + gpt-6.1-sol subagent实现；测试执行与独立验收由真实Claude Code + GLM负责，协调者负责设计/任务/编排/证据审查。用户已确认范围，不再把“可选反馈”误写成输入内核依赖。

**目标：** 补齐通用输入能力、可执行原生动作case、跨应用验证、多屏及项目自带可选桌面反馈。所有失败/未验证门禁保留，阶段结果不代替完整交付。

**架构：** native-input原子输入；Runtime时序/会话/取消/状态；Host接入可选展示模块；接入方可替换品牌。多屏捕获/拓扑映射独立模块。反馈关闭路径无GUI依赖。首版提示只要求明确标识、轻量鼠标反馈和真正停止，不要求蓝色光晕。

**参考：** `../specs/2026-09-30-complete-computer-actions-design.md`、`../../computer-use-acceptance-cases.md`、`.agents/CONTRACT.md`最后扩展段。旧文档里Enigo/主屏限定等是历史，不作新的完整交付结论。

## 阶段A：原子输入语义（可立即实施，无桌面输入）

- [ ] 先写回归证明Mac固定click_state的问题与metadata丢失；尽可能构造native事件但绝不post到用户桌面，保留fail-first日志。
- [ ] 统一Button click_count原子元数据：Action多击plan index1..N贯穿到Driver；driver不加耗时循环、取消仍归Runtime；drag=1。
- [ ] Windows/X11真实按钮事件+元数据有效性验证，明确系统时序聚合；Mac事件字段正确；cleanup保留最后press元数据并只在释放成功后forget。
- [ ] 更新纯测试/fake callsites，验证单/双/三击、非法参数无派发、取消/失败release、文本/scroll/drag不回归；不修改Unicode输入语义来“修”模型抄错。
- [ ] 无反馈模块/无新UI依赖情况下native crate与root纯测试、macOS release、Windows交叉构建通过；Windows原生测试exe在acer-win执行（SSH仅无GUI测试）。

## 阶段B：可执行原生动作靶场（与A写集互斥）

- [ ] B1 首批multiclick、drag、scroll各10个case，两平台原生控件、可见caseID/nonce/说明/结果、app-only事件oracle；语义与原始OS字段分别记录。
- [ ] B1 配套纯catalog/判定自测、真实Mac构建、Windows csc构建、导出parity；新分析器将工具参数/事件/屏幕证据关联，明确缺失/unknown，不按N对N盲配。
- [ ] B1 新真实GLM任务，沿用8工具、source-free、禁止shell/a11y/CDP；计划行为不得自称已发生。
- [ ] B2 补pointer/keyboard/cancel/focus/geometry其余suite；不得因为首批3suite完成缩小完整矩阵。
- [ ] 文本known09按平台原生expected修正新增run的检查器，原始actual不做宽松normalize；旧9/10记录保留。
- [ ] 每基础动作至少10次首次有效操作；非法输入/零量/重试不凑成功率，随机layout不冒充新语义case。

## 阶段C：独立审查及安全构建门禁

- [ ] 独立Claude审查新增IR/cleanup、fixture实际事件判定、分析器关联及文件边界；修实际问题，不盲从过时报告。
- [ ] 协调者调度CC+GLM重跑所有受影响纯测试/真实构建并确认实际GLM标签；禁止单元测试/工具审计替代GUI通过。
- [ ] windows-rs/macOS系统API/X11底层输入保留，不回退Enigo/xdotool；产品源码不引入LLM循环。
- [ ] 文件拆模块，避免继续向接近1000行的session.rs堆积拓扑/展示逻辑。

## 阶段D：多显示器（不等实机才能编码）

- [ ] 明确DisplaySnapshot、选择单屏/全桌面、运行期displayID及拓扑版本；扩展open/describe而非让worker发明多个工具协议。
- [ ] 全桌面区域映射与像素预算、负坐标、上下排列、混合DPI、padding/gap、跨屏drag；纯用例先覆盖。
- [ ] stale观察/拔屏/布局变化拒绝；动作中变化取消/释放/如实报告，不偷偷回退主屏。
- [ ] 用户提供多屏后执行multi-01..12实机；缺布局仍pending，不用同一截图冒充全部验证。

## 阶段E：项目自带可选桌面反馈

- [ ] 独立state snapshot/event契约与optional sink，默认no-op路径完整可运行；renderer不能反向成为native-input依赖。
- [ ] Host侧受控桌面展示模块：明确操作标识、轻量AI指针/点击反馈、暂停/停止入口；颜色/文案/品牌可配置/替换。
- [ ] 控制权/observing/executing/paused/cleanup/fault/closed从真实Runtime来；不得让Agent猜显示/消失；断连和崩溃不假称成功释放。
- [ ] 装饰点击穿透与停止按钮独立命中区域；不抢焦点；停止入口真正走取消/清理并反馈。
- [ ] Windows/macOS各自capture exclusion实测；当前截图后端不支持时做明确后端配合，不靠截图后涂抹，不把透明属性当排除证明。
- [ ] 未启用/未加载/不打包反馈模块，输入内核仍可构建并完整工作；显式启用但模块不可用不得悄悄报告已显示。
- [ ] 光晕/特殊动画是后置可选润色，不作为核心能力完成的替代。

## 阶段F：完整真实GUI验收及清理

- [ ] Windows Session1 ScheduledTask，动态网络配置，真实Claude CLI+GLM；SSH只部署/编排/取证。只操作自有靶场/文档，不操作用户Notepad24332。
- [ ] macOS只在活动可见桌面运行，不自动解锁/改TCC；剩余旧文本3suite与Calculator3次仍须补完。
- [ ] 跨应用crossapp-01..06，输入/焦点/scroll/drag完整链路；无源码/无a11y/CDP视觉绕路。
- [ ] 多屏/反馈开关与停止/截图排除；真实模型失败由协调者看图核查，不改原始失败。
- [ ] 按身份清理自有进程/任务/端口与输入状态；保留旧证据，报告所有未完成/失败/环境阻塞，不宣称整个范围已完成。

## 实施记录

2026-09-30：用户明确确认“项目自带可选反馈模块、不绑输入内核、品牌可配置/替换、基础标识/轻量反馈/有效停止优先”。本计划据此分阶段；本段不表示源码或GUI已完成。

2026-09-30（角色切换后的首轮验证）：原子多击源码由gpt-6.1-sol接手交付；CC+GLM独立审查并执行native macOS纯测试72/72、Windows目标机纯测试57/57；Windows release两个bin真实交叉构建exit0。root lib首轮225 passed/1 failed/1 ignored，失败是新测试把协议层input_error错断言为backend层input_failed；已由Codex最小修正，CC复测进行中，不能提前记green。原首轮CC报告关于失败断言之后的覆盖推断有纠错说明，保留原失败和纠错。真实GUI尚未执行本轮新动作。

环境：CC只读预检（17:56）证实Mac两屏active/asleep0，Windows console Session1 active，用户Notepad24332保留；活动状态不证明解锁，GUI前仍需截图门禁。反馈库、默认renderer与三套gesture fixture仍在独立写集实现；Host接线任务已细化，完整多屏及其余case未完成，不能以本批替代。
