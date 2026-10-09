# GLM 真实 Windows GUI 单轮测试 — 2026-09-29

## 总结

已将两端 Claude 用户级全局模型改为 GLM（见 `glm-global-switch-20260929.md`），本轮所有 Claude 实现/修复 worker 与远端 GUI Agent 均使用 GLM。真实动作结果：靶点点击 PASS 1/1；夹具 Unicode 文本校验 PASS 1/1；系统计算器 PASS 1/1；记事本 FAIL 0/1（字符异常并超时）；运行中取消完成底层诊断，但严格诊断断言未全部通过，不能计作 GLM Agent 取消验收成功。

**没有完成每平台每案例 10 次、至少 8 成功门禁；没有 OS 文件隔离证据；没有本轮 macOS GUI 验收。**

## 路由、模型与安全边界

- 远端 Claude CLI → 远端 computer-client → TLS `100.200.20.168:8399` → Windows Host；地址本轮重新读取，不是硬编码产品地址。
- 远端 GUI Agent 从源码外目录启动，8 个 computer MCP 工具，内建工具关闭、strict MCP、禁 hooks/slash commands。
- 本轮任务响应模型为 `glm-5.3-flash`。本机实现 worker 亦为 GLM。原始日志中的历史 Kimi 记录未改写。
- Host PID26304，Session1，与 explorer 相同；任务 `RpaComputerRemoteHost-52bd4c15b53449bd919c0b5bb50c8f77`。
- 测试 fixture PID12736，Session1；run `5b3f6173ae81462c9d3993487aa68614`。
- 无 SSH 隧道，SSH 只负责部署、CLI 启动和取日志。远端 CLI/Client 在 Session0 不承担原生 GUI 执行，桌面 Host 在 Session1。
- 夹具任务完成后停止 fixture，随后系统应用测试中用户既有窗口重新可见；Agent 未操作其内容，但**本轮后半段并非完全无私人背景的受控桌面**。截图仅留本地测试证据，报告不转录无关窗口内容。

## 1. 靶点点击 + 夹具 Unicode：PASS

- Run `0a679124-de5d-4ac3-9048-fc4b81fad14c`，59.010 秒，7 调用、8 turns，CLI exit0，策略审计17项通过。
- 随机 NONCE W8LRAG，蓝色菱形/绿色圆形/黄色方形/红色三角；Agent 由截图选中 `(414,243)`。
- Oracle `hit`：slot=1,target_slot=1；截图有 `TARGET HIT`。
- Agent 从屏幕读取 `computer-use 你好 10×20`，使用 text_input 输入，点击 Check text。
- Oracle `text_check`：matched=true, expected_len=21, got_len=21；截图 `MATCHED — 21 characters`。
- close cleanup_outcome=released。
- 证据 `.agents/runs/glm-target-text-20260929/`，最终 `screenshot-5.png`。

## 2. 系统计算器：PASS

- Run `2382829f-617a-46a6-a313-c9541aac4d2d`，145.679 秒，16调用、17turns，CLI exit0，策略审计17项通过。
- 通过任务栏搜索输入“计算器”并点击可见应用结果，打开新的计算器，初始显示0。
- 按钮点击链 `1 → 0 → × → 2 → 0 → =`，每次 based_on 使用上一观察。
- 最终观察 `session-16824-1-obs-13` 显示 `10 × 20 =` 和 `200`，协调者已查看原始截图，不是仅采信答案。
- 关闭仅本次打开的计算器，close cleanup_outcome=released。
- 证据 `.agents/runs/glm-calculator-20260929/`，`screenshot-13.png` 为求值结果，14为关闭后。

## 3. 记事本中文/emoji 编辑：FAIL

- Run `89f118e0-27d9-42b2-9231-65b2f160a7a3`，MaxTurns30、MaxSeconds360；达到360秒超时，runner按身份终止自有CLI PID8608。
- 没有 final result、没有 Agent computer_close；策略审计明确失败，不能计成功。
- Agent GUI 搜索 notepad，打开新的空白现代记事本文档。启动前没有记录到已有 Notepad 进程。
- 第一行输入 `你好，世界 🌍` 可辨。
- 单独 Enter 使用 `key_chord` modifiers=[] 被 runtime 拒绝，invalid_action：至少需要一个 modifier；没有注入。Agent 改用 text_input 的换行继续。这是动作使用/接口限制，不能当成字符乱码的已证实根因。
- 后续实际请求 `\nUnicode 测试 ✅`，以及单次修正 `Unicode 测试 ✅`，截图出现多个绿色勾号。
- 第三行实际请求 `\n本文档共 3 行（含本行）`，截图出现重复括号。**协调者确认截图本身确实异常，并非仅模型错误描述。**
- 文本工具返回 dispatched 只证明提交了输入，不证明目标应用显示正确；不能以字符总数符合预期判为内容正确。
- 尚未定位是后端 Unicode 注入、目标记事本处理/渲染、时序或其他原因；未修改产品后端掩盖失败。
- 证据 `.agents/runs/glm-notepad-20260929/`，14/15为异常后画面。

