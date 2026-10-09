# Layered GUI Evidence Analyzer — 2026-09-30

## 交付物（本次独占写入）

- `scripts/analyze-layered-gui.py` — 纯离线诊断分析器（无 GUI 输入、无部署、无网络）
- `tests/layered_gui_analyzer.py` — 17 个合成 fixture 测试（TDD）
- 本报告

未改动 fixture/src/其他任何文件；未 commit/push；未改全局设置。

## CLI

```
python3 scripts/analyze-layered-gui.py --oracle PATH --transcript PATH --output PATH [--catalog PATH]
```

- `--oracle`：协调者独占的 oracle JSONL（`utf-8-sig` 读取，兼容 BOM；CRLF 行尾无影响）。
- `--transcript`：Claude CLI stream-json。
- `--catalog`：可选，全 suite case 列表（`{"cases":[{suite, case_id, task_payload, expected_text}]}`，
  也兼容 `{"suites":[{suite, cases:[...]}]}` 与 `{"suites":{name:[...]}}`）。
- 输出：JSON 报告（`--output`）+ stdout 摘要。

## 解析契约（与真实代码核对）

- CRLF 契约：读 `src/runtime/plan.rs:43` `normalize_newlines`（只读确认）：`\r\n`→`\n` 一次，
  单独 `\r`、单独 `\n` 保留。分析器按同一规则构造 layer-1 预期（known 用 catalog `task_payload`
  normalize 后比较）。
- 时间戳：Windows `DateTime.UtcNow.ToString("o")` 7 位小数、macOS `ISO8601DateFormatter` 无小数、
  纯数字按 epoch 秒。统一解析为 epoch；小数截断到微秒（保守，利于歧义检测）。
- UTF-16 hex：fixture 用 `ToCharArray()` 每码元 `"X4"` 空格连接；分析器用 `utf-16-le` 编码重算并
  与 `actual_utf16_hex`/`expected_utf16_hex` 规范化比较（去空白、忽略大小写）。
- transcript：只统计**顶层** `assistant.message.content[].tool_use`；`tool_result` 的 content 完全
  不读取、不遍历、不打印（可能含嵌套重复 tool_use 与 image base64）。
- 模型核验：全部 `assistant.message.model` 必须为 `glm-5.3-flash`，否则 `model_check.ok=false`。

## 三层比较（每 attempt 可追溯）

| 层 | 比较 | 含义 |
|---|---|---|
| layer1 | payload vs expected（known=catalog task_payload 规范化 CRLF；unknown=oracle expected_text） | 识别/任务参数先有偏差 |
| layer2 | oracle actual_text vs normalize_newlines(payload) | 输入/应用差异 |
| layer3 | oracle expected_text == actual_text（且 matched==true） | 最终 exact |

`exact = layer3 && matched && utf16 校验通过 && tool_result 存在`。verdict 优先报告最先失效的层
（utf16 失效 > payload_mismatch > input_divergence > oracle_mismatch > exact），即使 oracle 层
exact 也会以 failure 记录 layer1/layer2 偏差。

## 对齐规则

- 全局顺序配对：第 i 个 `text_input` ↔ 第 i 个 `text_check`（按文件/时间顺序），配对继承 check 的
  (suite, case_id)。
- 未配对的 input → `unattributed_payloads`（无法判定，不算通过）；未配对的 check →
  `unpaired_checks`（unknown，不强行判注入失败）。
- 歧义守卫：inputs==checks≥2 且两侧时间戳各自全部相同（或不可解析）→ `ambiguous_timestamp`，
  该 case exact 输出 `null`（unknown），不盲选。
- 无 check / 截断 / 缺 tool_result / 空 result 一律不算通过。
- 同 case 重试保留全部 attempt：`first_attempt_exact` 只看第一个配对 attempt，`final_exact` 看最后
  一个；重试不会洗成 first-pass。重复 check 不会制造新 attempt。

## TDD 结果

```
python3 tests/layered_gui_analyzer.py
Ran 17 tests ... OK
```

覆盖场景（全部合成数据，无 GUI）：

1. 精确成功（含 BOM oracle、Windows 7 位小数时间戳）
2. 视觉参数错误（text exact 但 wrong 点击，hit_count=0，不洗白）
3. 应用与 payload 不一致（layer1 真 / layer2 假 → input_divergence）
4. known CRLF 正确（task_payload `\r\n` 规范化后 exact）
5. NBSP 差异（U+00A0 vs 空格，不归一化，判失败且原文保留在报告中）
6. NFC/NFD 差异（é 组合字符，不归一化）
7. 未完成 case（catalog 有、oracle 无 → missing，不计通过）
8. 重试不提升 first-pass（attempts=2，first=False，final=True）
9. 重复 check 不重复计数（attempts=1，unpaired_checks=1）
10. 错误 UTF-16 hex（校验不一致 → utf16_hex_mismatch，绝不 exact）
11. 孤儿 check 无 payload → unknown，不强行判注入
12. 同时间戳多条 payload/check → unknown（exact=null），不盲选对齐
13. wrong model（claude-sonnet-5 → model_check 失败）
14. 缺 tool_result → missing_result 标记，不算通过
15. tool_result 内嵌重复 tool_use / image base64 不计数、不进日志
16. 无 catalog 的 unknown case 回退 oracle expected_text，偏差标注 unknown_case
17. payload 无后续 check（截断）→ 不算通过

## 已知边界（诚实声明）

