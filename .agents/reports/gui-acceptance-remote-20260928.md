# 远端 Claude CLI + Kimi GUI 验收 — 2026-09-28

## 结论

**视觉前置门禁未通过；输入类验收 BLOCKED，项目验收未完成。**

真实执行了两次独立的远端 Claude CLI 只读会话，针对同一个受控随机挑战画面复核，识别成功 **0/2**。这不是两次不同随机布局，也不满足每平台每案例 10 次、至少 8 次成功的原验收门禁。工具策略审计通过不能替代 GUI 语义正确。

## 实际执行环境与边界

- Windows：SSH 主机 `acer-win`，本轮检测到地址 `100.200.20.168`；地址不应作为持久配置常量。
- 远端 Claude CLI native exe，版本 2.1.266；调用参数 `--model fable`。两次响应 `message.model` 均记录为 `kimi-k3`（服务自报模型标签，不是底层供应商身份证明）。
- 本轮路由：远端 Claude CLI → 远端 computer-client → TLS `100.200.20.168:8399` → Windows Host。没有 SSH 隧道。
- SSH 用于部署、启动远端 CLI、读取日志；Windows 桌面 Host 和原生夹具经 Interactive Scheduled Task 运行在 explorer 所在 Session 1。
- 上阶段 Mac Client→远端 Host 的直连证据见 `direct-remote-live-20260928.md`；本轮不是该拓扑的新验收。
- Agent 从源码外发布目录启动，仅暴露 8 个 computer MCP 工具，strict MCP，关闭内建工具、hooks、slash commands。未将 oracle 或源码传给 Agent。
- **本轮没有 OS 文件隔离证据**；这些逻辑限制不能等价于系统级沙箱，原隔离门禁仍未满足。
- 没有修改全局 Claude 模型配置；未修改产品源码、未 commit/push。

## 视觉真值与结果

通过转录中工具返回的 image block 解码得到实际 PNG，并人工查看；不是另拍一张截图替代转录证据。该证据证明图片已出现在 Claude CLI 的 MCP 工具结果中，不能证明后续网关将其正确转换为模型视觉输入。

两轮 PNG 完全相同：960×540，58244 bytes，SHA-256：
`471a1d5a572abe843a7e8ad161b43d5fab8057e706d42187c5185de7d3e99a8d`

受控桌面画面清楚，无终端遮挡：
- Trial 1；NONCE `LCVGPQ`。
- 从左至右：黄色三角、红色菱形、蓝色方形、绿色圆形。
- 原生 Computer Use Acceptance 窗口，中性浅灰背景。

| 运行 | 远端 run id | 时长 | 工具调用 | 语义结果 |
|---|---|---:|---|---|
| 第一次 | `05c39fd4-1023-4d18-9c6b-88073688b3a0` | 35.923 秒 | open → observe → close | FAIL：回答 Trial 0026、JD68R4，且形状、背景均与图不符 |
| 只读复核 | `0f201fd8-c796-4e6b-a4f2-aa8a96e1d798` | 40.895 秒 | open → observe → close | FAIL：明确回答 UNREADABLE，没有编造答案 |

两轮 CLI exit=0，均无 timeout，stdout drain 完成；两轮工具策略审计各 17 项通过、8 个可用工具、3 次调用、4 turns。两轮 close 都返回 `cleanup_outcome=released`，均无 computer_step/键鼠输入。

两轮在截图前 `message.usage.input_tokens=937`，截图后 `36704`。这是诊断线索，**不足以证明 base64 被当成文本、网关丢图或模型缺少多模态能力**；尚未检查实际出站请求转换和服务端处理。

## 429 与模型切换

用户授权 Kimi 遇到 429 时切换 GLM。两轮保存的转录和 stderr 未观察到 429，result.api_error_status=null；未触发 fallback，未执行 GLM。本报告不声称掌握网关内部未暴露的重试情况。

## 案例矩阵

| 案例 | 本轮状态 |
|---|---|
| 受控截图识别 | FAIL，0/2，同图两次独立模型会话 |
| 随机靶点点击 | BLOCKED，未执行 |
| 计算器 GUI 10×20 | BLOCKED，未执行；不以心算替代 |
| 文本编辑 | BLOCKED，未执行 |
| 取消/中断专用案例 | BLOCKED，未执行；正常 computer_close 不算取消案例通过 |
| 每平台每案例 10 次且 ≥8 成功 | 未满足 |
| OS 文件隔离 | 未满足 |

## 证据

本机路径（相对项目根）：
- `.agents/runs/gui-kimi-trial1/`：transcript.jsonl、stderr.log、run-summary.json、model-screenshot.png、policy-audit.txt。
- `.agents/runs/gui-kimi-trial2/`：transcript.jsonl、stderr.log、run-summary.json、model-screenshot-1.png、policy-audit.txt。
- `.agents/runs/gui-windows-oracle-20260928.log`：协调者专属 oracle。
- `.agents/runs/gui-remote-kimi-trial2-launch-20260928.log`：复核启动记录。
- `.agents/runs/gui-remote-cleanup-20260928.log`：清理动作。
- `.agents/runs/gui-remote-cleanup-verified-20260928.log`：独立清理后检查。

## 清理

按专属记录核验身份后停止并注销：
- Fixture `AccFixture-871b9fbebedd45a88ab1ae32b0fd1ae6`，原 PID 18652。
- 中性背景 `ComputerGuiBackdrop-20260928`，核验唯一 action、参数、principal 和 Interactive 登录类型。
- TLS Host `RpaComputerRemoteHost-860993c5c7fd4af4b686ddf56dcd1e5b`，原 PID 11844。

保留发布产物、日志、截图、oracle 和凭据文件。没有关闭用户应用，没有按进程名批量 kill，没有改防火墙。

## 后续建议（未执行）

先定位 CLI→本地模型网关→模型的图片转换链路，再恢复 GUI 输入案例。可在用户授权视觉失败也使用 GLM 对照后，以同类受控只读挑战做对照；不能把当前视觉失败记为 429 来触发已授权的条件切换。不要在识别未通过时扩大盲点测试。

清理后独立核验：2026-09-28 17:57:32 +08:00，远端检查 exit=0；自有部署目录进程、三个专属计划任务、记录中的 CLI PID、8399 监听均为空。此检查确认测试进程已退出，不是取消场景功能验收。
