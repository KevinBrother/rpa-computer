# Layered GUI Analyzer 修复复核（2026-09-30）

- 对象：`scripts/analyze-layered-gui.py`（修复后）、`tests/layered_gui_analyzer.py`（26 tests）
- 方式：只读 + 纯离线运行；反例 fixture 均在 /tmp，未改产品/测试文件，未操作 GUI
- 测试基线：`python3 tests/layered_gui_analyzer.py` → **Ran 26 tests, OK**（exit 0）

## 上轮缺陷修复验证

### F1（阻塞）已修复 ✅
- `analyze_attempt` 的 `exact` 现在门控全部三层：`scripts/analyze-layered-gui.py:397-404` 要求 `result ok + hex ok + matched True + layer1 + layer2 + layer3`。
- 上轮反例 R1 重放（catalog `aaa`、模型实发 `bbb`、oracle expected==actual 且 matched=true）：`verdict=payload_mismatch`，`exact=False`，suite `final_exact=0`，failures 含 `raw_task_payload_deviation` + `input_divergence`。不再洗白。
- 回归测试：`tests/layered_gui_analyzer.py:760-774`（`test_payload_mismatch_never_counts_as_pass`）。

### F2（对齐）已修复，盲 N 配对已移除 ✅
- 全局第 i↔i 配对已删除，替换为 per-case 可证明性配对：payload 先经 oracle trial 区间归属 case（`assign_payloads_to_cases:450-471`，`bisect_right`，trial 同秒并列或 payload 无 ts 均归 unassignable），再在同 case 内要求唯一因果候选（`build_report:560-592`：`payload_ts <= check_ts`，且 tool_result 完成时间不晚于 check；0 候选 / 多候选 / oracle 无 ts 分别记 `no_candidate_payload` / `ambiguous_candidates` / `missing_oracle_ts` 为 unresolved，绝不猜测）。
- 实测边界：payload ts == check ts 时可 proven，但无 tool_result 则 `missing_result` → 不会 exact（fail-closed）。
- 关键回归测试均通过：
  - case1 漏 check 不偏移 case2：`tests:548-575`，c1 `payload_without_check`、`final_exact=None`，c2 exact。
  - 同秒多候选不 exact：`tests:406-429`、`tests:656-676`（macOS 秒精度场景），全部 `ambiguous_candidates`，`final_exact=None`。
  - 跨 case 同文本不误配：`tests:709-758`（含反向变体：payload 早于下个 trial 属于前 case，后 case `no_candidate_payload`）。
  - 乱序 / 缺 ts 均降级 unknown 且不污染健康 case：`tests:610-654`。
  - tool_result 完成时间证明配对：`tests:678-707`。

### F3（matched 缺失）已修复 ✅
- `scripts/analyze-layered-gui.py:383-384` 新增 `oracle_matched_missing` failure；verdict 链 `:412-413`。
- 实测：无 `matched` 键且其余全匹配 → `verdict=oracle_matched_missing`、`exact=False`、failures 含 `oracle_matched_missing`。

### F4（model 旁路）已修复 ✅
- `parse_transcript:201,209,216-217` 统计 `assistant_events`/`missing_model_events`；`build_report:484-497` 要求 `observed==[glm-5.3-flash]` 且 `missing_model_events==0`。
- 测试 `tests:445-459`（一条带正确 model + 一条缺失 model → `model_check.ok=False`）。

## Known 原始 CRLF→LF 契约 ✅（比上轮更严格）
- layer1 现为 **raw 精确比对**（`:350-356`，`expected_source="catalog.task_payload(raw, exact)"`），不再归一；同时独立记录 `raw_payload_vs_task_payload` 与 `normalized_payload_vs_task_payload` 信息字段（`:369-374`）。
- 发送原始 CRLF、应用内 LF、oracle LF → exact（`tests:220-236`）；发送 LF 而 catalog 为 CRLF → `raw_task_payload_deviation`、`final_exact=False`（`tests:238-255`）。方向正确、不洗白。

## NFC/NFD、NBSP ✅
- 无任何 Unicode 归一化；`tests:257-290` 确认 NBSP 与 NFD 差异均 `input_divergence`、`final_exact=False`。

## first-pass 不被重试提高 ✅
- `first_exact=attempts[0]["exact"]`（`:599`），仅 proven attempts 参与且按 check 顺序；`tests:314-338` 双 attempt（先败后成）→ `first_attempt_exact=False`、suite first=0。

## fixture manifest schema 与 CLI catalog 格式（非阻塞，需注意）

真实 manifest `.agents/runs/layered-fixture-build-20260930/cases-manifest-macos.json` 的 schema 为：

```
{format:"acceptance-fixture-cases", version:1, warning:"COORDINATOR-ONLY...",
 suites:{ "<suite>": { cases:[{id, task_payload, expected_text}, ...], total:N } } }
```

要点：
- case 字段名是 **`id`**（如 `known-01`），非 `case_id`；无 per-case `suite`（由 suites 键名隐含）。
- 分析器 `load_catalog`（`scripts/analyze-layered-gui.py:282-317`）支持 4 种格式：`{cases:[...]}`、`{suites:[{suite,cases}]}`、`{suites:{name:[case,...]}}`（值为**列表**）、顶层裸列表。
- **不**支持 manifest 的 `{suites:{name:{cases:[...],total}}}` dict-of-dict 形态。实测把 manifest 直接喂 `--catalog`：`load_catalog` 返回 0 个 case（dict 被 `for c in lst` 迭代成字符串键后跳过，**静默无错**），所有 case 落为 `in_catalog=False`、layer1 回退 oracle expected_text（实测 verdict 仍 fail-closed）。
- 结论：当前 coord 生成 canonical catalog `{cases:[{suite,case_id,task_payload,expected_text}]}` 即可正常工作，**不构成运行阻塞**；但建议（非必须）让 `load_catalog` 对 dict-of-dict 形态或 0-case 结果给出 warning，避免静默降级到 unknown 模式。

## 覆盖范围边界（明确声明）

分析器**不覆盖**最终 result 判定与工具策略：
- 只做 text/oracle 三层比对 + hex 一致性 + 点击 hit/wrong 计数统计；不审计工具调用序列、不验证截图视觉效果。DISCLAIMER（`:41-45`）与模块 docstring（`:25-28`）均明示"does NOT replace a strict tool-policy audit / does NOT prove screenshots looked correct"。
- 报告无任何"run 通过"聚合字段；`exact` 是 per-attempt/per-case 文本层结论。**coord 必须另跑 strict tool-policy audit，且不能把 case exact 计数当作完整 run 通过**——分析器自身不产生也不暗示该结论，符合要求。

## 门禁结论

F1–F4 修复全部实际生效，26 测试真实通过，盲 N 配对确认移除且各未知/歧义场景均降级 unknown。无新引入阻塞项。唯一改进建议（非阻塞）：`load_catalog` 对不识别的 manifest 形态应告警而非静默 0-case；strict audit 与 run 级判定仍由 coord 承担，分析器边界声明明确。
