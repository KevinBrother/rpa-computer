# CC+GLM：原子多击一行测试修复后独立复测

只执行验证，不修改任何源码/测试/脚本，不GUI、不SSH、不commit/push、不启动其他agent。只能写此任务报告和runs。读 .agents/reports/atomic-multiclick-sol-repair-20260930.md 和 .agents/reviews/atomic-multiclick-cc-evidence-correction-20260930.md。对照src/runtime/error.rs与当前测试，确认唯一修改是协议层预期码，无断言被弱化。

注意首轮报告错误声称后续trace全部执行：实际上失败断言后面的calls/held等未执行，且首迭代中断3×2循环。必须保留原失败，按本次完整运行证据报告，不沿用这个推断。

按顺序执行并保存各exit/完整日志：
1. cargo test --lib runtime::session::tests::multiclick::uncertain_press_or_release_cleanup_keeps_pair_metadata_and_skips_later_pairs -- --exact
2. cargo test --lib runtime::session::tests::multiclick
3. cargo test --lib
4. cargo test --tests
不要加--ignored/--include-ignored。任一失败不要自行修复，继续必要收集并报告实现者。若定向用例仍失败，优先定位并停止长全量而不是掩盖失败。--tests可能先运行lib，再integration binaries，要记录实际跑到哪些target、未跑到哪些，不合并虚构计数。

报告 .agents/reports/atomic-multiclick-cc-retest-20260930.md，日志 .agents/runs/atomic-multiclick-cc-retest-*.log。核对当前测试文件hash与修复报告；编译/纯测试不是GUI验收。其它agent独立目录实现feedback/fixture，不碰本次native/root代码。
