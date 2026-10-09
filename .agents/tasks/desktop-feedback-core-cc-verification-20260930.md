# CC+GLM：独立反馈基础库验证

最新角色：你只审查/执行测试，不写/修改源码、测试代码或脚本。工作目录本项目，不worktree、commit/push、GUI、截图、SSH，不启动其它agent。只可写本任务report/runs。crate源码已冻结，Host接线尚未实施。

阅读 .agents/reports/desktop-feedback-core-sol-20260930.md、crates/desktop-feedback/{README,IMPLEMENTATION}.md、.agents/tasks/desktop-feedback-v1-wire-20260930.md 及实际代码。

按顺序：
1. 规格独立检查：严格v1和16KiB有界wire、latest snapshot、session/generation授权、ready/heartbeat sticky故障、off不解析路径/不spawn、无输入/UI依赖、surface.version接受真实逗号格式。不能把抽象ProcessControl fake测试说成实际子进程或GUI已验证。
2. 质量独立检查：队列/部分write/序号/锁竞争/过期stop/清理token，确认代码真实保证上述契约；报告具体证据，不脑补pass。
3. 执行并保留日志/exit，各条互相独立：
   CARGO_TARGET_DIR="$PWD/crates/desktop-feedback/target" cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked
   CARGO_TARGET_DIR="$PWD/crates/desktop-feedback/target" cargo clippy --manifest-path crates/desktop-feedback/Cargo.toml --all-targets --locked -- -D warnings
   cargo fmt --manifest-path crates/desktop-feedback/Cargo.toml --all -- --check
   最后一条仅check不改代码。失败不得自行修，报告回协调者。
4. 列实际执行target/计数与未执行项。33是实现者测试源码计数，不先认定真实执行数。别把当前数据结构逻辑通过当Host安全停止/截图排除通过。

报告 .agents/reports/desktop-feedback-core-cc-verification-20260930.md；日志 .agents/runs/desktop-feedback-core-cc-*。诊断不泄漏敏感信息。
