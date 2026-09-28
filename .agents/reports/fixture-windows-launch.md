# Windows 靶场交互会话启动/停止脚本报告（仅语法验证，未启动 GUI）

日期：2026-09-24 ｜ 角色：fixture worker ｜ 范围：`acceptance-fixture/start-windows.ps1`、
`acceptance-fixture/stop-windows.ps1`、`acceptance-fixture/README.md` 与本报告。
**未运行任何 GUI，未启动任何计划任务，未注入任何输入；未触碰任何桌面锁定状态。**

## 交付物

- `acceptance-fixture/start-windows.ps1`（132 行）：协调者专属启动器。校验源 exe 精确路径
  （默认 `C:\Temp\accfix\ComputerUseAcceptance.exe`）+ SHA-256（默认
  `2A80F568144C68B7370B8DE076F92ECB846F8BEBAF2361AF642AABE9B4ED1F32`）与**已存在**的协调者
  证据目录；在其中新建 GUID 唯一运行目录，只复制 exe（不含源码），oracle 证据文件位于受控
  运行目录内；经 `WTSGetActiveConsoleSessionId` 取活动控制台会话，以该会话 explorer.exe 属主
  为 principal（Interactive / Limited）注册**全新一次性**计划任务，直接执行 staged exe
  `--evidence-file <oracle>`（无 shell/任意命令）；启动后复核进程精确路径+会话+创建时间，
  写 `run-record.json` 记录任务 action/参数/principal 与进程身份；输出 oracle 路径与 PID
  （不含 nonce 内容）。执行时限 30 分钟兜底，不删不改任何既有目录/任务。
- `acceptance-fixture/stop-windows.ps1`（69 行）：消费 run-record.json，先复核任务 action/
  参数/principal、进程 PID/路径/会话/创建时间全部匹配，才停止并注销**仅属于本工具**的
  任务与进程；任何不匹配即拒绝（无模式强杀、无覆盖、不删源 exe/用户文件）；证据目录保留。
- `acceptance-fixture/README.md` 新增「协调者启动/停止」章节：含精确 scp/ssh 用法
  （只复制两个辅助脚本到 `C:\Temp\accfix\`，由协调者用新运行目录调用既有 exe）。

## 验证（read-only，未执行脚本）

- 远端 exe 复核（ssh acer-win，Get-FileHash）：
  `C:\Temp\accfix\ComputerUseAcceptance.exe` SHA-256 = `2A80F568144C68B7370B8DE076F92ECB846F8BEBAF2361AF642AABE9B4ED1F32`，
  与编译报告一致；15360 字节。远端会话：`console Administrator ID 1 Active`（SSH 账号
  `node1\administrator`，PowerShell 5.1.26100.9444，Windows 10.0.26200）。
- 语法验证（远端 PowerShell 5.1 自带 `System.Management.Automation.Language.Parser.ParseFile`，
  仅解析不执行）：
  - `SYNTAX OK C:\Temp\accfix\start-windows.ps1`
  - `SYNTAX OK C:\Temp\accfix\stop-windows.ps1`

## 诚实声明 / 缺口

1. **未做任何实际 GUI 运行**：两脚本只通过语法解析验证；计划任务注册、会话解析、
   进程复核、停止路径均未在真实会话中执行，需协调者在交互式桌面实测。
2. 进程存活/会话匹配**不代表 GUI 成功**；判定仍需协调者核对截图与 oracle 证据，
   本交付不含截图或模型循环。
3. 本机 Mac 桌面当前处于锁定状态，未尝试解锁任何桌面。
4. 停止脚本只停记录中的进程；若任务执行时限（30 分钟）外仍无记录文件，遗留任务
   需人工按任务名 `AccFixture-<GUID>` 排查（脚本不会代为清理）。
