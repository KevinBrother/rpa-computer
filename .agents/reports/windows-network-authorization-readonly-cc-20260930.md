# Windows 网络授权只读诊断 — CC（2026-09-30）

只读查询，零变更：未启动 Host/fixture/GUI、未绑端口、未发送输入、未点 Allow/Cancel、未增删防火墙规则/凭证/权限/全局设置、未重启服务、未复制/替换任何文件。查询于 2026-09-30 15:43 UTC 经 SSH acer-win 以只读 PowerShell cmdlet 完成，无提权（未遇 admin 失败）。Notepad PID 24332 存活。

## 核心发现：两条路径均为已启用的入站 Block 规则，无 Allow 规则

对任务给定的两个精确程序路径，`Get-NetFirewallApplicationFilter`+`Get-NetFirewallRule` 各匹配到 2 条规则（TCP+UDP，"TCP/UDP Query User" 命名样式，即对话框处置产物），**全部为 Block**：

| Program | 方向 | Action | Enabled | Profile | PolicyStoreSourceType | PrimaryStatus |
|---|---|---|---|---|---|---|
| `C:\computer-native-20260929\bin\computer-host.exe`（旧路径） | Inbound | **Block** | True | Private | Local | OK |
| 同上（UDP） | Inbound | **Block** | True | Private | Local | OK |
| `C:\computer-cc-preflight-20260930-191500\bin\computer-host.exe`（preflight 路径） | Inbound | **Block** | True | Private | Local | OK |
| 同上（UDP） | Inbound | **Block** | True | Private | Local | OK |

### 与 preflight2 阻塞的对应关系（仅区分观察，不下结论）

- **旧 native 路径（20260929）与 preflight 路径（20260930-191500）被分别登记**，且状态一致：各 2 条已启用入站 Block。两路径没有 Allow 规则。
- "known old path authorization" 与 "new preflight path" 的区分结果：**两条路径当前都没有授权（Allow）记录，只有 Block 记录**。不存在旧路径已被放行、仅新路径待处理的情形。
- 该 Block 规则命名（Query User）与 preflight2 报告的安全中心对话框相容：最可能的历史处置是 Cancel（生成 Block），但**对话框当前是否仍打开/是否再次弹出无法经只读查询确定**。

### 明确的区别声明（按任务要求）

- **观察到规则 ≠ 有效访问**：规则的优先级/其它 profile/策略叠加未评估，不能把 Block 规则断言为在所有条件下必然生效；反之也不存在任何已见 Allow 证据。
- **规则 ≠ 桌面无遮挡**：即便规则存在，preflight2 记录的 UI 对话框状态未知；用户尚未确认当前对话框已处理。本次未做任何 GUI 观察或交互。
- **网络在线 ≠ Host 可连**：InterfaceIndex 3、NetworkCategory=**Private**、IPv4Connectivity=Internet。当前活动网络恰为 Private（与上述规则 profile 匹配），但这不构成任何连接成功或放行生效的证明。

## 其他状态

- **8399 LISTEN**：查询无匹配 MSFT_NetTCPConnection 实例 → **无监听**（JSON 中 listen_count 记为 null 并原样保留 CIM 无匹配消息；即 0 listener，与前轮终检一致）。未尝试连接。
- **Notepad PID 24332**：存在（`notepad_pid_24332_exists=true`），零交互。

## 边界

- 未读取 FileHash/规则详细内容/任何 ca/token/私钥；未导出其它应用的规则；未收集 SSID/用户/账户信息。
- 本诊断不请求、不假设任何修改权限；无 bypass——对两条路径同等对待，均未变更。
- 这不是第三轮 GUI preflight；对 preflight2 的 pending manual handling 状态无改动。

## 证据

- runs：`.agents/runs/windows-network-authorization-readonly-cc-20260930/`
  - `query.ps1`（一次性查询脚本）、`query.command.txt` + `query.timestamp`、`query.stdout`、`query.stderr`、`query.exit`（0）
  - `network-auth-query-cc-20260930.json`（远端原始 UTF8 JSON 输出，规则/网络/端口/Notepad 全部原始字段）
  - `upload.scp.stderr`、`cleanup.ssh.stderr`（查询后已删除远端两个一次性 TEMP 文件）

停止；是否处置 Block 规则/对话框由协调者决定。

## 协调证据复核与措辞纠正（2026-09-30 23:46 CST）

已亲读原JSON和numeric query exit：两条精确程序路径各有TCP/UDP两条Enabled=True、Inbound/Block、Private、Local规则；活动network category为Private。仅能确认本次查询所得规则，不能扩大为全局无Allow/所有路径被阻止，不能替代有效连接测试或当前桌面截图。**规则名称不能证明用户之前点击了Cancel**；原“最可能的历史处置”是未证推测，不采信、不归责用户。

端口查询返回具体“无匹配MSFT_NetTCPConnection”错误，count原样null；不把一般查询失败自动改成0。此处可记录该次查询未找到8399 LISTEN，不声称长期状态。Notepad24332存在。

没有改变规则、权限或产品文件；用于查询的两个临时文件曾写入并自行清理，因此“未复制任何文件”应读为未复制/替换产品或用户文件，而非零文件操作。用户尚未授权本轮修改防火墙，**是否处理规则/授权弹窗由用户决定**，协调者不擅自放行、不换程序路径来绕过。未启动第三轮GUI预检。
