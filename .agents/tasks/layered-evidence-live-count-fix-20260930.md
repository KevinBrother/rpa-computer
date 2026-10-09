从真实live轨迹发现两个统计边界需要最小补充，只写你原有 scripts/analyze-layered-gui.py/tests/layered_gui_analyzer.py/自己报告，不GUI不commit。不要重写对齐算法。

1. 真实.partial输入 .agents/runs/layered-win-baseline-live-analysis-20260930.json：只有3text_check和4trial，却suite.observed_case_count=10。源652行 observed_case_count=len(suite_cases ids)把catalog里的未出现cases也算observed。请改为只根据oracle实际trial/hit/check event出现的distinct(suite,case_id)计数；total expected case_count继续catalog10。增加partialrun4trial3check的回归 => observed4/expected10/checked3。
2. 源599行 first_exact=attempts[0].exact 仅看proven paired attempts，若同case首次text_check未能对齐unknown、后续retry成功，不能升级first-pass=true。请按最早check/输入顺序综合unresolved与proven，最早未证明时first_attempt_exact=None（或明确false但需unknown类别），后续成功可final_exact=true但first_pass不变。类似最后check未证明时final_exact不能保留上次true（last remainsunknown）；保留完整attempts/unresolved字段。不洗白首次/末次unknown。新增3类回归：首unknown后success、首failure后unknown、success后unknown，均不可将unknown升级passes。

执行全部pure tests并更新简短报告；文件<1000行，不扩展其他scope。