- 输出 disclaimer（JSON `disclaimer` 字段）：这是**文本/oracle 诊断**，不替代严格工具策略审计，
  不证明截图看起来正确，同 case 重试不洗成 first-pass。
- hex 校验缺失字段视为 `utf16_hex_missing`（unknown，不算通过）——严格按"必须可追溯"。
- 配对是全局顺序假设（transcript 无 case_id，只能靠顺序）；歧义守卫覆盖计数相等但时间戳不可分
  辨的情况；计数不等时不足/多余部分显式输出 unknown，不猜。
- suite 汇总中 `first_attempt_exact`/`final_exact` 为"该层结果为 True 的 case 数"；unknown（null）
  不计入通过侧。

## 验证命令与结果

```
$ python3 tests/layered_gui_analyzer.py
Ran 17 tests in 0.018s
OK
```

## 第二轮：修复独立审查反例（同日，F1–F4 + 协调者补充）

依据 `.agents/reviews/layered-evidence-20260930.md`（reviewer 持有，未改动）：

- **F1 exact 全门控**：`exact = layer1 && layer2 && layer3 && matched && hex ok && result ok`。
  verdict 失败（如 payload_mismatch）的 attempt 不可能产生 pass 计数；审查反例 R1/R2 以
  `test_payload_mismatch_never_counts_as_pass` 回归。
- **F2 放弃全局第 N↔N 盲配**：payload 经 oracle trial 区间（payload ts ∈ [trial ts, 下一 trial ts)）
  归属 case；check 仅在**同 case 内恰有一个未消费 payload 能因果先于它**（input ts ≤ check ts，
  且 tool_result 完成 ts ≤ check ts 若已知）时唯一证明配对。同秒多候选、缺 oracle ts、payload 无 ts、
  trial 同 ts 平 tie、倒序时间戳均降级 unknown（`unresolved_checks[].reason`：
  `missing_oracle_ts` / `no_candidate_payload` / `ambiguous_candidates`），并独立保留每条 check 的
  期望/actual 正确性字段（`oracle_check_validity`）与配对状态两套字段，不臆断注入正常。
  输入已发送缺 check 的 payload 留在本 case（`payload_without_check`），不会错配下一 case；
  重复 check 未消费任何 payload，不能偷用下一 payload 或升级 first-pass。
- **F3**：`matched` 缺失 → failure `oracle_matched_missing`，verdict 同名，exact=False。
- **F4**：assistant 事件缺 `model` 字段计入 `model_check.missing_model_assistant_events`，
  任一缺失即 `ok=False` 并输出 warning。
- **known CRLF 语义修正（协调者发现）**：known layer1 改为与**原始** `task_payload` 精确 raw 比较
  （`raw_payload_vs_task_payload` 独立字段）；layer2 用 `normalize_newlines(payload)` 比对 actual；
  `normalized_payload_vs_task_payload` 作为独立信息字段（双边归一）。发送原始 CRLF、应用显示 LF
  → exact；agent 改发 LF → `raw_task_payload_deviation` failure、exact=False（不算严格 raw 通过）。
  无全局 Unicode 归一化（NFC/NFD、NBSP 测试保持不变）。

测试从 17 例更新/扩展到 26 例（覆盖未减）：新增 漏 case1 check 而 case2 成功、duplicate check
在新 payload 到达前不得偷用、倒序 ts/单侧缺 ts、同秒双候选、tool_result 完成时间建立唯一因果配对、
跨 case 重复文字不错配（正/反两向）、F1 反例、F4 缺 model；CRLF 两条替换原 LF→LF 伪案例。

```
$ python3 tests/layered_gui_analyzer.py
Ran 26 tests in 0.028s
OK
```

单文件行数：分析器 751 行、测试 778 行（均 <1000）。仍未 commit/push、未改 fixture/src、未操作 GUI。

## 第三轮：live 轨迹统计边界（同日，最小补充，未动对齐算法）

真实 live 轨迹 `.agents/runs/layered-win-baseline-live-analysis-20260930.json`（3 check / 4 trial
却 `observed_case_count=10`）暴露两个边界：

1. **observed_case_count 只算 oracle 实际出现**：改为按 trial/hit/wrong/text_check 事件的
   distinct (suite, case_id) 计数；`case_count` 仍为 catalog 期望总数。case 列表现也包含仅有
   trial 无 check 的 oracle-observed case。回归 `test_partial_run_observed_vs_expected`：
   4 trial / 3 check / catalog 10 → observed=4、expected=10、attempts=3。
2. **first/final 按 check 文件顺序合并 proven+unresolved**（新增 case 级 `check_sequence`）：
   最早 check 未证明 → `first_attempt_exact=None`；最后 check 未证明 → `final_exact=None`
   （不得继承此前 proven true）。后续 retry 成功只提升 final，不洗白 first；
   完整 attempts/unresolved 字段保留。回归三条：
   `test_first_unknown_then_success_not_upgraded`（首 unknown → first=None, final=True）、
   `test_first_failure_then_unknown_final_stays_unknown`（first=False, final=None）、
   `test_success_then_unknown_final_stays_unknown`（first=True, final=None）。
   原 duplicate-check / orphan-check 用例期望同步更新为"末位 unknown 不继承"。

```
$ python3 tests/layered_gui_analyzer.py
Ran 30 tests in 0.038s
OK
```

行数：分析器 771、测试 878（<1000）。仍未 commit/push、未操作 GUI、未改 fixture/src。
