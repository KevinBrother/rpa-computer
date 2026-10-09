# Complete-Actions 双平台只读环境预检 — 2026-09-30 (CC+GLM)

只读预检，未写产品源码/测试/脚本，未启动桌面应用，未做任何 GUI 输入/截图，未创建计划任务，未 commit/push。SSH 仅只读查询。

## 1. macOS 显示探针（只读 probe，未唤醒/未解锁）

- 脚本：`.agents/runs/native-mac-display-probe-20260929.py` — 已确认只读（仅 CG/AX 查询 API；`CGPreflightScreenCaptureAccess` 不触发弹窗；唯一写操作为调用方指定输出文件）。
- 输出（新路径，未覆盖旧文件）：`.agents/runs/complete-actions-mac-display-preflight-cc-20260930-20260930-175619.json`
- 运行时间：2026-09-30 17:56 本地。

结果：
- 在线屏 2、活动屏 2、休眠屏 0。均 1920×1080，无镜像集，均非内建。
- 主屏 display_id=2（unit 1），副屏 display_id=1（unit 0）。
- `screen_capture_preflight = true`（录屏权限已授予，无需再弹窗）。
- `ax_is_process_trusted = true`（辅助功能权限已授予）。
- 会话字典：`kCGSSessionOnConsoleKey = true`；其余键为 null（脚本运行的 shell 会话非 GUI 控制台会话上下文，属预期，不代表锁屏）。
- **结论：Mac 侧屏幕在线且活动、未休眠，屏幕录制/辅助功能权限均就绪。旧"Mac 已休眠"判断已失效。** 但"显示活动"仍不等同于"已解锁、可点击"——后续 GUI 验收前仍需真实截图作证明。

## 2. Windows (ssh acer-win) 只读元数据

命令均为 Get-* 查询 + `quser`，无创建/启动/删除操作。

| 项目 | 结果 |
|---|---|
| Hostname | `NODE1` |
| OS | Microsoft Windows 11 Pro |
| IPv4 | `100.200.20.168`（WiFi） |
| 交互会话 | explorer.exe pid 8648，SessionId **1** |
| quser | `administrator  console  1  Active  idle none  logon 2026/9/16` |
| 用户 Notepad pid 24332 | **存在**：Notepad，SessionId 1，启动于 2026/9/29 16:00:23（仅查身份，未读文档内容，未触碰） |
| computer-host / fixture 进程 | **无** |
| 监听端口 8399/8398/8400/9000 | **均无监听** |
| Framework64 csc.exe | `C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe`（存在） |
| PowerShell | 5.1.26100.9444（Windows PowerShell，非 pwsh） |

- 交互用户为 administrator，console 会话 1 处于 Active、idle none（近期有人活动）。
- **将来 GUI 验收约束确认**：SSH 登录会进入非交互会话（quser 无 sshd 会话条目），桌面注入必须落在 console 会话 1 → 需交互式 ScheduledTask（在会话 1 上下文运行），不能经 SSH 服务会话直接跑 GUI host。本次未创建任何任务。

## 3. 本机交叉构建工具链（只读检查，未构建未安装）

- `cargo-xwin-xwin 0.23.1`（`~/.cargo/bin/cargo-xwin`）可用。
- `rustup target list --installed` 含 `x86_64-pc-windows-msvc`。
- cargo-xwin 缓存存在（`~/Library/Caches/cargo-xwin`），SDK/CRT 已就绪。

旧构建成功记录（供下一任务复用，未重新构建）：
- 成功：`cargo check --target x86_64-pc-windows-msvc --lib`（见 `.agents/reports/native-input-core-followup-20260929.md`，Finished clean），命令环境为 `docs/remote-connection.md` 记录的 Mac→Windows 方案：`cargo xwin env` + `DYLD_LIBRARY_PATH`（取自 `rustc --print sysroot`）+ `AR_x86_64_pc_windows_msvc=<sysroot>/lib/rustlib/<host-triple>/bin/rust-lld` + `ARFLAGS='-flavor link /lib'`，随后 `cargo build --release --target x86_64-pc-windows-msvc --bins --offline`。
- 成功日志：`.agents/runs/native-win-test-build-20260929.log`（`Finished` test profile，crates/native-input）。
- 失败日志（勿直接复用）：`.agents/runs/native-win-release-20260929.log` — cc-rs 找不到 `llvm-lib`；root workspace 全量 release 构建在 Mac 上有此已知坑，需走上述 AR/ARFLAGS rust-lld 方案。

## 4. 预检清单与遗留环境项

已就绪：
- [x] Mac 双屏在线/活动/未休眠；录屏+辅助功能权限已授予
- [x] Windows 可达，交互会话 1 Active，csc.exe v4.0.30319、PS 5.1 就绪
- [x] 无残留 computer-host/fixture 进程、无 8399 等端口占用
- [x] cargo-xwin + msvc target + SDK 缓存可用，交叉构建命令有成功先例
- [x] 用户 Notepad pid 24332 仍在会话 1 运行（后续验收若涉及文本编辑须避开它，或提前人工确认）

下一步前必须解决/注意：
1. "进程在交互会话" ≠ "桌面可点击/已解锁" — 真实 GUI 验收第一步仍需 host 实际截图验证解锁状态。
2. GUI host 必须经交互式 ScheduledTask 在会话 1 启动；本次未创建。
3. root workspace Mac→Windows 全量 release 构建有 llvm-lib 失败先例，用 docs/remote-connection.md 的 rust-lld 方案。

限制：本次全部只读；未验证 8399 之外的非常规端口；quser 未显示锁屏状态字段（Windows 无只读锁屏查询，需后续截图确认）。
