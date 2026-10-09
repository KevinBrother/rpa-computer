# Windows gesture read-only visual preflight — CC+GLM（2026-09-30）

角色：CC+GLM 测试编排者（非实现作者）。未修改任何源码/测试/helper；未 commit/push/worktree；无 subagent。
用户 Notepad PID24332 全程存活、未受影响。Mac 侧仅 SSH 编排/复制/记录。

## 1. 输入边界确认

- FROZEN_HOST 基线（18:09 Windows release 构建，非当前 HEAD）本地冻结二进制逐字节核对：
  - computer-host.exe `9EFF4BE7…DE769` ✓ 与任务给定一致
  - computer-client.exe `2EB020A4…DC84` ✓
- FIXTURE_GREEN：`gesture-fixture-windows-cc-20260930.md` + `gesture-analyzer-windows-cc-retest-20260930.md`
  （csc 构建绿 / self-test 89 绿 / analyzer 26 绿 / parity 绿）。目标 exe
  `C:\Temp\computer-gesture-cc-20260930-185517\build\ComputerUseAcceptance.exe`
  远端实测 SHA-256 `3A5FA228477E176E42D05A614F20AD8E8187A7E6CD6217B84EE9411514F526FF`
  与报告 §4 **完全一致**后才启动。
- GUI_SLOT_RELEASED 仅限只读首屏视觉门禁；未发送任何输入套件。

## 2. 只读预检（step 1）

- 主机 `node1`，OS 控制台会话 **1**，无 LogonUI（未锁屏）。
- 当前 IP（DHCP 现查）：**100.200.20.168**。
- 端口 8399 空闲（0 listener，无外来占用者）；无遗留 RpaComputerRemoteHost/RpaGuiRunner/AccFixture 任务；
  无遗留 computer-host/ComputerUseAcceptance 进程。
- 存量部署 `C:\computer-native-20260929` 只读检查：其 bin 与 FROZEN_HOST 基线**哈希不同**
  （host `389EDFD9…`，client `4573197A…`），故未复用其 exe，仅按授权复制凭证文件到新部署。

## 3. 全新 source-free 部署

