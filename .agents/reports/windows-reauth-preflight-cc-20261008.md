# Windows reauthorized read-only preflight — CC+GLM（2026-10-08）

角色：CC+GLM 测试编排者（外层仅编排；全部截图判读/输入决策由远端 Windows Claude CLI 实际模型完成）。
无 subagent/Task；无源码/产品/测试/helper 修改；无 commit/push/worktree/reset；无全局模型/凭证/防火墙改动；
无解锁/唤醒；未重启 RPAD/Executor；未触碰任何非自有应用/任务。仅一次尝试，未重试。

## 0. 授权与范围

用户明确表示网络授权阻塞已人工处置（“认证通过了”）。本任务仅为 **一轮新的只读 preflight**
（describe/open/observe/close，零输入），不授予输入套件，不做防火墙改动，不作为当前产品验收。

## 1. 路由修正（桌面启动前，非 GUI 结果重试）

- 首次本地外层请求使用字面 `--model glm-5.3-flash`，被既有 localhost 代理实际路由到 **kimi-k3**
  （transcript 中 12 条 message.model=kimi-k3）；在仅本地读取、未做任何 SSH/桌面操作后停止（exit 143）。
  原始 transcript 保留于 `.agents/runs/windows-reauth-preflight-cc-20261008/`。
- 随后 no-tool 单轮 `--model sonnet` 探针确认实际 assistant 模型为 **glm-5.3-flash**
  （model-probe.jsonl：glm-5.3-flash，"READY"；transcript 20 条均 glm-5.3-flash）。
- 结论：外层沿用既有 sonnet 别名，未修改任何 settings。新外层 transcript 路径
  `windows-reauth-preflight-sonnet-cc-20261008`；内层截图/证据/报告仍使用本任务原定路径，保持命名稳定。
- 远端内层模型系由其 transcript 的实际 message.model 单独核验（见 §4），不以上述本地探针推断 Windows 路由。

## 2. 复用与哈希核验（未重建/未替换任何二进制或凭证）

- 复用已部署基线 **`C:\computer-cc-preflight-20260930-191500`**，仅作连接/可见性检查，非当前产品验收。
  远端实测哈希与 09-30 报告期望值一致：
  - bin\computer-host.exe `9EFF4BE78AAA36EA341871640EDF5806C53BF1072C1946EEF57A8FBD419DE769` ✓
  - bin\computer-client.exe `2EB020A404C1FFC6673042839B5C2D5F12F17AB05ACD26B437A2C3435FC8DC84` ✓
  - fixture `C:\Temp\computer-gesture-cc-20260930-185517\build\ComputerUseAcceptance.exe`
    `3A5FA228477E176E42D05A614F20AD8E8187A7E6CD6217B84EE9411514F526FF` ✓（先核对报告原文件名/哈希后使用）
- helper 字节对比（本地 vs 远端 staged）：
  - 一致 ×7：windows-remote-start/stop.ps1、stop-windows.ps1、gui-runner-launch-20260930.ps1、
    gui-neutral-desktop-20260930.ps1、gui-runner-stop-20260929.ps1、gui-remote-agent-glm-utf8-20260929.ps1。
  - **漂移 ×1**：`acceptance-fixture/start-windows.ps1` 本地工作树（08e7bafd…）≠ staged（9c44f70e…）。
    判定：staged 副本 LastWriteTime 仍为 2026/9/30 19:12:29（两轮 preflight 均用其成功启动），
    即未变的已验证基线；本地漂移是部署之后工作树的未提交改动（diff 仅为 ValidateSet 追加
    pointer/keyboard/focus 套件，multiclick 支持不变，该改动归实现方所有）。**处理：原样使用 staged helper，
    未用本地覆盖远端，未重建。** 此漂移已如实记录，不构成阻塞。
- 凭证仅读取路径存在性（`C:\computer-native-20260929-build\{ca.pem,client.token}` 均在），内容未读取、未改动；
  mcp.json 未变（端点未变，无需新生成；仅含路径无内联密钥）。直连 TLS，无隧道。

## 3. 启动前状态核验（DHCP 现查）

- 主机 node1，2026-10-08T11:01+08:00；DHCP IPv4 **100.200.20.168**（WiFi，与 09-30 相同，但系本次现查）。
- 活动控制台会话 **1**，无 LogonUI（未锁定），explorer pid 8648 位于会话 1。
- 8399 无任何 listener；无遗留 owned 进程/任务；9/30 两轮遗留均已清零。
- 受保护用户进程 **Notepad PID 24332 存活**（会话 1），全程未被定位/触碰；无任何全局/名称 kill。
- 非自有计划任务（DG-RPAD-Executor、rpa-*、Rpa* 等一批）原样存在，未触碰。

## 4. 启动与内层只读运行（全部 Interactive ScheduledTask，会话 1，无 Session0/SSH Start-Process）

| 组件 | pid | 任务/记录 |
|---|---|---|
| Host | 29944 | `RpaComputerRemoteHost-bef44b32d5e742e8a5a81308cbf26f24`，`100.200.20.168:8399` LISTEN 验证通过 |
| Fixture（multiclick） | 23032 | `AccFixture-bbdfe226b45a4b8abdc14597fc74ed0c`，run dir `run-bbdfe226b45a4b8abdc14597fc74ed0c` |
| Backdrop | 14916 | `RpaGuiRunner-7b4d9ec5867d4444aac64edb537920cf`（gui-neutral-desktop，MaxMinutes 25） |
| 内层 agent | 22352 | `RpaGuiRunner-5d3c49795d48456185e3698c99a15ac5`（MaxTurns 12 / MaxSeconds 180） |

