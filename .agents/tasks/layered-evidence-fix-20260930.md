继续你已实现的分析器，只写 scripts/analyze-layered-gui.py、tests/layered_gui_analyzer.py 与自己的 .agents/reports/layered-evidence-analyzer-20260930.md，不改其他文件，不GUI，不commit/push。独立审查 .agents/reviews/layered-evidence-20260930.md 已有确认反例，先读再修。请直接做最小正确修复并补回归，不再展开长篇设计。

必须修：F1 exact pass同时门控三层比较/hex/result/必要字段，不允许verdict失败但pass计数true；F2放弃全局第N个payload配第N个check的盲配，利用trial区间+输入时间+check时间，按case构造唯一可证明的配对（支持Mac秒精度oracle区间，同秒多候选或缺timestamp无法证明时unknown）；输入已发送但缺check不能错配到下一case，重复check不能偷用下一payload或升级first-pass。F3 missing matched明确失败类别；F4任何assistant缺model需标未知/失败。

协调者额外发现：knownCRLF原始task_payload是含\r\n的串，agent实际发送原始串是正确的，layer1不能把它与normalize后的catalog期望直接raw比较而误报。记录raw_payload_vs_task_payload独立字段；known layer1用原始task_payload比较（用户要求精确输入），layer2用normalize_newlines(payload)比较actual；expected应用文本另外存oracleExpected，不全局做unicode归一化。添加测试：发送原始CRLF→应用LF应exact；agent改成LF需至少报告raw task偏差（不算严格raw通过），不要只覆盖LF→LF伪CRLF案例。

增加回归：遗漏case1的check而case2成功；duplicatecheck后新payload未到；倒序timestamp/一个缺ts；2个同秒候选；跨case重复文字仍不能误配；timestamp+tool_result完成时间能建立因果顺序时唯一配对；当前17例合理更新，不能减掉覆盖。缺失oracle ts也要unknown。若可信时间信息不足，就保留期望/actual正确性和参数对齐unknown两套字段，不臆断注入正常。

执行全部pure tests，把结果和修复字段补报告；总单文件仍<1000行。审查报告由reviewer持有，不改它。
