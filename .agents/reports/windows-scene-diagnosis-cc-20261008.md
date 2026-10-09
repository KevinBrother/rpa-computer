# Windows scene 疑点诊断 — CC+GLM 只读 — 2026-10-08

Role: 诊断执行者（真实 Claude CLI，实际模型 `glm-5.3-flash`，已在内层 transcript 逐条核实：全部 assistant 消息 model 字段均为 `glm-5.3-flash`，别名 sonnet）。仅只读诊断：未操作 GUI、未唤醒/解锁、未结束进程、未改任何配置、未重试截图、未写产品/helper/测试代码、未 Add-Type 新诊断代码。全部原始输出在 `/private/tmp/windows-scene-diagnosis-cc-20261008/`（remote-*.txt 共 7 份，含首次输出，无删改）。

## 1. 待解释疑点

1. 截图任务栏日期 2026/10/3 11:33，而执行日志为 2026-10-08 12:57-12:58 (+08:00)。
2. observe-1.png 与 observe-2.png 逐字节相同（43871 B，SHA `ad19b5e4…`）。
3. 画面中无 fixture / backdrop / 本轮任何窗口。

## 2. 事实（全部来自只读采集，可复核）

**F1 主机时钟是准的。** SSH 实时：`2026-10-08 13:15:32 +08:00`，UTC `2026-10-08T05:15:32Z`，时区 China Standard Time (+08:00)，主机 NODE1。capture 时刻（内层 transcript UTC 04:57:55/04:58:02 = 本地 12:57:55/12:58:02）若画面实时，任务栏应显示 10/8 12:57。→ 疑点 1 不是主机时钟错误。

**F2 无活动显示器。** `Get-CimInstance WmiMonitorID` 返回**空**（无任何枚举中的显示器）。`Win32_VideoController`：AMD Radeon(TM) Graphics 输出 1920×1080 + NVIDIA **GeForce RTX 3050 Laptop** GPU（此为笔记本，与主机名 acer 一致），两卡 Status OK，GPU 仍保留 1920×1080 的分辨率记录（与 capture topology 的 native 1920×1080 一致）。

**F3 机器一直醒着、会话未变。** 9/28 以来 System 日志无 Kernel-Power 42（睡眠）/恢复事件；quser：console 会话 1 **Active**，登录时间 2026/9/16 16:58；无 LogonUI 进程（未锁屏）；explorer 自 9/16 运行；电源请求中 DISPLAY 无占用（仅 chrome.exe "Download in progress"，见 §5）。

**F4 画面内容是 10/3 时代的交互桌面，与本项目运行无关。** 本执行者亲读 observe-1.png：任务栏 2026/10/3 11:33、天气挂件 26°C；可见窗口为 (a) "Administrator" Windows Terminal，(b) "管理员: Windows PowerShell" 显示 `npm warn cleanup` / `changed 2 packages in 17s` / `codex-cli` / 反复的 `Error: sta… must not… To work wi… Session ID`——是一次 codex-cli 安装排错的交互记录；右侧中文笔记（"如果正确进入 TUI，就修复了" 等）；(c) 中央"空白窗口"实为标题 `C:\computer-cc-preflight-2026…` 的旧 Windows Terminal。**没有任何 10/8 本轮启动的窗口**（无 backdrop、无 fixture、无本轮 helper PowerShell）。

**F5 后端无缓存路径。** 代码核对：desktop.rs:92-117 每次 capture 都调用 `display.capture(budget::requested(1366, 768))`（1365×768 = 1920×1080 默认预算下缩放，非 override）；windows.rs 仅门面；运行时另有 geometry 绑定校验（desktop.rs:148-167）。两次 observe 各自独立走 GDI，工具层无帧缓存。

**F6 无崩溃痕迹。** Application 日志在 10/8 12:40-13:10 与 10/3 11:20-11:45 两个窗口内均无 Error/Warning；System 日志在 10/3 11:25-11:45 与 10/8 12:50-13:05 均为空（无显示驱动重置/显示器事件）。backdrop（MaxMinutes=25，12:57 启动）在 13:00 诊断时已消失——远早于存活期，且 Application 无 WER 记录，属静默退出。

**F7 启动参数与顺序逐字复核（外层 transcript）。** backdrop `gui-neutral-desktop-20260930.ps1 -EvidenceDir C:/computer-cc-preflight-20260930-191500/evidence-20261008-green-cc-01 -ExpiryMinutes 30 -ScriptArgs @('-FixturePid','31576','-FixtureExe','…/run-18f70838…/ComputerUseAcceptance.exe','-MaxMinutes','25')`；顺序 host(30612)→fixture(31576)→backdrop(29036, 12:57)→inner(26468, 04:57:35Z)。看门狗（fixture 存活检查）与 25min 寿命计时器均不应触发（fixture 当时仍存活）→ backdrop 更可能是 Shown 事件内抛错退出（`MainWindowHandle` 跨会话为零或 `SetForegroundWindow` 失败即 throw），与 memory 中"曾观察到一次 backdrop 早退、原因未知"一致；但只读证据无法确证。

