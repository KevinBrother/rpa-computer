# CC+GLM独立审查与纯测试：原子多击

当前目录本项目；你只负责验证，不写/修任何产品源码、测试代码、构建脚本。源码由Codex+gpt-6.1-sol实现。只能写本任务专用报告/runs。无GUI输入/截图/启动窗口/SSH、不运行ignored、不commit/push、不修改全局设置；不启动agent。其它agent在feedback独立目录与fixture写代码，本任务只测native crate/root Rust原子输入，root源码暂冻结。

先读 .agents/tasks/atomic-multiclick-implementation-20260930.md、.agents/reports/atomic-multiclick-sol-20260930.md，与实际代码核对。
1. 先规格审查：原子count1..3贯穿Runtime->BackendCore->Driver；Mac真实event字段，Win/X11真实原子事件；非法count前置拒绝；Runtime掌管时序/取消；不改变文本语义；Host唯一held authority。
2. 再质量/缺陷审查：重点 Plan::is_answered_release 对多击/多modifier是否错误释放没按下的按钮，uncertain press/release失败保留元数据，重复按钮不双重held；确认测试测生产构造而非平行oracle；不将派发成功当GUI成功。只报告可定位证据，不猜测过时Enigo方案。
3. 实际运行并独立记录exit/完整日志：
   cargo test --manifest-path crates/native-input/Cargo.toml
   cargo test --lib
   cargo test --tests
   cargo build --release --bin computer-host
   绝不加--ignored/--include-ignored，不运行Host。失败不要改代码/改测试，报告交回实现者。
4. 校验报告freeze hash是否一致（实现者报告给出manifest位置），如果不一致说明具体原因/范围；不以dirty仓库本身判断失败。

写 .agents/reports/atomic-multiclick-cc-verification-20260930.md 与 .agents/runs/atomic-multiclick-cc-*.log。列规格、质量分别结论，测试精确pass/ignored、编译exit，未测Win真实目标/GUI。macOS没有GUI操作，本批纯测试通过不等于所有通用能力通过。
