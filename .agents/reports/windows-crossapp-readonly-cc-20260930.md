# Windows crossapp 只读准备调查 — CC — 2026-09-30

- 执行身份：CC（经既有 SSH acer-win 只读元数据查询；**未启动/激活/关闭任何应用或 Host，无 GUI/输入/截图/录屏，未改权限/防火墙/模型/全局设置，未运行任何 GUI 测试**）。实际模型 glm-5.3-flash。
- 性质：仅为已批准 crossapp-01..06 的**未来启动条件准备事实**；不是第三次 GUI 预检，更不是跨应用验收。

## 1. OS / console session

| 项 | 值 |
|---|---|
| OS | Microsoft Windows 11 Pro, 10.0.26200 (build 26200), 64-bit |
| 当前查询进程 session | 0（SSH 派生服务会话） |
| 登录会话 | 13 条（type 0×1、2×4、3×2、5×6；这是登录记录，不是当前活动桌面证明） |
| 查询时间 | 2026-09-30T17:07:24Z（远端本地 10-01 01:07 +08:00） |

## 2. 受保护进程 PID 24332（仅存在性/身份）

- **存在**：`Notepad.exe`，session **1**（交互 console），exe=`C:\Program Files\WindowsApps\Microsoft.WindowsNotepad_11.2607.14.0_x64__8wekyb3d8bbwe\Notepad\Notepad.exe`，创建于 2026-09-30 前（epoch ms 1790668823353）。
- 未读窗口内容/文档/命令行/其他用户进程。**它属于 Store 版 Notepad（WindowsApps 包）**，与 System32 经典 notepad.exe 不是同一二进制；未来清理只能按届时确认的 owned HWND，绝不按名称/PID 清理。

## 3. 应用安装元数据（存在性 + 文件版本；未读浏览器 profile）

| 目标 | 状态 | 路径 / 版本 |
|---|---|---|
| Explorer | ✅ | `C:\WINDOWS\explorer.exe` 10.0.26100.8117 |
| System32/Windows Notepad 路径 | ✅ | `C:\WINDOWS\System32\notepad.exe` 与 `C:\WINDOWS\notepad.exe` 10.0.26100.8457 |
| System32 Calculator 路径 | ✅（存在） | `C:\WINDOWS\System32\calc.exe` 10.0.26100.8521（仅确认文件存在，未执行启动或验证重定向） |
| 经典 mspaint | ❌ 不存在 | `C:\WINDOWS\System32\mspaint.exe` 无（Paint 走 Store 包） |
| Edge | ✅ | `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe` **154.0.4258.37** |
| Chrome | ❌ 两处标准路径均不存在 | 未安装（按本查询范围） |
| Appx Calculator | ✅ | `Microsoft.WindowsCalculator_11.2607.0.0_x64__8wekyb3d8bbwe` |
| Appx Notepad | ✅ | `Microsoft.WindowsNotepad_11.2607.14.0_x64__8wekyb3d8bbwe` |
| Appx Paint | ✅ | `Microsoft.Paint_11.2605.81.0_x64__8wekyb3d8bbwe` |

Appx 查询为精确名称过滤（Name/PackageFullName/InstallLocation）；三个包各恰好 1 条，无查询失败。

## 4. 运行时依赖

- **Python 3.12**：`C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe` 存在，FileVersion **3.12.10**（真实安装，非 Store 占位符；未执行任何 Python）。
- **PowerShell**：5.1.26100.9444 Desktop，`C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe`——与既有 bounded runner 兼容（前轮已实际验证）。

## 5. 可用性分类（对未来启动条件的诚实状态）

| 分类 | 项 |
|---|---|
| 已确认（安装/版本事实） | OS 版本、上述全部安装路径/版本、Python3.12、PowerShell 5.1、PID24332 存在及 session=1 |
| **需未来 GUI 所有权确认** | 任何 Notepad/Calculator/Explorer/Edge 新窗口的 HWND/session 归属；PID24332 现有窗口**不可复用也不可按 PID 清理**；Explorer 常驻进程不能被清理（未来只清已确认 owned HWND） |
| 不可用/受限 | Chrome 两个已查询标准路径不存在（不能排除其他安装位置）；经典 mspaint.exe 不存在；安装路径存在 ≠ 新窗口独占——**不能声称可安全复用用户已有 Calculator/Notepad/Explorer**；browser 未用且未来必须专属 profile + 专属下载目录，不得使用现有 profile |

## 6. 执行与归档

- 单个只读查询脚本经 base64 传输到**新 owned TEMP** `C:\Users\Administrator\AppData\Local\Temp\computer-crossapp-ro-cc-2b4e5080e7a742b39bafbcfd5deff220`，以 `powershell -File` 执行，stdout 重定向落盘：**PS_EXIT=0**，stdout 10844 bytes，stderr 0 bytes；无 Start-Process/计划任务/app 启动/录屏截图；未读防火墙政策、未再测 8399。
- 归档 `.agents/runs/windows-crossapp-readonly-cc-20260930/`：`crossapp-readonly-query-result.json`（原始精确 JSON，SHA256 `aa9db364…`）、`query-script-copy.ps1`、`query-run-log.txt`（远端 stdout/stderr 长度与退出记录）、`remote-temp-final.tgz`（远端 TEMP 终态含 query-stdout.log/query-stderr.log）。
- 远端写入：仅该 TEMP 内 `query.ps1`（查询脚本）与两个输出日志；**查询脚本已按授权删除**（复查 `Test-Path=False`），result/日志保留在远端 TEMP，本地已全量归档。**不声称零文件写入**——共写入上述 3 个文件。

## 7. 边界（不隐瞒）

1. 结果仅为准备事实；**不是 GUI 预检/跨应用验收**；未获得也未等待 GUI 授权弹窗处理确认。
2. 版本/路径为查询时刻快照，未来 run 前如需应重新确认；未声称任何应用可被"安全复用"。
3. 本地一次控制台直读 JSON 出现非 ASCII 字节（©）损坏（编码往返问题，wrapper 层面），已改 base64 字节精确拉取纠正；首次读取记录保留于运行日志。
4. 未触碰产品/测试源码、root Rust、Mac/Linux；无 commit/push/worktree。

**STOP：只读调查完毕并归档。跨应用 GUI 仍待协调者授权。**

## 协调者 raw 复核纠正

原文将type5计数写7，raw为6，另有type0一条；上表已纠正。System32 notepad/calc文件存在和版本字段不能证明“经典应用”“启动必定转跳”或独立窗口能力，已收窄措辞；Chrome只排除两条已查询路径，不能断言全机未安装。当前查询session0与保护Notepad session1都不证明桌面未锁定/活动console/权限弹窗已处理。未新增查询或GUI动作。
