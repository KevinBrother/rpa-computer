# Focus phase boundary — production-called fixture RED freeze

2026-10-16。用户批准有限RED authoring；**仍保留错误归因行为，不是修复/GUI资格。**
未运行测试、进程、SSH、GUI、Xvfb、Docker或依赖安装；仅对4个变更Python源码做内存syntax compile。
不改产品、observer、rules、环境、diagnostic sink；capture exclusion unsupported，Host关闭。

## 原始现场事实：不改写FAIL、不说产品抢焦点

原始证据：`.agents/runs/linux-parity-20261015/cc-ui-atom-green-live-1/remote-logs/evidence/run1/`。
setup move事件批t81769.154173649含target2097152的focus_out→focus_in，current为target；
早于renderer spawn_before81769.15451849。新binary在81769.233480681真实发Ready，
pointer_feedback=true/exclusion=unsupported；第一次inspect只有renderer create/map而无新focus_out，
status/Stop viewable、no_focus_hint/override_redirect，current仍target。
原fixture把所有历史交给focus guard，从而FAIL 'transient focus loss observed'；9 NOTRUN、无PPM。
这不是产品导致该pre-spawn事件的证据，不是完整UI通过；历史旧binary的BadAtom归因仍UNKNOWN。

## 接线与明确保留的缺陷

仅2个已有fixture文件变更：
- `case.py`：初始化focus_start；真实start在setup move之后、Renderer构造之前调用_arm_focus；
  inspect/image/ring前图/ring最终fence四处全部接入同一个_check_focus。
- `review_red/support.py`：ring_case绕过构造/start，只添加显式focus_start=0，表示其脚本本来
  已从运行期开始；原13断言完全不变，不为正常生产Case添加fallback。

RED seam的_arm_focus **仍设0**、仍不检查setup focus；_check_focus传history[0:]，
等价于原全历史错误归因。未建立正确cursor、未记录focus_guard_armed、未实现bad-setup禁止spawn。
没有无条件Err stub；所有旧正常/拒绝行为仍走原rules.check_focus。没有增加X请求、clear历史或重置预算。

新增 `focus_support.py` 只提供IO观察、constructor/ready时点注入和内存Evidence；
`NoProcesses.spawn`明确禁止启动进程。测试实例化真正Case，调用真正start/inspect/image/
ring_clickthrough及其shared guard、clickthrough、ring_continuity；不复制focus判断算法。

## 精确新13cases：源码预期4 FAIL +9 PASS，尚未执行

`test_focus_boundary.py`：

| 测试（test_前缀省略） | 预期 | 约束 |
|---|---|---|
| setup_history_does_not_fail_start_and_epoch_precedes_spawn | FAIL | 真实start通过focus门到首截图；cursor及phase证据先于Renderer；完整setup历史保留 |
| bad_setup_focus_prevents_renderer_construction | FAIL | setup焦点错误必须抛Failure且spawn0次 |
| transient_loss_inside_renderer_construction_is_rejected | PASS | spawn内瞬时丢焦并恢复仍拒绝 |
| transient_loss_during_ready_wait_is_rejected | PASS | Ready等待期间失焦不能被延后cursor掩盖 |
| runtime_rechecks_and_image_do_not_reset_focus_interval | PASS | inspect/image/reinspect持续拒绝同一失焦，cursor与历史不重置 |
| wrong_current_focus_rejects_even_without_focus_out | PASS | 无事件但current错误也拒绝 |
| ring_pre_image_checks_runtime_transient_loss | PASS | ring前图注入被真实guard拒绝，click尚未发生 |
| ring_final_fence_checks_runtime_transient_loss | PASS | click和后图之后、最终fence注入仍拒绝 |
| ring_guards_share_pre_spawn_epoch_not_entire_setup_history | FAIL | setup prefix不误伤真实ring路径，完整prefix保留 |
| ready_false_establishes_same_pre_spawn_boundary | FAIL | 不等Ready也必须spawn前建正确边界；后续inspect不误伤prefix |
| ready_false_still_rejects_spawn_time_transient_loss | PASS | ready=False不能免除运行期guard |
| unrelated_and_mapping_history_remain_append_only | PASS | unrelated失焦、press/map/destroy审计历史保留且可查询 |
| clean_ring_path_reaches_existing_continuity_check | PASS | 无失焦正控走真实前后图、clickthrough、最终fence/continuity |

前三个需接受的历史prefix正控把Failure显式转换为unittest assertion failure；badsetup使用assertRaises，
故预计是**4 assertion FAIL而不是ERROR**。没有expectedFailure/skip或人为抛出的RED缺实现stub。
正向clean startup/ring controls防止reject-all伪GREEN；真实运行期FocusOut/FocusIn恢复不被豁免。
不把源码预测写成已观测RED，必须等独立CC实际13非零case及失败位置。

## 原freeze/断言保持

以 `renderer-live-ui-review-green-1` 的41个live-ui路径为基线：39不变，仅上述2文件接线/初始化改变。
旧45断言所在8个Python/Rust文件逐字节不变；Rust observer整个源码及manifest/lock不变。
atom-bootstrap-green-1原76完整清单逐项不变（包括原4/117产品断言），不转移历史PASS为新snapshot结果。
具体hash审计见integrity.log。原现场rawFAIL、旧freeze、archive/报告不改写。

## 冻结与编译证据

本目录 `focus-red.sha256`：完整fixture46文件（原41 + 新support/test/本handoff/syntax/audit5）。
`fixture-focus-red-1.tar.gz`含这46文件；清单单独交付，不含自己/其他archive/cache。
`overlay.sha256`及`fixture-focus-red-1-overlay.tar.gz`仅7文件：2修改＋上述5新增。
可叠加原fixture41；产品直接使用未变的GREEN76。新的总集成文件数由协调者按其wrapper去重核算。
原review_green/fixture-green.sha256只证明旧snapshot，不作为当前case/support的验收清单。

syntax-check.log：4文件compile(source,...,'exec')，无import、无测试执行、无pycache；AST计数13。
没有Rust编译（observer/产品未变），没有运行任何fixture。

## 独立CC精确命令（仅交付，不执行）

使用现有approved snapshot与依赖，unset DISPLAY/XAUTHORITY/WAYLAND_DISPLAY；分别记录退出码/case数。

```sh
L="$PWD/desktop-feedback/linux/testing/live-ui"
sha256sum -c "$L/focus_red/focus-red.sha256"
# 新13，预期4断言FAIL/9PASS，0ERROR、rc1；必须拒绝零case结果。
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L:$L/focus_red" \
  python3 -m unittest discover -s "$L/focus_red" -p 'test_focus_boundary.py' -v
# 旧Python27与13（注意与本轮新13分开统计）
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L" \
  python3 -m unittest discover -s "$L/tests" -v
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L:$L/review_red" \
  python3 -m unittest discover -s "$L/review_red" -p 'test_*_red.py' -v
# 旧Rust5；沿用现有locked vendor配置，不装依赖。
cargo-1.89 test --locked --offline --manifest-path "$L/observer/Cargo.toml" \
  --bin rpa-feedback-live-observer
```

这些不是GUI授权。实际RED后再由协调者决定最小GREEN：验证setup current，保存一次性len(history)，
记录phase边界；四处guard共用且不刷新。不能为通过新断言删历史、放宽rules或改旧45断言。
