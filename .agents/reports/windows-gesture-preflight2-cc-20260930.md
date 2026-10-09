# Windows gesture read-only visual preflight 第 2 轮 — CC+GLM（2026-09-30）

角色：CC+GLM 测试编排者（非实现作者）。无源码/产品/测试/helper 修改；无 subagent；无 commit/push/worktree。
无 Mac/Linux 测试。内层实际模型 **glm-5.3-flash**（未改动任何全局模型配置）。用户 Notepad PID24332 全程存活。

## 1. 复用与状态核验（未重建/未替换）

- 复用第 1 轮已部署路径 **`C:\computer-cc-preflight-20260930-191500\`**（18:09 冻结基线，非 feedback root）。
  启动前重验哈希，全部与第 1 轮一致：
  - bin\computer-host.exe `9EFF4BE7…DE769` ✓
  - bin\computer-client.exe `2EB020A4…DC84` ✓
  - FIXTURE_GREEN 目标 exe `C:\Temp\computer-gesture-cc-20260930-185517\build\ComputerUseAcceptance.exe`
    `3A5FA228477E176E42D05A614F20AD8E8187A7E6CD6217B84EE9411514F526FF` ✓（与第 1 轮/报告一致）
- 凭证/全局配置未改动；mcp.json 未变（仍指向本轮冻结 client + 存量
  `C:\computer-native-20260929-build\{ca.pem,client.token}` 路径，内容未读取）。直连 TLS，无隧道。
- 预检（DHCP 现查 IP **100.200.20.168**；主机 node1；控制台会话 1 无 LogonUI；8399 空闲 0 listener；
  无遗留 owned 进程/任务；第 1 轮另一 CC 的 SSH lib 测试在其它 TEMP，未接触；无其它 GUI 任务）。

## 2. 第 2 轮启动（均 Interactive ScheduledTask，控制台会话 1，helper 原样复用）

- **Host**：windows-remote-start.ps1 → pid **436**，task `RpaComputerRemoteHost-3316d56094c44421a1b4274fcb0fa0a3`，
  `100.200.20.168:8399` LISTEN 验证通过。
- **Fixture（multiclick suite，hash 校验通过）**：pid **27308**，run dir
  `evidence2\run-c9725f26c5184edfaf8f66896176bad2\`（全新，未覆盖第 1 轮 evidence）。
- **Backdrop**：gui-runner-launch-20260930.ps1 + gui-neutral-desktop-20260930.ps1（MaxMinutes 25）→
  pid **18128**，task `RpaGuiRunner-781c7168cf754fcbb369829f1de03527`；启动后进程活性已确认。

## 3. 内层只读运行

- 任务文件与第 1 轮逐字节相同（prompt 1030 bytes，SHA-256 `315CCCB0…4EC3`）：仅
  describe/open/observe/close，明确禁止 step/get_step/pause/resume 与一切输入；被遮挡/缺失/锁屏即停止并 close。
- gui-remote-agent-glm-utf8-20260929.ps1 经隐藏 gui-runner-launch 启动：pid **22476**，
  task `RpaGuiRunner-32c24493b6ab4601ac6cad59c6af95c6`，MaxTurns 12 / MaxSeconds 180。
- Run dir：`evidence2\dbb585e9-1238-47da-8cd0-39376bf2dbae\`；exit_code **0**，未超时，drain 完成，stderr 空。
- **实际模型**：transcript 逐条 message.model 汇总 = {`glm-5.3-flash`}（请求别名 sonnet）。
- **实际工具调用（恰好 4 次，全部只读）**：`computer_describe` → `computer_open` → `computer_observe`
  → `computer_close`。**零** step/get_step/pause/resume；无内置工具（strict-mcp + 空 tools + disableAllHooks）。

## 4. 初始截图（原始像素提取，未解释、未改写）

- 本地路径：**`.agents/runs/windows-gesture-preflight2-cc-20260930/initial-screenshot-preflight2.png`**
  （165481 bytes，SHA-256 `4967F00726172C7128DE49EAF0A4B1AC0A4788A579EED10FD54E86CCB79BF69D`）。
  提取自 transcript tool_result 的 image block（media image/png），纯数据提取。
- → **交协调者人工目检**；本报告不据此下任何 GUI 通过结论。
- 内层 GLM 自述（仅记录，不作为门禁依据）：窗口可见但**仍被 Windows 安全中心关于 computer-host.exe
  的防火墙对话框部分遮挡**（Allow/Cancel）；suite MULTICLCK 同前 — 确切为 MULTICLICK、
  case multiclick-01、Trial 1/10、**nonce 3W76XM**（新 trial，区别于第 1 轮 4E6Z7M）；
  TARGET 矩形被对话框遮挡，Check/Next/Close 与 READY 状态可见。已按规则 close 并停止
  （`cleanup_outcome: released`）。

## 5. 阻塞（按指示原样记录，未处理、未重试）

- **同一 Windows 防火墙/安全提示仍遮挡 TARGET**（第 1 轮与第 2 轮两份截图均涉及）。按任务边界
  **未点 Allow/Cancel、未做任何防火墙/系统设置更改**。→ 记录为 **pending manual handling**，
  需协调者人工处置（如预先放行该程序/端口）后另行 GO。本轮结束后未做任何 preflight 重试。

## 6. 清理（全部经身份校验 helper；非进程退出证明）

- stop-windows.ps1：task `AccFixture-c9725f26…` 停止并注销；helper 报 “process pid=27308 already ended
  with the task (identity verified)” —— 按第 1 轮报告更正，此为 **Stop-ScheduledTask 之后的正常结束路径**，
  不构成 fixture 提前崩溃的证据（两轮均不作该推断）。
- gui-runner-stop ×2：backdrop task/pid 18128、inner task/pid 22476 注销并移除 record，证据保留。
- windows-remote-stop.ps1：host task + pid 436 停止，record 移除，secrets/log 原地保留。
- 终检：0 个 owned 进程/任务；8399 无监听；Notepad PID24332 存活；全程零输入调用。

## 7. 边界声明

- **source-free = 工具受限（仅 8 个 computer MCP 工具、禁 shell/a11y/CDP/oracle），不构成
  Windows OS 文件系统隔离的证明**；不得标注 complete black-box。
- 无任何 GUI 输入套件运行；无任何通过率声明。第 1 轮报告
  `.agents/reports/windows-gesture-preflight-cc-20260930.md` 原样保留，本轮为其追加轮次。

## 8. 证据位置

- 本地：`.agents/runs/windows-gesture-preflight2-cc-20260930\`
  （initial-screenshot-preflight2.png、inner-run\{transcript.jsonl,run-summary.json,stderr.log}、
  inner-final-text.txt、inner-task.txt、fixture-start.log、backdrop-start.log、inner-start.log、cleanup.log）
- 远端（原状保留）：`C:\computer-cc-preflight-20260930-191500\evidence2\`（run dir + inner run dir + records）、
  `host.log`。
