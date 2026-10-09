# layered-gui-helpers-20260930 — 测试 helper 生命周期/副屏遮挡修复

## 范围

仅新增以下文件（旧 helper 与旧失败证据全部保留，未改动）：

- `.agents/runs/layered-gui-helpers-20260930/macos-backdrop.swift`
- `.agents/runs/layered-gui-helpers-20260930/macos-backdrop`（swiftc -O 编译产物，89632 字节）
- `.agents/runs/layered-gui-helpers-20260930/gui-neutral-desktop-20260930.ps1`
- `.agents/runs/layered-gui-helpers-20260930/helper-selftest.py`

未写其他文件；未启动 GUI；未委派；未 commit/push。按任务约定不做 topmost/窗口浮动扩展、不改用户屏幕。

## 修复内容

### Mac（基于 `native-gui-helpers-20260929/macos-backdrop.swift` 拷贝修改）

1. **`--max-minutes` 生命周期**：默认 25，合法范围 1..30（含）。越界（0、31、-1、非整数）在参数解析阶段以 usage 退出码 2 **拒绝**，不再 clamp。timer 严格按参数值启动；fixture watchdog（5s 轮询，fixture 消失即自退）与 SIGTERM 自退处理保留。
2. **副屏 origin 修复**：旧 helper `NSWindow(contentRect: screen.frame, screen:)` 把已是全局坐标的 screen.frame 再按目标屏幕局部坐标解释，导致 secondary（frame origin x=1920）实际窗口落到 x=3840，与 `.agents/runs/layered-mac-baseline-d-visibility-probe-20260930.json` 观测一致。新实现改为局部 content rect `NSRect(origin: .zero, size: screen.frame.size)` 创建，再 `w.setFrame(screen.frame, display: true)` 绝对定位到该屏 frame；secondary 覆盖保持 x=1920。
3. **单次激活**：fixture activate 仍只在 `applicationDidFinishLaunching` 执行一次（modern + deprecated fallback 各一次），timer 内无任何激活/抢焦点；不点击/按键、不 AX/AppleScript、不改系统权限。

### Windows（基于 `gui-neutral-desktop-20260928.ps1` 拷贝修改）

1. **`-MaxMinutes` 参数**：默认 25，范围 1..30，越界 `throw`（不 clamp）。WinForms lifetime timer `Interval = MaxMinutes * 60 * 1000`，随 Shown 启动，超时 `$form.Close()` 结束 `Application.Run`。Windows 侧额外安全垫：coordinator 注册的 Interactive ScheduledTask 仍应保留 30min expiry（脚本内未也不应覆盖）。
2. **fixture watchdog**：5s 轮询 `Get-Process -Id FixturePid`，进程消失即关表单自退。
3. **原契约不变**：仍只覆盖 primary screen；backdrop `TopMost=$false`；Shown 时一次性把 fixture 提前（SetWindowPos/ShowWindow/SetForegroundWindow，失败即 throw 不绕过前台限制）；不点击/按键、不关闭/最小化任何窗口。

## 验证（helper-selftest.py，纯离线，不创建真实 window / 不 GUI / 不信号 / 不 SSH）

先 fail 后 pass 设计：

- **fail-first（旧 helper）**：`--max-minutes 31` 被 clamp 后继续走流程（exit 3，非 usage 2），证明旧实现确有此缺陷。
- **pass（新 helper，实际执行编译产物）**：
  - 拒绝：0 / 31 / -1 / `--max-minutes=31` / junk / 999999 → 全部 exit 2。
  - 接受：1 / 25 / 30 / 默认（无 flag）→ 通过解析，进入身份检查后因无该 pid 以 exit 3 退出（未创建任何窗口）。
  - 缺参 → usage exit 2。
- **几何回归（源码级静态断言，不建窗口）**：局部 origin rect 存在、`setFrame(screen.frame)` 绝对定位存在、旧 `contentRect: screen.frame` 模式已移除；probe JSON（secondary origin 1920）存在性检查通过。
- **Windows ps1**：静态断言 `-MaxMinutes` 默认/throw 校验/timer 使用参数/watchdog/仅 primary/非 topmost 全部通过；复用 `native-gui-helpers-20260929/ps-structural-check.py` tokenizer 结构检查 → `gui-neutral-desktop-20260930.ps1: OK`。

selftest 最终结果：`SELFTEST PASS: all checks passed`（30 项）。

## 限制（不得宣称真桌面已验证）

- 本机无 pwsh，Windows ps1 仅做 tokenizer 结构检查与源码静态断言，**未做真实 PowerShell 语法/运行验证**。
- Mac 几何修复为源码级静态断言 + probe 坐标对照，**未在真实桌面创建 backdrop 窗口截图验证**；真实两端 baseline 验证（25min 全程中性、secondary 实拍）由协调者拍图确认。
- Mac 编译产物为本机 (arm64 macOS) 编译，仅本机使用；Windows helper 无需编译。

## 20260930 追加：runner 控制台隐藏 launcher（gui-runner-launch-20260930.ps1）

### 背景与修复

观察（`.agents/runs/layered-win-known-input-live-frame-20260930.png`）：Windows 25min background 运行中，agent powershell 经 Interactive ScheduledTask 以 `powershell.exe -File` 启动后新建了 terminal 窗口，盖在 normal backdrop 上——说明之前的遮挡不能单归为 5min timer bug，我们自己新建的 runner 控制台本身就产生新窗口。原则：只隐藏**我们自己新建的 runner 控制台**，不全局隐藏其他 app、不动用户窗口；fixture.exe 不隐藏，仍由 fixturestart 独立启动。

### 唯一新文件

`.agents/runs/layered-gui-helpers-20260930/gui-runner-launch-20260930.ps1`（复制自 `native-gui-helpers-20260929/gui-runner-launch-20260929.ps1`，旧文件未改动、正在运行的 known-input agent 未被打断）。

### 变更内容（diff 实证）

唯一功能性差异一行：

```
$argList = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File $quoted"
```

其余全部不变：Interactive principal / Session1（拒绝 Session 0 与 0xFFFFFFFF）、RunLevel Limited、直接 `-File` 执行、路径/参数 fail-closed 验证（控制字符与单双引号拒绝）、GUID task 名拒绝覆盖、严格 process identity（name/ExecutablePath/session/CreationDate/CommandLine.Contains(argList)）、record 全字段、ExpiryMinutes 边界。**同一最终 argList** 用于 task action、身份匹配 Contains、record 的 `action_arguments`/`command_line`，因此 `gui-runner-stop-20260929.ps1` 的严格核验（tick 与 CommandLine EXACT 相等、action_arguments 相等）契约不变，无需宽松匹配。

### 非 GUI 验证

- `ps-structural-check.py`（旧 tokenizer，复用）：`gui-runner-launch-20260930.ps1: OK`（引号/括号平衡、here-string 终止）。
- diff 旧版：仅文件名头注释 + delta 说明注释 + argList 一行；`-WindowStyle` 在旧版 argList 中不存在。
- grep 断言：`WindowStyle Hidden` 仅出现于 argList 与注释；Interactive/Limited/refuse-overwrite 逻辑原样保留。

### 限制（不宣称已验收）

- 本机无 pwsh，未做真实 PowerShell 语法/运行验证。
- **`-WindowStyle Hidden` 在 Windows Terminal 作为默认宿主时是否真的无可见窗口，需协调者下一轮实拍确认**；Windows Terminal 可能忽略 console `-WindowStyle`（conhost 与 WindowsTerminal 行为不同）。本报告不宣称真桌面已验证。本 helper 只用于后续 runs。
