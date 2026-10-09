# Layered GUI Analyzer 独立代码审查（2026-09-30）

- 审查对象：`scripts/analyze-layered-gui.py`、`tests/layered_gui_analyzer.py`
- 方式：只读审查 + 纯离线运行（importlib 导入脚本模块，stdin 临时反例，全部 fixture 位于 /tmp，未修改产品/测试文件，未操作 GUI）
- 测试基线：`python3 tests/layered_gui_analyzer.py` → Ran 17 tests, OK（exit 0）

## 结论

发现 2 个会将错误 case 判为通过的确认缺陷（F1、F2），2 个弱化项（F3、F4）。未发现 NFC/NFD、NBSP、CRLF、hex、unknown/截断洗白方向的阻塞缺陷。

## F1（确认，高危）：`exact` 不被 layer1/layer2 门控，payload 错误仍计入 final_exact

- 位置：`scripts/analyze-layered-gui.py:357-362`。`exact = result_status=="ok" and hex_status=="ok" and matched is True and layer3`，不含 `layer1` / `layer2`。而 `first_exact`/`final_exact`（`:533-534`）与 suite 汇总（`:582-583`）直接消费 `attempt["exact"]`。
- 后果：模型发送了错误 payload（layer1 失败、甚至 layer2 input_divergence），只要 oracle 内部 expected==actual 且 matched=true，case 的 `exact`/`final_exact` 即为 True，suite 的 `first_attempt_exact`/`final_exact` 计数将其算作通过——verdict 是 `payload_mismatch` 但计数是 pass，报告自相矛盾，且 run 级汇总被洗白。
- 反例（实际运行验证）：
  - R1：catalog task_payload/expected=`aaa`，模型实际发送 `bbb`，oracle check expected=`aaa` actual=`aaa` matched=true → `verdict=payload_mismatch`，`exact=True`，`final_exact=True`，suite `final_exact=1`，failures 含 `payload_mismatch(known)` 和 `input_divergence`。
  - R2：payload=`abd`，oracle expected/actual=`abc` matched=true → `verdict=payload_mismatch`，`exact=True`，suite `final_exact=1`。
- 修复方向：`exact` 应同时要求 `layer1`（或至少排除 failures 非空）；`not_exact` verdict 分支同理。

## F2（确认，中危）：时间戳部分退化时 ambiguous 不触发，位置配对被静默当作事实

- 位置：`scripts/analyze-layered-gui.py:431-436`。ambiguous 条件要求 `len(inputs)==len(checks)` 且两侧 distinct 都 ≤1。macOS oracle 秒精度 + Claude 毫秒时间戳的组合下：若 inputs 毫秒可分（distinct>1），即使全部 oracle check 落在同一秒内（顺序仅靠文件序、且 input 与 check 的真实对应未知），`ambiguous` 恒为 False；配对是全局第 i 个 input ↔ 第 i 个 check（`:427-443`），无 nonce/序号交叉校验。
- 反例（实际运行验证）：3 个 input（`xxx`,`aaa`,`bbb`，同一毫秒时间戳）、2 个 check（c1/c2，同秒）→ `ambiguous=False`，位置配对把 `xxx`↔c1、`aaa`↔c2，两条均产出 `verdict=payload_mismatch` 且 **`exact=True`**（叠加 F1），suite 层面会计为 2 个 final_exact。计数被错误对齐的配对洗白。
- 缓解因素：payload 文本互异时 layer1 多数情况会 fail-closed（另测 R5'：input `bbb`,`aaa` 与 c1/c2 期望互换，两条均 `payload_mismatch`、exact=False——语义错误方向不洗白）。但 exact 计数与 verdict 矛盾（F1）使其失去 fail-closed 保证。
- 附带：macOS 秒精度本身不引入误判——配对靠文件序而非 ts 排序；R7（oracle 无时区 naive 时间戳按 UTC 解析，`parse_ts:99`）只影响 ambiguous 启发式，不影响配对正确性，实测 fail-closed。
- 修复方向：任一侧 distinct≤1（或 counts 不等且存在同 ts 簇）即应降级为 unknown/ambiguous，而不是要求两侧同时退化且数量相等。

## F3（弱化项）：matched 字段缺失时 `not_exact` 无任何 failure 类别

- 位置：`:346-349` 的 oracle_inconsistent 检查只在 matched 为 True/False 且两侧为 str 时触发；`analyze_attempt` 的 failures 组装（`:341-355`）没有覆盖 `matched is None`。
- 反例（实际运行验证）：text_check 无 `matched` 键、其余全匹配 → `verdict=not_exact`、`exact=False`、`failures=[]`。`final_exact` 正确为 False（不洗白），但按 failures 过滤的下游消费者会漏掉该 case；`not_exact` 也未列入 case_failures。
- 修复方向：`matched is None` 时追加 `oracle_matched_missing` failure 类别。

## F4（弱化项）：无 model 字段的 assistant 事件被静默跳过，model 校验可通过

- 位置：`parse_transcript:198-199` 只收集 `isinstance(msg.get("model"), str)` 的 model；`build_report:421` 仅比较 `observed == [MODEL_EXPECTED]`。
- 反例（实际运行验证）：一条 assistant 带 `glm-5.3-flash`，另一条完全缺失 model 字段 → `model_check.ok=True`，无警告。非 glm 流量可混入而不被察觉。
- 修复方向：统计缺失 model 的 assistant 事件数并在 model_check 中报告。

## 未发现问题的重点方向（均有测试或反例覆盖）

- CRLF：known case 的 catalog task_payload 按 plan.rs 契约 CRLF→LF 归一后比对（`:324-326`），测试 `test_known_crlf_normalized` 通过；oracle 侧 CRLF 未归一导致的差异会被 layer3 捕获，方向正确。
- NFC/NFD、NBSP：无任何归一化，测试 `test_nfc_nfd_difference_not_normalized`、`test_nbsp_difference_not_normalized` 确认不洗白。
- oracle 伪 matched / hex 差异：`exact` 强制 `hex_status=="ok"` 且 `layer3`（`:357-362`），hex 与 text 不一致时 verdict=`utf16_hex_mismatch`（`utf16_status:296-314`），测试 `test_wrong_utf16_hex_futures`（原文 `test_wrong_utf16_hex_fails`）通过。hex 空白/逗号差异被 `canon_hex:70-73` 抹平属预期容错。
- unknown / 无 result：catalog 缺失 case 计 `missing` 不计 pass（`test_incomplete_case_missing_from_oracle`）；截断（有 payload 无 check）计 `no_text_check` + unattributed（`test_no_check_and_unattributed_payload`）；孤儿 check 计 `unpaired_checks`（`test_orphan_check_unknown_no_payload`）；无 tool_result 判 `missing_result` 永不 exact（`test_missing_tool_result_marked`）；tool_result 嵌套 tool_use/base64 不进入统计与日志（`test_nested_duplicate_tool_use_not_counted`）。
- first-pass/retry：retry 不提升 first（`test_retry_does_not_improve_first_pass`）；重复 check 计 unpaired 不双计（`test_duplicate_check_does_not_double_count`）。
- model 标签：非 glm 判 FAIL（`test_wrong_model_flagged`）——但见 F4 的旁路。

## 阻塞评估

F1 为阻塞：run 级 `first_attempt_exact`/`final_exact` 汇总可在模型发送错误 payload 时被计为通过，直接违背脚本自我声明的"retries never upgrade / 不洗白"目标。F2 在 F1 修复前为放大器、修复后为独立的中危对齐缺陷。F3/F4 建议一并修复但不阻塞。