- 远端目录：`C:\computer-cc-preflight-20260930-191500\`（全新、本轮独占）。
- bin/ 为本地冻结基线二进制 scp 上传，远端实测哈希与本地一致（host/client 见 §1）。
- tls\server.pem（`2C6A16E3…7797`）、tls\server.key（`53D91AD0…19C7`）、host.token（`FA8FDABF…A15`）
  **从存量部署复制而来**（内容未打印、未改动原部署、未改任何凭证）；windows-remote-start.ps1 启动时对
  host.token/server.key 重新设 ACL（仅 console-user SID/SYSTEM/Administrators）。
- helpers/ 8 个经审核 helper 原样上传，远端哈希与本地逐一相等：
  windows-remote-start `8FA3B441…7DEB`、windows-remote-stop `97D7DB2D…1995`、
  start-windows `9C44F70E…F099`、stop-windows `026D4331…3744`、
  gui-runner-launch-20260930 `6A6871A1…FB9`、gui-neutral-desktop-20260930 `43BEB6F2…DD8`、
  gui-runner-stop-20260929 `D5EB1791…789`、gui-remote-agent-glm-utf8-20260929 `FC5A733E…1D6A`。
- mcp\mcp.json（本轮新配置，`B30AADF2…`→后被本轮配置替换，见本地
  `runs/windows-gesture-preflight-cc-20260930/mcp.json`）：指向**本轮冻结 client**，凭证仅引用存量路径
  `C:\computer-native-20260929-build\{ca.pem,client.token}`（存在性已验证，内容未读取）。直连 TLS，无隧道。

## 4. Host / fixture / backdrop 启动（step 2，均 Interactive ScheduledTask，控制台会话 1）

- **Host**：windows-remote-start.ps1 → `100.200.20.168:8399` LISTEN 验证通过；
  pid **23640**，task `RpaComputerRemoteHost-e5e289596b5d48d7b0c09c231cf8079e`，用户 NODE1\Administrator。
- **Fixture**：start-windows.ps1（Suite multiclick，hash 校验通过后 staged）→ pid **22364**，session 1；
  run dir `evidence\run-2772673b290f48ac88c678489c431b9a\`；oracle 仅协调者可见。
- **Backdrop**：gui-runner-launch-20260930.ps1（-WindowStyle Hidden 隐藏**自身** runner console）+
  gui-neutral-desktop-20260930.ps1（MaxMinutes 25，task expiry 30m）→ pid **6808**，task
  `RpaGuiRunner-384efd18659b4254913f02a611c87ea8`。

### 保留的一次失败（编排调用错误，非源码缺陷）

首次 backdrop/inner 启动把参数名以**不带 `-` 的裸 token**（`ReleaseDir`、`FixturePid`…）传给
`powershell -File`，被当作位置参数 → 两个脚本立即参数绑定失败（无窗口、无内层运行、无 run 目录）。
两个实例 record 经 gui-runner-stop-20260929.ps1 身份校验后清理，原样记录于
`runner-retry-stop.log`；随后改用带 `-` 的参数名重跑成功。未隐藏任何失败。

## 5. 内层只读视觉任务（step 3）

- 任务文件 `evidence\inner-task.txt`（本地副本 `inner-task.txt`，prompt 1030 bytes，
  SHA-256 `315CCCB0…4EC3`）：**仅** describe/open/observe/close，明确禁止
  computer_step/get_step/pause/resume 与任何输入；screen 不可读则如实上报并 close。
- 经 gui-runner-launch 隐藏启动 gui-remote-agent-glm-utf8-20260929.ps1：
  pid 4728，task `RpaGuiRunner-4625bd3199f84b07b965b602b8891c2a`，MaxTurns 12 / MaxSeconds 180。
- Run dir：`evidence\7f95e1f9-3b4f-4a80-8a3b-4ad6891db80a\`，exit_code **0**，未超时，drain 完成。

## 6. 内层运行证据（step 4）

- **实际 assistant 模型：`glm-5.3-flash`**（transcript 逐条 message.model 汇总；请求别名 sonnet）。
- **实际工具调用（严格 4 次，全部只读，零输入调用）**：
  `mcp__computer__computer_describe` → `computer_open` → `computer_observe` → `computer_close`。
  无 step/get_step/pause/resume，无内置工具（`--tools ''` + `--strict-mcp-config` + disableAllHooks）。
- **初始截图（原始字节提取自 transcript tool_result，未改任何像素）**：
  本地 **`.agents/runs/windows-gesture-preflight-cc-20260930/initial-screenshot-preflight.png`**
  （163070 bytes，SHA-256 `86922B2687A60DF5D141B9A734FD43930C97E9F24B02D1EB42D40583D9B1EBCA`）。
  → **交协调者人工目检**；本报告不据此下任何 GUI 通过结论。
- 内层 GLM 的文字结论（其自述，仅记录、不作为门禁依据）：
  “Computer Use Acceptance”窗口可见但**被 Windows 安全中心关于 computer-host.exe 的防火墙对话框部分遮挡**；
  suite=MULTICLICK，case=multiclick-01，trial=1/10，nonce=4E6Z7M；TARGET 矩形存在但中心被对话框遮住，
  Check/Next/Close 按钮完全可见。它按规则调用 computer_close 并停止（`cleanup_outcome: released`）。
- 证据副本：`inner-run/{transcript.jsonl,run-summary.json,stderr.log}`（stderr 为空）+ `inner-final-text.txt`。

## 7. 清理（step 5，全部身份校验后执行）

- stop-windows.ps1：fixture task `AccFixture-2772673b…` 停止并注销；**记录在案：fixture 进程
  在清理前已自行结束**（“already ended with the task”，成因未查明，原样保留这一观察）。
- gui-runner-stop ×2：backdrop task/pid 6808、inner task/pid 4728 均注销并移除 record。
- windows-remote-stop.ps1：host task + pid 23640 停止，record 移除，secrets/log 原地保留。
- 终检：0 个 computer-host/ComputerUseAcceptance 进程；0 个遗留任务；8399 无监听；
  Notepad PID24332 存活。已确认**全程零输入调用**（transcript 工具序列即证明）。

## 8. 边界声明

- **source-free ≠ OS 级文件系统隔离**：本轮内层 agent 的 MCP 仅限 8 个 computer 工具且未给出
  任何 shell/a11y/CDP/oracle 通道，但 Windows 主机文件系统本身**未被证明**与内层 agent 隔离
  （非黑盒完整隔离结论）。不得标注 “complete black-box”。
- 本轮仅首屏视觉预检：**无任何 GUI 输入套件运行、无任何通过率声明**。初始截图待协调者
  人工审查后另行 GO，方可进行首个输入 suite。

## 9. 阻塞/待办

1. **防火墙对话框遮挡 TARGET 中心**（内层自述 + 待截图复核）：属系统级对话框，按规则
   未做任何防火墙/系统设置更改。需协调者决定处理方式（如预先允许该程序/端口）后重排首屏。
2. fixture 进程提前结束的成因未查明（原样保留）。
3. 截图人工目检：协调者需查看 `initial-screenshot-preflight.png` 并确认/修正内层自述。
