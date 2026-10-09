# GUI 视觉识别对照 — 2026-09-29

## 范围与授权

用户明确允许 Kimi 视觉失败后切换 GLM，并要求必要时由当前协调模型直接读图验证。仅做只读识别，不以手工操作代替 Agent 验收。不修改全局 Claude 配置，不改产品源码，不 commit/push。

## GLM 实测：PASS（1 次，不代表完整验收）

- 远端 acer-win；Claude CLI → computer-client → TLS Host；同前轮发布产物和任务文本，仅本次 `--model sonnet`。
- 运行前确认路由名称为 glm-5.3-flash，实际响应 `message.model=glm-5.3-flash`。该标签是响应自报，不是底层供应商身份证明。
- Host 与新随机夹具均在 Session 1，通过 Interactive Scheduled Task 启动。
- Run ID：`32caa70a-571d-422c-881d-752d6aceb255`。
- 持续 24.933 秒，exit=0，无 timeout，stdout drain 完成；仅 open→observe→close，无键鼠输入。
- 8 工具策略审计 17 项通过；close 返回 cleanup_outcome=released。
- 原始 MCP PNG：960×540、70073 bytes、SHA-256 `de52b416f53497b5cb8f19c5754459ca8776ad8b595e32e00026599910c7c12c`。
- 答案：Trial 1、NONCE FGQ8TL；从左至右黄色方形、绿色圆形、红色三角、蓝色菱形；目标为左起第二个。
- 与夹具 oracle 完全一致。

## 当前协调模型盲读对照：PASS

从 GLM 转录 image block 解码原始 PNG，无裁剪/缩放/重编码。先 view_image 直接读图并保存 `coordinator-blind-read.md`，之后才显示 GLM 答案并拉取 oracle，三者一致。

看图可辨随机码 FGQ8TL、四个形状及绿色圆形目标，估计中心 (414,245)。未执行点击，坐标估计不算点击成功。

右下角出现应用通知，但未遮挡挑战内容。不向报告抄录无关通知文字。受控窗口内容清楚，但背景不是完全无通知的理想环境。

这证明该 PNG 在当前图片查看路径可解码、内容可辨；不是通过 Claude CLI/原网关的等价端到端验证。

## 结论边界

至少本次 GLM 链路可正确识图，不能再认为“两个模型都不能识别”。昨日 Kimi 的错误不支持判定截图本身普遍不可读；仍需区分 Kimi 模型能力、该路由的图片转换以及暂态服务问题。

本轮仍没有 OS 文件沙箱证据；源码外 cwd、8 工具白名单和禁用 hooks 不能代替该隔离门禁。每平台每案例 10 次、至少 8 成功仍未满足。本轮不执行计算器、文本编辑或取消场景。

## 证据

- `.agents/runs/gui-glm-trial1-20260929/`：完整转录、原始截图、模型答案、盲读记录、oracle、运行摘要、策略审计。
- `.agents/runs/gui-glm-start-20260929.log`：Host/夹具启动身份。
- `.agents/runs/gui-glm-launch-20260929.log`：GLM 启动记录。

## 同图 Kimi 对照：FAIL

在 GLM 会话关闭后，以完全相同任务文本新开远端 Claude CLI `--model fable` 会话。没有传入 GLM 答案、协调者答案或 oracle。

- Run ID `71ccebb9-c3b3-46a4-92b0-49d56a8d6a40`，响应 `message.model=kimi-k3`。
- 49.058 秒，exit=0；仅 open→observe→close；8 工具策略审计 17 项通过。
- MCP 返回 PNG 同为 70073 bytes，SHA-256 与 GLM **完全相同**：`de52b416f53497b5cb8f19c5754459ca8776ad8b595e32e00026599910c7c12c`。这不是仅“相似截图”，是相同图片字节。
- Kimi 回答 NONCE `K7P9QX`，形状为绿色圆形、蓝色方形、橙色三角、紫色菱形；还声称存在实际没有的 OK 按钮及状态文字。与图不符，判为 FAIL。
- 图片前 input_tokens=937，图片后=43648，仅作诊断线索；GLM usage 为零，不能拿不同路由的 usage 当可信的统一计量。
- 本轮未观察到 429；切换依据是用户本次明确授权视觉失败可使用 GLM。
- 证据：`.agents/runs/gui-kimi-comparison-20260929/`。

### 对照结论

| 路径 | 同一 PNG 的识别结果 |
|---|---|
| 远端 Claude CLI → GLM 路由 | 正确 |
| 远端 Claude CLI → Kimi 路由 | 错误/编造 |
| 当前协调模型直接查看 PNG | 正确，且先于 oracle 保存答案 |

可以排除本次 PNG 文件损坏、内容本身模糊或两条 MCP 路径截图不一致。故障定位范围是 Kimi 这条模型调用链路：模型能力、模型专属网关适配或服务暂态仍未区分。没有抓取出站图片转换证据，不能宣称已证明 Kimi 本体不支持视觉或已证明网关有 bug。

建议后续 GUI 验收先使用本次看图通过的 GLM，仍逐案例检查截图和真实输入结果；Kimi 路由另做图片转换诊断。单次看图成功不等于完整 computer-use 验收通过。

## 清理核验

按记录核验身份后停止/注销本轮 Fixture、背景窗口和 TLS Host；保留日志与产物，不关闭用户应用。2026-09-29 09:42:16 +08:00 独立复查 exit=0：自有发布目录进程、三个本轮任务、记录中的 CLI PID、8399 监听均为空。证据 `.agents/runs/gui-comparison-cleanup-20260929.log` 与 `.agents/runs/gui-comparison-cleanup-verified-20260929.log`。

另以 cmp 逐字节比较 GLM/Kimi 的原始 PNG，exit=0，确认不仅哈希相同。
