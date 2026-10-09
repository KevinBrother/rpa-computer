# 独立审查：layered fixture 绘制裁剪 + 新 GUI helpers livegate（2026-09-30）

审查者：独立审查 Agent（只读；未改任何产品源码/测试/脚本；未 GUI、未 delegate、未 commit）。
对象：两个独立 scope 的最小修复（见 `.agents/tasks/layered-fixture-helper-livegate-review-20260930.md`）。

## 结论

两个 scope 的最小修复本身**无阻塞缺陷**，可以保留。但两个 live-gate 均未真正闭合：
Mac 实拍未进行（display inactive/no_display），Windows 仅到达 coord/Session 证据层、未宣称完整 GUI 通过。**不宣布全套完成**。

## Scope 1：Mac main.swift ShapesView 裁剪 + 3 项 offscreen 自检

### 已独立验证

1. **修复内容真实存在且正确**（acceptance-fixture/macos/main.swift）：
   - `ShapesView` 两个构造器都设 `clipsToBounds = true`（main.swift:166-174）。
   - `draw()` 中白色底填充严格限制为 `bounds.intersection(dirtyRect)`（main.swift:208），形状绘制外层再显式 `cgContext.clip(to: bounds)`（main.swift:215）并与外层 clip 相交。
2. **自检独立复跑通过**：审查者自行 `swiftc` 重编译 main.swift+Cases.swift 到 /tmp 后执行 `--self-test`，**49 PASS / 0 FAIL**（含 3 项新绘制自检）：
   - `draw-clip-harness-detects-unclipped-escape`：确实先证明 offscreen 装置能捕获未裁剪逃逸（fail-first），legacy 填充 (-200,-200,1400x700) 被检出越界；
   - `draw-fill-within-bounds`：fill (0,0,860,224) 在界内；
   - `draw-shape-clip-within-bounds`：clip (0,0,860,224) 在界内。
3. **header 宽度 500 是否截断（任务点名项）——不截断**。字体测量（boldSystemFont 18，`NSString.size`，非真实 window）：全部 4 suite × 30 个可能 header 组合中最宽为
   `Trial 10/30 — punctuation-10 — suite: punctuation` → **428.37pt < 500pt**，余量约 72pt。suite/case 位置、ID、target 均可读。nonce（860pt 居中，monospaced 30）与 TARGET 行（860pt）也不受影响。header 保留 caseID（main.swift:444）且证据日志带 case_id/case_index/case_total。
4. **旧保障全部保留**：主屏定位 `placeWindowOnMainDisplay`（找不到匹配 CGMainDisplayID 的屏则 exit 1 拒绝隐形启动）、`--coordinator-raise` SIGUSR1 单窗口 raise（仅触碰自身窗口，无 AX/AppleScript/输入注入/层级提升）、文本严格 UTF-16 比对（无归一化）、自动化文本改写全部关闭。

### 必须保留的 caveat（与任务要求一致）

实拍因 display inactive/no_display **未进行**。上述 offscreen 自检只证明：(a) 该测试装置能检出此类逃逸；(b) 修复后的 view 不逃逸。它**不能**确认真机上 header 被涂白的 root cause 就是未裁剪填充。报告中不得写成"真机 root cause 已确认"。

## Scope 2：新独立 helpers（.agents/runs/layered-gui-helpers-20260930/）

### 已独立验证

1. **macos-backdrop（swift）**：
   - duration 默认 25；`boundedMinutes` 对 0/31/-1/junk/999999 **一律 exit 2 拒绝、从不 clamp**（helper-selftest.py 实跑验证通过，含对旧 helper clamp 行为的 fail-first 对照）。
   - fixture 身份验证在任何 UI 创建之前完成（pid 存活 → executableURL 路径标准化/符号链接解析比对，不匹配 exit 5），无 input、无剪贴板、无网络，只创建/调度自己的 backdrop 窗口。
   - 副屏双原点最小修复正确：本地 contentRect（origin .zero）+ `setFrame(screen.frame, display: true)` 绝对放置；旧 `NSWindow(contentRect: screen.frame` 模式已移除（源码静态断言 + 自检均通过）。secondary x=1920 不再翻倍为 3840。
   - **不重复激活**：fixtureApp.activate 仅在 applicationDidFinishLaunching 调用一次（现代 API + deprecated 兜底各一次），timer 内无 activate（自检断言）。25 分钟单发退出 timer + 5s fixture 进程 watchdog 均保留；SIGTERM 处理保留。
2. **gui-neutral-desktop-20260930.ps1**：
   - `-MaxMinutes` 默认 25，越界 `throw` 拒绝不 clamp（1..30）；lifetime timer `MaxMinutes*60*1000`，5s watchdog 保留，均在 Shown 中先启动 timers 再做 raise（顺序正确：raise 阶段抛错也不破坏生命周期上界）。
   - fail-closed 检查齐全：自身 SessionId>0（拒绝 SSH 服务会话）、fixture 路径与会话匹配、无主窗口句柄即 throw、`SetForegroundWindow` 失败即 throw 不绕过前台限制。
   - 仅 PrimaryScreen（契约未变）、backdrop TopMost=false、无点击/按键合成。
   - 结构 tokenizer 检查（ps-structural-check.py）通过；helper-selftest.py 全部 PASS（审查者实跑）。
3. **25min 覆盖 1200s+setup**：25min=1500s，对 1200s 任务留 300s 余量，且外层仍有 coordinator 的 30 分钟 ScheduledTask 安全过期。满足。
4. **旧 helpers 未改动**：新旧分目录，旧 native-gui-helpers-20260929 原样保留（并被 selftest 用作 fail-first 对照），旧失败记录未被覆盖。

### 无法独立验证（如实声明）

- Windowscoord 的 `PSParser.ParseFile` 成功与"新 helper + agent 进程在 Session 1"：本审查主机无 pwsh/Windows，只能采信为已提交的运行证据，不能本地复核。与任务声明一致：**这不是完整 GUI 已通过的结论**，只是 coord 层证据。
- Mac helper 的实机窗口行为同样未实拍验证。

### 非阻塞建议（不构成重构要求）

1. ps1 把 fixture 窗口 `HWND_TOPMOST` 后从未恢复为 `HWND_NOTOPMOST`：helper 退出后 fixture 仍是 topmost 残留状态。建议 coordinator 在收尾时显式恢复，或至少在报告里记录该残留（测试会话内影响有限）。
2. macos-backdrop 在 macOS 14+ 上会先后执行现代 `activate()` 与 deprecated `activate(options:)` 两次调用（仅启动时一次，非重复激活），行为无害，可留待后续顺手清理。
3. `helper-selftest.py` 的 ps1 检查是静态/结构级的，与 PSParser 证据互补而非替代——两者已分别覆盖，无需再补。

## 交付门禁状态

- macOS：`--self-test` 49/49（独立复编译复跑）；实拍 live-gate **未闭合**（display inactive/no_display）。
- Windows：coord/Session1 证据在案；完整 GUI live-gate **未闭合**（任务本身也未宣称通过）。
- 旧失败记录保留、未用 stub 冒充真机结果；完成矩阵未删项。

结论：两处最小修复予以通过保留；live-gate 待实机验证，不得据此宣布 layered 验收完成。
