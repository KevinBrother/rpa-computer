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
