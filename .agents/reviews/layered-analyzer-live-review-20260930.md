# Layered Analyzer 边界补丁只读复核（2026-09-30）

- 审查角色：独立只读复核（非 worker 自证）；未读 GUI、未改任何产品/测试源码、未 delegate、未 commit。
- 复核对象：`scripts/analyze-layered-gui.py`（check_sequence / first_attempt_exact / final_exact / observed_case_count）、`tests/layered_gui_analyzer.py` 新增 partial/unknown 回归。
- 真实输入：`.agents/runs/layered-win-baseline-final-{oracle,transcript}-20260930.jsonl` + `layered-baseline-catalog-20260930.json`；中间 partial live 输入 `layered-win-baseline-live-{oracle,transcript}-20260930.jsonl`。

## 测试套件独立重跑

`python3 tests/layered_gui_analyzer.py` → `Ran 30 tests ... OK`（本机独立重跑，非转录 worker 报告）。新增回归覆盖：`test_partial_run_observed_vs_expected`、`test_first_unknown_then_success_not_upgraded`、`test_first_failure_then_unknown_final_stays_unknown`、`test_success_then_unknown_final_stays_unknown`。

## 关键统计逐项核实

### 最终 Windows 完整运行（10-case baseline）
- oracle 独立计数（逐行解析，不经脚本）：`trial=10, hit=10, text_check=10`，catalog 10 cases（baseline-01..10）。
- transcript 独立计数（自写脚本遍历顶层 tool_use）：text_input=10，与脚本 `text_input_count=10` 一致。
- 用当前脚本独立重新生成分析（不经既有 analysis 文件）：`observed_case_count=10, attempts=10, proven_pairs=10, unresolved=0, first_attempt_exact=10, final_exact=10`，全部 verdict=exact，missing=[]，model_ok=True（glm-5.3-flash）。与 coord 已有 final-analysis JSON 一致，可复现。

### 中间 partial live 运行（不可计 observed10 的验证）
- live oracle 独立计数：`trial=4, hit=4, text_check=3`。
- 用当前脚本对 live 输入重跑：`observed_case_count=4, attempts=3, case_count=10`，missing=['baseline-05'..'baseline-10']。
- **发现（非阻塞）**：`.agents/runs/layered-win-baseline-live-analysis-20260930.json` 中记录 `observed_case_count=10`，与当前脚本对同一输入的输出（4）不一致。该 artifact 是修复前旧版脚本生成，属于陈旧 coordinator 产物，建议重新生成或在引用时标注失效；当前代码语义已正确，不构成产品阻塞。

## 边界语义独立推理（非 GUI 反例）

1. **首 unknown 后 success 不洗白**：`first_exact` 取 merged sequence 首元素，unresolved 首位 → `first_attempt_exact=None`；suite 级计数只累加 `is True`，None 不计入。反例构造（check 先于 payload 落盘、重试后 proven exact）被 `test_first_unknown_then_success_not_upgraded` 覆盖，`check_sequence[0].unresolved_reason=no_candidate_payload` 保留。✓
2. **success 后末尾 unknown 不继承**：`final_exact` 取末元素，trailing unresolved → None，suite `final_exact` 仍为该 case 计 0，proven 成功不会升级为 final pass。`test_success_then_unknown_final_stays_unknown` 与 `test_orphan_check_unknown_no_payload` 覆盖。✓
3. **重复 check**：同 case 同 ts 重复 check 时第二个 payload 已消费 → `no_candidate_payload`，保留为 unresolved，`attempts` 不膨胀，final 保持 None，且不能偷取下一 case 的 payload（trial-interval + per-case 归属保证）。`test_duplicate_check_does_not_double_count`、`test_duplicate_check_before_next_payload_not_stolen` 覆盖。✓
4. **missing cases**：catalog 中无 oracle 事件的 case 保留 `missing=True`、列入 `suites.missing`，first/final 计数不含它。`test_incomplete_case_missing_from_oracle`、`test_missed_case1_check_case2_still_success` 覆盖。✓
5. **observed_case_count**：仅统计 oracle 实际出现 trial/hit/wrong/text_check 的 distinct case_id；partial live 实测得 4 ≠ case_count 10。✓
6. **exact 全门控**（result_status/hex/matched/layer1/2/3 全通过才 exact）：F1 反例（payload 错但 oracle 内部 expected==actual）由 `test_payload_mismatch_never_counts_as_pass` 覆盖；tool_result 内容不遍历不落盘（nested tool_use/image 均不计、不 log）。✓

## 结论

- 未发现阻塞。当前脚本与测试的边界语义经独立重跑与非 GUI 反例推演均成立；最终 Windows 运行的 10/10/10 统计可由当前脚本独立复现，partial live 的 observed_case_count 正确收敛为 4。
- 唯一残留：`.agents/runs/layered-win-baseline-live-analysis-20260930.json` 中旧版 `observed_case_count=10` 为陈旧 artifact，建议重新生成（coordinator 权限范围内），不阻塞。
- 按任务约束，本报告不宣称双平台 GUI 验收结束；最终 10/10/10 仅为 oracle/文本层诊断结论，不替代严格 tool-policy 审计与截图证据。
