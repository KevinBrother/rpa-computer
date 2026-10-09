# Agent 工作区：Computer Runtime

协调者：Codex，只负责设计、任务 Markdown、调度、审查和执行验证命令。所有产品源码、测试与构建/部署脚本由 Claude Code 编写或修复。

用户授权：可运行多个 Claude Code Agent；本机 macOS 和 ssh acer-win 的 Windows 编译产物都要验证。不要 commit/push；不要 worktree；不要改父目录其他项目。

## 边界

- 所有命令从 rpa-computer 根目录执行。这里是独立 Git 仓库；保留已有文件，允许按新契约重构本项目。
- `.agents/CONTRACT.md` 是协作接口；详细设计在 docs/2026-09-24-computer-use-runtime-design.md。冲突时以最新任务和 CONTRACT 为准，并报告冲突。
- Claude 实现 Agent 可以看源码；白盒测试 Agent 可以看源码；真实 computer-use 验收 Agent 不得读取源码，三者不得混淆。
- 子任务必须有不重叠的写入范围；共同 Cargo.toml 和 lib.rs 由 core Agent 单独管理。
- stdout 不得泄漏凭据或敏感图像；日志放 `.agents/runs/`，报告放 `.agents/reports/`。不给测试 Agent 这些路径。
- 不改用户全局 Claude 配置、不打印凭据、不自动切换模型；使用现有配置。
- 真实桌面输入需协调者统一串行调度；实现 Agent 不擅自调用鼠标键盘，不操作其他应用。
- 对输入与截图失败、取消、清理失败、未知结果必须如实报告，不用测试 stub 冒充真后端。

## 角色与文件所有权

| 角色 | 写入范围 |
|---|---|
| core | Cargo.toml/Cargo.lock/src/lib.rs、src/runtime/**、旧公共模块清理、tests/protocol.rs 迁移、.gitignore |
| backend | src/backend/**；自身测试写在模块内 |
| transport | src/mcp/**、src/bin/**、examples/** 配置，不写旧 calc_loop.rs |
| qa | tests/contract*.rs、tests/mcp*.py、scripts/**、测试任务文档、README.md |
| visual-fixture | acceptance-fixture/**（独立原生 GUI 验收靶场，不修改产品源码/QA scripts） |

旧 examples/calc_loop.rs 由 core 删除/替换，避免旧模型调用示例和新架构混淆。

## 交付门禁

1. 实现必须经过独立审查；worker 的“通过”不是验收结论。
2. macOS 和 Windows 都必须有真实构建结果；Windows 在交互式桌面启动，不能在 SSH 服务会话直接操作 GUI。
3. 黑盒验收从独立发布目录启动，只含编译产物、MCP 配置、任务。工具清单只能有 computer 工具；需要 OS 文件隔离证据，不能仅用提示词要求不读源码。
4. 黑盒验收先图片链路、再靶点、再计算器/文本编辑；保留显式允许的测试截图和调用证据，不能用心算代替 GUI。
5. 完成矩阵见 plans/implementation.md；未满足项不得被删掉以宣布完成。

## 当前模型策略（2026-09-29 用户明确覆盖旧约束）

用户要求本项目原 Kimi 执行任务全部改为 GLM，并明确要求修改本机及 acer-win 的 Claude 用户级全局配置。两端默认 model=sonnet；既有 sonnet 路由对应 glm-5.3-flash；旧 fable 映射也改到各自主机的 sonnet 路由。此授权覆盖上文“不改用户全局 Claude 配置/不自动切模型”的旧约束，仅限此次已批准的 GLM 迁移。保留其他模型、认证和网关设置。后续实现、审查和 GUI 验收用 GLM，不能静默退回 Kimi。响应 model 标签需以实际转录核对；历史 Kimi 证据保留不改写。

## 当前代码实施策略（2026-09-30 用户明确覆盖）

用户最新要求：后续产品源码、测试和构建脚本由 **gpt-6.1-sol subagent** 实施，而非真实 Claude CLI/GLM。本条覆盖本文及旧任务中“所有源码只能经 Claude Code 编写”的限制；旧 CLI 两条未完成任务已按身份停止，保留全部部分改动和原始日志，新 subagent 从现状接手，不重置工作区。真实桌面黑盒验收仍沿用 Claude CLI + GLM，除非用户另行更改。协调者负责范围划分、集成审查和验证。不 worktree、不 commit/push、不改父目录其他项目；实现者不得进行真实桌面输入/启动 GUI/SSH。按实际证据报告，不能把中断的旧任务当作已完成。

### 2026-09-30 用户进一步明确：开发与测试分离

- Codex + `gpt-6.1-sol` subagent 只负责产品源码、测试代码和构建脚本的实现/修改。
- 测试执行、独立验证与验收统一交给真实 Claude Code（CC）+ GLM，尤其所有真实GUI点击/拖拽/滚动/键盘输入。不得让Codex subagent替代CC执行桌面测试。
- 开发者可做实现必需的编译检查，但不得据此宣布测试验收通过。此前已经执行的纯测试仅保留为开发自检证据，后续由CC独立执行。
- CC测试发现的问题交回Codex + gpt-6.1-sol修改；CC验证任务不兼任产品源码实现者。协调者负责分配、环境编排、证据审查与进度记录，真实GUI串行且仅操作自有测试资源。

### 2026-09-30 最新测试优先级：仅Windows先行

用户明确“先不用测试mac和linux，先测试windows”。从该指示起不再启动Mac/Linux测试或GUI验收；已启动的Mac旧文本测试已按身份停止，保留partial/中断证据，不计通过。后续CC+GLM测试先在acer-win Windows执行（Mac上交叉编译Windows产物不属于Mac测试）；源码实现职责仍为Codex+gpt-6.1-sol。Mac/Linux未测项保留pending，不能删除矩阵或当通过。独立跨平台代码可以保留，但不得以等Mac/Linux验收阻塞Windows门禁。
