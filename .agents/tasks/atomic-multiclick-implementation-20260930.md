# 实施：原子多击元数据（用户已确认范围）

你是真实Claude CLI实现者，只在rpa-computer工作。用户已批准完整computer-use案例/实现与可选反馈边界；本任务只修输入内核多击，不实现GUI反馈/多屏。读`.agents/README.md`和`.agents/CONTRACT.md`末尾2026-09-30扩展段，以本任务及该新段为准。无worktree/commit/push；保留所有已有修改，不恢复/覆盖别人改动。禁止任何真实GUI输入、截屏、SSH或启动应用。

## 独占写集
- `crates/native-input/**`（源码/纯测试/必要API文档，尽量不改依赖）
- host Rust内Button定义及透传/cleanup/fake匹配所必需的调用点：`src/backend/mod.rs`、`dispatch.rs`、`dispatch_core.rs`，`src/runtime/plan.rs`及相关测试、`src/runtime/testutil.rs`、`src/runtime/session/tests.rs`、`src/mcp/worker/tests.rs`、`tests/**/*.rs`。
- 若其它Rust文件仅因Button新增字段需机械适配，允许最小调整并最终列明；不改变capture/geometry/MCP公共工具/schema/会话策略。
- `.agents/reports/atomic-multiclick-implementation-20260930.md`与本任务专用`.agents/runs/atomic-multiclick-*`。
不碰acceptance-fixture、Python analyzer、根README/设计/计划/CONTRACT。另一个worker在独立fixture目录工作。

## 已锁定契约
1. `PlanEvent::Button` / `InputEvent::Button`增`click_count: u8`，有效1..=3，含义THIS press/up pair的native click-state。Action Click count=N编译为N对，元数据分别1..N，保持runtime间隔/取消检查；drag、普通单击=1。
2. Backend seam `NativeInput::button`和native crate `Driver::button`显式接同一元数据；standalone Input wrapper调整一致（不为旧签名留假支持），非法count在任何native操作前拒绝。
3. Mac按钮CGEvent使用传入click-state，不再硬编码1。移动/dragged事件依平台语义用明确合适字段，不把普通Move错当第二次点击；原生事件构造可读回字段做无post的API回归。
4. Windows/X11发真实原子按钮事件，OS按时序/位置聚合多击（无原生count字段不意味着不发事件）；正确验证参数，禁止no-op成功。不要发明driver内sleep/多击循环。不要在此任务“顺手修”系统双击阈值，若当前90ms间隔与某配置不符准确报告待测风险。
5. 保留uncertain press先追踪、release成功才forget；按button记录最后press元数据供release_all/Drop匹配，不能HashSet里积累同一个按钮多个count导致重复释放，也不能失败后丢记录。Host仍用raw Driver并以BackendCore为唯一held authority，独立Input wrapper也完整维护自己的状态。
6. 旧文本Unicode/CRLF节奏不要更改，模型抄错不是native问题。不得引入UI/MCP/LLM/截图依赖到native crate。

## TDD / 验证
- 先最小回归（不是只报缺符号编译错误当行为red）：旧click_state=1导致期待2/3的测试失败或旧plan元数据丢失的可观察断言失败，保存red日志，再修实现。
- 覆盖 count1/2/3传递和native字段；count0/4拒绝零派发；drag始终1；多击取消时清理；press失败/release失败及第二次cleanup metadata保留；release_all所有键/按钮尽力释放；独立session wrapper Drop同样验证。
- 运行`cargo test --manifest-path crates/native-input/Cargo.toml`、root `cargo test --lib` / `cargo test --tests`（不得运行ignored真实输入测试）以及Mac release构建。不要启动computer-host实时后端。自检不能post任何CGEvent/SendInput。
- Windows/Linux跨平台编译可只做现有工具链支持的构建；不要安装新工具/改系统。协调者稍后做真机Windows纯测试和GUI。
- 编译/测试失败要定位修复，不挪走/删除旧测试；大文件新增逻辑拆模块，根session.rs接近1000行不要继续堆。

最终报告：改动文件列表、精确red/green命令日志路径和exit情况、actual接口设计、仍待平台构建/真机项。不要宣称双击GUI已验收，不更改旧GUI统计。
