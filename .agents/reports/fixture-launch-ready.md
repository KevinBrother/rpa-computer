# Fixture launch helpers READY — start-windows.ps1 / stop-windows.ps1

日期：2026-09-24 ｜ 角色：QA 脚本修复（qa-fix-3，步骤 A）
范围：`acceptance-fixture/start-windows.ps1`、`acceptance-fixture/stop-windows.ps1`、
`acceptance-fixture/README.md` 与本报告。
**未运行任何 GUI，未注册/启动任何计划任务，未注入任何输入，未触碰任何桌面锁定状态。**

## 本轮修复（相对 fixture-windows-launch 版）

1. **`New-ScheduledTaskAction -Arguments` → `-Argument`（真实参数名）**
   目标机 acer-win 实际安装的参数集（远端实测
   `(Get-Command New-ScheduledTaskAction).Parameters.Keys`）为
   `Id,Execute,Argument,WorkingDirectory,CimSession,ThrottleLimit,AsJob,...` ——
   **没有 `Arguments`**。旧脚本用 `-Arguments` 会在注册时直接 ParameterBinding 失败。
   已改为 `-Argument`。
2. **stop-windows.ps1 两个错误分支的 `-f` 位置修复**
   旧代码把 `-f <args>` 写在 `Write-Error` 实参之后（`Write-Error (...) -f ...`），
   `-f` 会落到 **Write-Error 的实参列表**里而不是格式化字符串上（PowerShell 接受任意
   位置参数数组），导致报错信息变成未格式化模板 + 独立数组。已改为先构造
   `$msg = (...) -f ...` 再 `Write-Error $msg`。
3. 其余安全属性保持不变：GUID 唯一运行目录/任务名、碰撞即拒绝、只复制 exe、
   30 分钟执行时限兜底、停止前复核任务 action/参数/principal 与进程
   PID/路径/会话/创建时间，不匹配即拒绝，从不按名字强杀、从不删除任何目录/任务/用户文件。

## 验证（全部 read-only / 内存内；未注册任务、未启动进程）

- 语法解析（远端 Windows PowerShell 5.1，`System.Management.Automation.Language.Parser.ParseFile`，仅解析）：
  `SYNTAX OK start-windows.ps1`、`SYNTAX OK stop-windows.ps1`。
- 语义检查（远端、纯内存对象构造，**未调用 Register/Start-ScheduledTask**）：
  - `New-ScheduledTaskAction -Execute C:\Temp\accfix\ComputerUseAcceptance.exe -Argument '--evidence-file "..."'`
    构造成功，`Action.Execute`/`Action.Arguments` 回读正确；
  - `New-ScheduledTaskPrincipal -UserId Administrator -LogonType Interactive -RunLevel Limited` 构造成功；
  - `New-ScheduledTaskSettingsSet -ExecutionTimeLimit PT30M` 构造成功；
  - `Register-ScheduledTask` 参数集实测含 `TaskName,Action,Principal,Settings`（脚本所用全部存在）。
- 会话/principal 解析路径远端实测（read-only）：
  `WTSGetActiveConsoleSessionId() = 1`，该会话 explorer.exe 属主 = `NODE1\Administrator`，
  与早前 fixture-windows-launch 报告一致。
- 修复后的两个脚本已 scp 到 `acer-win:C:\Temp\accfix\`（与本地哈希一致，见下）。
  exe 哈希默认期望值 `2A80F568144C68B7370B8DE076F92ECB846F8BEBAF2361AF642AABE9B4ED1F32` 未变。

## 已审核快照（SHA-256，协调者按此快照运行）

- `acceptance-fixture/start-windows.ps1` = `446adf038031762580e652dfe6c55887c5881538f37afdc75cf38e75307ea27b`
- `acceptance-fixture/stop-windows.ps1`  = `026d4331cd09867360f1f03c0a965018f650d91ac84962dbbfd89d0c8d8e3744`  ← **最终版（见下）**
- `acceptance-fixture/README.md`         = `e92fc8613ce0efc31299c95996ec707afa73afb1e6507f1e0ee97a9271eaa9f1`

> **stop-windows.ps1 哈希更新（qa-fix-3 收尾）**：早前列出的 `96b92f...` 已两次被取代——
> (1) verify-first 重排（先读+验任务与进程，全部匹配才 mutation）后为 `a31e21...`；
> (2) 又在 Stop-Process 前加了存活复查（任务停止可能已结束同一 PID——已消失的**已验证** PID 视为安全成功，
> 不再误报），最终为 `026d4331...`。**部署停止脚本请以 `026d4331...` 为准。**

## 诚实声明 / 缺口

1. 脚本至此**仍未在真实交互会话中执行过**：计划任务注册→启动→进程复核→停止的完整链路
   只做了内存级语义验证；首次真实运行由协调者按 README 章节执行并核对截图与 oracle。
2. 进程存活/会话匹配**不代表 GUI 成功**。
3. 本机 Mac 桌面处于锁定状态，未尝试解锁；Windows 端状态仅做 read-only 查询。
4. 除非协调者报告运行时失败，本快照不再改动。
