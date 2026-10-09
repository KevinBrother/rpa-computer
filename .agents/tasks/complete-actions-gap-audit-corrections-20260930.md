# 只读审查报告纠错，不实施

原报告`.agents/reviews/complete-actions-gap-audit-20260930.md`已读，但有准确性/架构问题。禁止改任何源码/测试/脚本/用户设置，禁止GUI。只写新报告`.agents/reviews/complete-actions-gap-audit-corrections-20260930.md`，旧报告保留，不commit/push。

请重新直接核对：
1. 最新`.agents/reports/layered-gui-validation-20260930.md`后半段：Mac -e2 baseline已经10/10文本、HIT9/10；-f50自检已通过但GUI待测。不是“四suite全部未完成/-e header未验证”，这些旧进展不得作为当前结论。Windows baseline原helper5分钟到期的描述需核查该suite真实时间和证据，不能把历史旧helper问题张冠李戴。
2. P0只能说当前Mac click_state固定1是代码缺口、不能保证双/三击；没有应用GUI证据就不要断言所有应用实际事件均count1、一定失效。
3. 不接受把Plan多对press/release压成一个driver.click(count)耗时循环：这把runtime取消/睡眠调度藏进native driver，破坏提取库边界及可取消性。评估每个Button原子事件携带click-count/state元数据的方案，保持runtime掌管时序/取消，driver只做原子派发；Windows/X11没有原生count字段时明确由OS时序聚合，测试真实效果。
4. Linux/X11不允许no-op成功或忽略count的假支持，只能正确系统事件或明确unsupported；不要无根据断言当前全平台编译有Rc/RefCell阻塞，查当前代码/真实报告。
5. 准确核对引用文件名/行号；当前trait名Driver不是NativeInput。报告说明哪些是纯代码事实，哪些是预期/推测/真机待证。审查新设计doc第3/4/6节与用例规范的状态声明是否一致。

新设计：`docs/superpowers/specs/2026-09-30-complete-computer-actions-design.md`
用例：`docs/computer-use-acceptance-cases.md`

输出中文准确纠错结论；不借此实现新功能，不运行桌面。仍使用用户既有sonnet->GLM配置，不改全局路由。