## 3. 根因链（证据充分的主假设，机制层标注为假设）

- **事实链**：主机时钟准（F1）+ 无活动显示器（F2）+ 画面内容为 10/3 的交互桌面（F4）+ 后端逐次真实截图（F5）→ **"缓存/时钟错误/场景程序崩溃"三条均被排除**。画面不是本轮任何一个程序的输出，而是物理屏幕最后一次实际渲染（2026/10/3 11:33，用户人工使用时）的**冻结残留帧**。
- **假设（机制，未直接测得）**：笔记本屏幕自 10/3 11:33 后关闭/不枚举（合盖或屏幕熄灭，WmiMonitorID 空），桌面合成停止更新；GDI BitBlt 屏幕 DC 返回最后合成帧。该机制能同时解释疑点 1（10/3 时钟）、疑点 2（像素完全不变 → 确定性 PNG 编码 → 逐字节相同）、疑点 3（本轮新窗口从未进入合成）。已知缺口：GDI 在无活动显示器时返回陈旧帧的确切行为无法只读证明，需一次受控实验（见 §6）。
- backdrop 早退是**独立问题**（无事件痕迹、无存活记录可查），它使 fixture 调焦失败；但即使 backdrop 存活，冻结桌面上也不会出现任何新窗口。两问题不能互相归因。

## 4. 疑点定性结论

- 任务栏 10/3 11:33 = **画面冻结时刻**，非时钟漂移、非帧缓存。
- 两 PNG 相同 = **冻结帧重复确定性编码**，非传输层缓存。
- fixture 不在画面 = **scene 未建立**（backdrop 早退 + 桌面冻结双重原因），与上一轮报告一致且已正确停止。

## 5. 远端安全状态

机器醒着（SSH 实时响应）、会话 Active 未锁屏、9/28 起无睡眠。本轮 host(30612)/fixture(31576)/helper 任务均已按清理日志停止注销（当前进程查询已无 computer-host/computer-client/ComputerUseAcceptance）。Notepad 24332 存活未动。host.log 末段干净（writer-0 错误仅在 Stage-B 前缀）。注意两点（只读观察，未处置）：powercfg 显示 **chrome.exe 处于 "Download in progress" 电源请求**（远端有 Chrome 挂起的下载）；SYSTEM 有一条驱动请求 "Sleep Idle State Disabled"。

## 6. 下一步建议（不执行）

1. **不允许立即开始已有 layout 测试**：当前桌面冻结，任何 GUI 场景/布局实测都只会再次捕获同一张 10/3 冻结帧，结果必然无效。
2. 前置条件（人工，超出本次授权）：接外接显示器或打开笔记本屏幕，使 console 桌面恢复实时合成；恢复后由协调者安排**一次**最小验证截图（含任务栏特写）确认时钟与当日一致，再进入 layout 测试。
3. 请协调者交 sol 实现一个**环境预检 probe**（新增 Win32 查询，本执行者按约不自写）：在 `computer_describe` 输出中增加"活动显示器枚举"字段（WmiMonitorID 非空 / 活动显示拓扑），使 scene 测试在无活动显示器时 fail-fast，而不是产出冻结帧误导验收。

## 7. 证据清单（/private/tmp/windows-scene-diagnosis-cc-20261008/）

- `remote-clock-net.txt` — 实时时钟/时区/开机时间/IP
- `remote-sessions-procs.txt` — quser + 关键进程
- `remote-powercfg.txt` — 电源请求/可用睡眠态
- `remote-events.txt` — Application 两窗口 + Kernel-Power 粗查
- `remote-monitors-evidence.txt` — WmiMonitorID(空)/Kernel-Power 566/证据目录
- `remote-gpu-records-hostlog.txt` — GPU 状态/host.log 尾部
- `remote-system-events-windows.txt` — System 日志两精确窗口（均空）
- 本地核对：phase2 启动日志×4、外层 transcript-resume.jsonl（backdrop 参数）、内层 transcript.jsonl（model/timeline）、observe-1.png 亲读

---

## 8. 显式勘误（2026-10-08 第二轮补证；下文修正上文，上文原样保留不掩盖）

本章撤回/修正 §1-§6 中的错误结论。原始采集文件（remote-*.txt）均保留，不做改写。

