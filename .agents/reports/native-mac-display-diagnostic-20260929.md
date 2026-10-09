# macOS 显示/权限只读诊断 — native-mac-display-probe-20260929

日期：2026-09-29 · 机器：arm64 · macOS 26.3.1 (25D771280a) · probe 运行于 uid 501 (euid 501)
产物：`.agents/runs/native-mac-display-probe-20260929.py`（只读 ctypes probe）、`.agents/runs/native-mac-display-probe-20260929.json`（原始输出）
约束遵守：仅调用非变更系统 API；未请求任何 TCC 权限（仅 preflight）、未截图、未注入输入、未触碰 TCC 数据库、未读凭据。

## 结论（一句话）

**两台显示器的 `CGDisplayIsAsleep` 均为 true，导致 `CGGetActiveDisplayList` 返回 0 台 active display；任何依赖 active display 列表（或 `CGDisplayIsActive`）的"找主显示器"逻辑都会报 `no_display`，而 `CGMainDisplayID` 本身返回正常（非 0）。**

## Probe 结果

| API | 结果 |
|---|---|
| `CGMainDisplayID()` | `2`（非 kCGNullDirectDisplay，主显示器 ID 可解析） |
| `CGGetActiveDisplayList(16, …)` | cgError `0`（成功），**count = 0，display_ids = []** |
| `CGGetOnlineDisplayList(16, …)` | cgError `0`，count = 2，ids = `[2, 1]` |
| `CGPreflightScreenCaptureAccess()` | `true`（屏幕录制权限已授予，且 preflight 未触发任何弹窗） |
| `AXIsProcessTrusted()` | `true`（辅助功能权限已授予，无提示） |
| `CGSessionCopyCurrentDictionary` | `kCGSSessionOnConsoleKey = true`（会话在 console 上）；其余键（LoggedIn/Loginwindow/SecureInputSession/UserID 等）不在字典中，读为 null |

各显示器明细（两个列表并集，按 ID 排序）：

| DisplayID | IsMain | IsActive | IsOnline | IsAsleep | Pixels | Unit |
|---|---|---|---|---|---|---|
| 1 | false | **false** | true | **true** | 1920×1080 | 0 |
| 2 | true | **false** | true | **true** | 1920×1080 | 1 |

两台均非内建（`CGDisplayIsBuiltin = false`）、均不在镜像集内。

## 与 computer-host 启动失败的对应关系

- `CGMainDisplayID()` 返回 2（有效），因此若产品代码只检查 `CGMainDisplayID != kCGNullDirectDisplay`，不应报错。报告的 `no_display: no primary display found` 最可能来自以下两类检查之一（需回看 `src/backend/macos.rs` / `desktop.rs` 中对应实现确认）：
  1. 用 `CGGetActiveDisplayList` 的返回 count 作判定 → 此处 count = 0，直接触发 no_display。
  2. 取主显示器后用 `CGDisplayIsActive(main)` / `CGDisplayIsOnline(main)` 复核 → 此处 `IsActive(2) = false`（`IsOnline(2) = true`）。
- **sandbox 与非 sandbox 行为一致**与此数据吻合：这是显示状态层面的问题（显示器 asleep），不是 App Sandbox 权限/TCC 差异 —— preflight 与 AX 均已通过，权限不是根因。
- 所有 display 均 asleep 且会话在 console 上（`kCGSSessionOnConsoleKey = true`），符合"系统显示器睡眠/无物理点亮（headless 式使用或屏幕睡眠）"状态。此状态下 macOS 仍保留 online display，但 active display 列表为空 —— 这是 CoreGraphics 的标准行为，非系统异常。
- Swift fixture 与 backdrop 能启动，说明它们大概率只依赖 `CGMainDisplayID` / online list，而未要求 active display。

## 建议的后续核实方向（本次未执行，均超出只读范围）

1. 回查产品判定逻辑：grep `CGGetActiveDisplayList` / `CGDisplayIsActive` / `CGMainDisplayID` 在 `src/backend/macos.rs`、`src/backend/desktop.rs` 的使用点，确认 no_display 分支由哪条触发。
2. 若需在显示器睡眠时也可运行：判定应改用 `CGGetOnlineDisplayList`（或 `CGMainDisplayID` + `CGDisplayIsOnline`），online list 在 asleep 状态下仍返回 2 台。
3. 如需验证 asleep→active 转换，可由用户手动唤醒屏幕后重跑同一 probe（预期 `IsActive` 变 true、active count 变 1+）；probe 脚本本身不唤醒屏幕。

## 方法说明与可信度

- 所有 API 签名按头文件核实：`CGError = int32`、`CGDirectDisplayID = uint32`、boolean 类查询 restype 为 `unsigned char`、像素尺寸 restype 为 `size_t`；`CGPreflightScreenCaptureAccess` 为官方 preflight（按定义不触发 TCC 弹窗，结果 true 即已授权）。
- `AXIsProcessTrusted()`（无参数版本）同样不弹窗；弹窗的是带 Options 的变体，本 probe 未调用。
- 会话锁定状态：`CGSessionCopyCurrentDictionary` 为只读快照，仅读取列出的非凭据键；未读取任何凭据字段。
- probe 全程 `errors: []`，无 API 调用失败；JSON 为逐字段原始返回值，未做推断填充。