- 证据目录（全新，未覆盖历史）：`C:\computer-cc-preflight-20260930-191500\evidence-20261008-b7c6bfbae6105fc2\`。
- 内层任务文件与 9/30 两轮逐字节相同（1030 bytes，SHA-256 `315CCCB0…48EC3`）：仅 describe/open/observe/close，
  明确禁止 step/get_step/pause/resume 及一切输入；不含源码/oracle 内容。仅 8 个 computer MCP 工具，
  内置工具禁用，strict MCP，hooks 禁用（既有 UTF8 runner 原样）。
- 运行目录 `9b78b036-9642-41b3-8a83-f8db29e43a69`：exit_code **0**，未超时，stdout drain 完成，stderr 空。
- **实际模型**：transcript 全部 message.model = {`glm-5.3-flash`}（8 条）——远端 Windows 路由独立核验通过，
  非 kimi-k3，无 API/鉴权失败。
- **实际工具调用（恰好 4 次，全部只读）**：`computer_describe` → `computer_open` → `computer_observe`
  → `computer_close`。**零** step/get_step/pause/resume，零输入，无内置工具。
- 内层判读为权威；内层 GLM 自述（仅记录，不替代协调者目检）：
  窗口 "Computer Use Acceptance" **完全可见、无遮挡**；套件 **MULTICLICK**、case **multiclick-01**、
  **Trial 1/10**、nonce **LS4BQ9**；TARGET 矩形及 Check/Next/Close 按钮清晰可见；
  状态 "Waiting for native input" / "READY"。**本轮未见防火墙/安全对话框遮挡**
  ——与 9/30 两轮（部分遮挡、pending manual handling）不同，与用户已人工处置授权的表述一致。

## 5. 初始截图（原始字节提取，未解释、未改写）

- **`/Volumes/doc/workspace/datagrand/rpa/rpa-computer/.agents/runs/windows-reauth-preflight-cc-20261008/initial-screenshot-reauth-20261008.png`**
  （131801 bytes，SHA-256 `5FA2B621A3BF0E3F3AF80E0B33D18C1B919F036923178D42F1A501E004E6A716`）。
  提取自 transcript tool_result 的 image block（media image/png），纯数据提取。
- → 交协调者人工目检；编排者未对截图像素做任何判读/下结论。

## 6. 未运行输入套件

尽管内层报告目标无遮挡，按边界**未运行任何输入套件**，零 computer_step。需协调者另行明确 GO。

## 7. 清理（全部经身份校验 helper）

- stop-windows.ps1：fixture task `AccFixture-bbdfe226…` 停止并注销；helper 报
  “process pid=23032 already ended with the task (identity verified)” —— 按既定更正，
  这是 Stop-ScheduledTask 之后的正常结束路径，**不构成 fixture 提前崩溃的证据**。
- gui-runner-stop ×2：backdrop task/pid 14916、inner task/pid 22352 注销并移除 record，证据保留。
- windows-remote-stop.ps1：host task + pid 29944 停止，record 移除，日志/凭证原地保留。
- 终检：0 个 owned 进程/任务；8399 listener 计数 0；**Notepad PID 24332 存活**；非自有任务未动。

## 8. 边界声明

- **source-free = 工具受限（仅 8 个 computer MCP 工具、禁 shell/a11y/CDP/oracle），不构成
  Windows OS 文件系统/沙箱隔离的证明**；不得标注 complete black-box。
- 无任何通过率/验收声明；本轮仅为连接与可见性 preflight。
- 9/30 两轮报告（windows-gesture-preflight[-2]-cc-20260930.md）原样保留，本轮为其授权续轮。
- 仅一次尝试；路由修正属于 GUI 启动前的修正，不是对 GUI 结果的重试。

## 9. 证据位置

- 本地：`.agents/runs/windows-reauth-preflight-cc-20261008\`
  （initial-screenshot-reauth-20261008.png、inner-run\{transcript.jsonl,run-summary.json,stderr.log}、
  inner-final-text.txt、inner-task.txt、host-start.log、fixture-start.log、backdrop-start.log、
  inner-start.log、cleanup.log、final-check.log、routing 修正的 transcript.jsonl/stderr.log）
- 外层 transcript：`.agents/runs/windows-reauth-preflight-sonnet-cc-20261008\`（model-probe.jsonl 等）
- 远端（原状保留）：`C:\computer-cc-preflight-20260930-191500\evidence-20261008-b7c6bfbae6105fc2\`
  （run dir + oracle evidence + inner run dir）、`host.log`。

## 协调者原始证据与像素复核

实际内层model=glm-5.3-flash、恰好describe/open/observe/close、native0且未超时；提取PNG与transcript原始image字节一致。已实际查看图像：MULTICLICK01/Trial1/10、nonceLS4BQ9，TARGET及Check/Next/Close均无遮挡，未见权限弹窗。由此解除原基线连接/可见性阻碍；不能据此认证当前新产物或真实输入。

“内层判读为权威”应收窄：内层模型自述只是待核对陈述，最终依赖原始截图/事件/工具记录，不以模型自述定通过。首次内层启动前出现过wrapper引号/路径拼接错误及Write工具拒绝，均在外层原始转录保留；其后第一次实际GUI agent运行成功，不宣称编排零错误。final-check.log仅Notepad存活与listener0，不是任意后代进程containment证明；确切自有任务清理见identity-checked helper日志。

完整核对：同名runs/coordinator-readback.json。随后批准当前Windows产物部署及单次multiclick套件，另有独立任务/证据；不覆盖本轮记录。