**E1 任务栏读数错误（撤回"10/3 11:33 冻结五天"归因）。** 协调者对 `phase2/observe-1.png` 任务栏区域裁剪放大 6 倍亲读（`taskbar-analysis-enlarged.png`，裁剪 x=1240/y=725/w=125/h=43）：日期为 **2026/10/8**，时间约 **11:37**。本执行者复核该派生图，确认"2026/10/8"清晰可读；首轮低分辨率整图读成"10/3 11:33"是读图错误。§3 的"五天前冻结残留帧"根因链、"F4 画面内容是 10/3 时代"的定性、以及"codex 终端/天气挂件可佐证 10/3"均**作废**——终端窗口无法从截图断代。同时上一答"layout 测试会再次产出旧帧、不允许开始"的反对**撤回**：layout 测试测的是真实 WinForms 窗口几何、不读取截图，与画面时效无关；此时远端若出现新测试进程属另一 CC 已授权的自有运行，不是前次遗留。

**E2 "WmiMonitorID 空 = 无活动显示器"是采集解读错误（撤回 F2 的"无任何枚举中的显示器"）。** 本轮不做 `-ErrorAction SilentlyContinue`、不改管道地重采（`remote-monitor-cim-noisy.txt`，ssh exit 0）：
- `WmiMonitorID`：查询**成功返回 1 个实例，Active=True**，仅 UserFriendlyName 为空串。首轮输出为空是我自己的管道（拼接 friendly name）把空名过滤成了空输出——误判为"零实例"。
- `Win32_DesktopMonitor`（现时刻）：2 个实例——内置 LG 面板（PNPDeviceID `DISPLAY\LGD061E\…`）Status OK 但 **Availability=8 (Off Line)**；"默认监视器" **Availability=3、ScreenWidth/Height=1920×1080**。
- `Win32_VideoController`：两卡 Status OK、ConfigManagerErrorCode 0；AMD 当前模式 1920×1080，NVIDIA 3840×2160。

即：**存在活跃的 1920×1080 显示模式 + 一个离线的内置面板**。"机器一定合盖"不能认定（InputAccelerometer 事件仅提示曾有加速度计输入，物理状态未证实）；且这些查询测的是**现在（13:2x）**，不证明 capture 时刻（12:57-12:58）的状态。

**E3 撤回"三疑点统一根因"的确定性表述。** 保留的硬事实：
- F1（主机时钟准）、F5（后端每次独立 GDI capture、无自身缓存）、F6（两窗口无 Application 错误）维持；
- **新增硬事实**：capture 时刻任务栏读数（10/8 11:37）与 capture 时刻主机时钟（12:57:55/12:58:02 +08:00）**相差约 80 分钟**——不管机制如何，被捕获像素与 capture 时刻墙钟不一致这一矛盾仍然成立，`freshness_verified=false` 维持；
- **新增硬事实**：Kernel-Power 566（完整 XML 见 `remote-kp566-full.txt`，ssh exit 0）：10/8 **11:38:00** 会话 111→112 Reason **SessionUnlock**（前会话时长 2628243499µs≈43.8min，与 10:54:12 事件精确衔接）；10/8 **10:54:12** 会话 109→111 Reason **InputAccelerometer**。11:38:00 事件与任务栏 11:37 同分钟级吻合，可作为画面时刻的**锚点**。

**未证实/开放（不作结论）**：
- 画面是"整个桌面合成冻结于 ~11:37-11:38"还是"仅任务栏时钟停走"——两 PNG 间隔 7 秒且逐字节相同，与两者都相容，只读证据无法区分；
- 11:38 SessionUnlock 之后至 12:57 之间发生了什么（该区间 System 日志 12:50-13:05 窗口此前已查为空，11:35-11:45 窗口未再扩查——按本轮补证边界止步）；
- Availability=8 的内置面板与画面内容的确切因果（面板离线 → 合成冻结 → GDI 返回陈旧帧的机制链**仅为假设**，GDI 代码无自身缓存也不能排除 OS 层画面状态）；
- 无 LogonUI、quser Active 不构成"已解锁可输入桌面"的充分证明。

**E4 边界重申。** 本轮补证仅限：566 完整 XML/Message、无屏蔽显示器 CIM 三查询；无网络/电源/权限修改、无 Add-Type/helper、无输入/唤醒、未截图。所有 SSH 调用单独记录退出码（均 0）且 stderr（含 post-quantum 警告）原样保留，未以管道 exit 0 冒充 SSH 成功。不提出生产 fix、不新增实现。memory 中本轮首轮写入的错误内容已修正为事实/未证实假设（仅改本轮自加条目，未动其他 memory）。

**新证据文件（Data 根下，均新增不覆盖）**：`/private/tmp/windows-scene-diagnosis-cc-20261008/remote-kp566-full.txt`、`remote-kp566-full.stderr`、`remote-monitor-cim-noisy.txt`、`remote-monitor-cim-noisy.stderr`；派生图 `taskbar-analysis-enlarged.png`（协调者产出）。