### 文档清理（不算失败重测）

独立 GLM 清理任务 `b37a03fa-ab52-49ba-a0fb-ba3303971a03`，只根据本轮新建文档的可见内容识别测试 tab。点击该 tab 关闭按钮，再点击“不保存”，截图确认记事本消失，其他应用保持。67.648秒，策略审计17项通过，close released。

证据 `.agents/runs/glm-notepad-cleanup-20260929/`；任务明确禁止修改/丢弃其他文档。原失败记录保留，清理成功不转换为记事本验收成功。

## 4. 运行中取消：底层部分证据，完整门禁未通过

本项为协调者驱动本机 Client→远端原生 Host 的 MCP 诊断，由 GLM 编写并经协调者审查测试 helper。**不是远端 Claude Agent 驱动的完整取消验收**。

- 发送 key_hold shift 5000ms，0.3秒后向同一个 JSON-RPC requestId 发 notifications/cancelled。
- 取消响应耗时约 **36ms**（通知发送至收到工具响应，不是物理按键释放测量）。
- 实际返回 cancelled=true,error.code=cancelled,input_outcome=partial,cleanup_outcome=released,events_completed=[0],events_total=2。
- 后续 move 被拒绝，input_outcome=not_started、events_completed=[]，但错误是 **stale_observation**，不是本诊断所要求的 cancelled。
- 因此严格诊断 exit1。不能用这个拒绝结果独立证明“取消锁存阻止新有效输入”：旧 observation 在按键动作之后本来就会失效，构成测试混杂条件。
- source 审查解释了为何旧 basis 先命中陈旧观察校验；未据此宣称产品取消失效，也未放宽断言制造通过。
- finally computer_close 返回 state=closed,cleanup_outcome=released。由于早期断言失败，本次未完成 get_step 和 observe 后续步骤；物理 OS 按键状态/取消后新有效观察路径仍未独立验证。
- 证据 `.agents/runs/glm-cancel-native-20260929/`；`20_cancel_response.json`、`30_move_after_cancel.json`、`99_best_effort_close.json`。

## 脚本改动与独立检查

- `scripts/run-blackbox-claude.sh`：实际 argv 显式 `--model sonnet`，更新 print/interactive 验证。原防护保留。
- `.agents/runs/gui-remote-agent-glm-20260929.ps1`：新 helper 默认且只接受 sonnet；原历史脚本不改写。
- `.agents/README.md`：记录用户全局迁移授权覆盖旧约束。
- bash -n、runner --verify-only 通过；`scripts/test-release-scripts.sh` **81 passed, 0 failed**。只证明脚本/协议逻辑，不替代GUI。
- 取消 helper 初稿存在泛化错误码/cleanup断言问题，审查后由GLM收紧检查才执行；旧 writer max_turns 不作为代码成功证明。
- `git diff --check` 通过。本轮没有改产品Rust源码，没有 commit/push。

## 下一步

1. 先做现代记事本 Unicode 最小复现，对比夹具与目标编辑器、单字符/连续文本、换行和非BMP字符，定位真实边界，再由GLM做最小修复和回归。
2. 修正取消诊断的旧 observation 混杂，保留当前失败，补齐真正 Agent 运行中取消及 OS 层释放证据。
3. 上述问题解决后再进入多次随机挑战/跨平台验收；不把本轮1次通过当作10次门禁达标。

## 清理独立核验

2026-09-29 10:33:55 +08:00，远端检查 exit0：自有部署进程、本轮3个任务、Claude CLI、Calculator/Notepad进程、8399监听均为空。清理只针对本轮身份记录，用户其他应用未关闭。证据 `.agents/runs/glm-actions-cleanup-20260929.log`、`.agents/runs/glm-actions-cleanup-verified-20260929.log`。
