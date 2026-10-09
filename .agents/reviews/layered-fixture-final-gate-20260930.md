# Layered Fixture 最终门禁增量复核 — 2026-09-30

只读增量复核（上轮审查 `.agents/reviews/layered-fixture-20260930.md` 的后续）。未改代码、未 GUI、未 commit。产物路径：`.agents/runs/layered-fixture-build-20260930-c/macos/ComputerUseAcceptance.app`（未使用旧 `acceptance-fixture/build/macos`）。

## 实测结果

- **`--self-test`（-c 新产物，非 GUI）：46/46 PASS，exit 0**。新增 6 项 placement 检查全过：
  - `placement-main-not-first` frame 510.0,144.0
  - `placement-main-not-first-neg-origin` frame 510.0,144.0
  - `placement-secondary-horizontal` frame 2430.0,144.0（复现旧 bug 的 x=2430 次屏坐标）
  - `placement-vertical-neg-y` frame 510.0,-936.0
  - `placement-missing-main-fails` / `placement-too-small-fails` → 正确返回 nil
- `check-cases-parity.py`：PARITY OK，33 cases / 5 suites（当前树仍一致）。

## Mac 门禁核验（全部通过）

1. **placement 实现**：`Cases.swift:143-178` pure `ScreenPlacement.chooseFrame` — 按 `displayID == mainDisplayID` 匹配（Cases.swift:168），尺寸不足返回 nil（:169），AppKit 全局坐标居中（:170-172），`fullyInside` helper 完整判定（:175-177）。无静默 fallback、无 Y 轴手工换算。
2. **调用点**：`main.swift:259-274` `placeWindowOnMainDisplay()` 从 `NSScreen.screens` + `deviceDescription["NSScreenNumber"]` 构建 ScreenInfo（:261-264），匹配失败走 `Config.fail` exit 1（:268-272，带各屏诊断信息，诚实失败）；`buildUI` 在 :299 调用（早于任何 addSubview/show）。测试矩阵覆盖主屏非 first、负 origin、横排/竖排次屏、too-small、missing（Cases.swift:289-325），与实测 6 项 PASS 对应。
3. **中性输入域**：`main.swift:332-336` 已显式关闭 `isAutomaticQuoteSubstitutionEnabled / isAutomaticDashSubstitutionEnabled / isAutomaticTextReplacementEnabled / isAutomaticSpellingCorrectionEnabled / isContinuousSpellCheckingEnabled`。注释明示 oracle 原样记录。
4. **UTF-16 oracle 未回归**：`main.swift:424-425` 仍 strict `utf16ExactMatch`（无 Trim/无 normalize）；`:447` 仍输出 `actual_utf16_hex`/`expected_utf16_hex`；`checkText` 直接取 `textView.string`，ActualText 未被暗中 normalize。

## Windows 门禁（待 writer 完成 + coord 真机补证）

- `MainForm.cs:289-290` 确认已设 `AcceptsTab = true; AcceptsReturn = true`（上轮 A 项已修）。
- ⚠️ **当前树 transient 状态**：`acceptance-fixture/windows/Cases.cs`（656 行）存在**两个 `static class SelfTest` 定义（:221 与 :415）**及成对重复段（如 :239/:433、:298/:492），writer 正在修自检谓词/代理数/标点范围，属编辑中重复定义，会触发 CS0101。不据此判 Windows 失败——但 coord 最终重建前 writer 需收敛为单一定义。
- **不宣称 Windows 自测通过**：上轮 csc out-fixed 编译成功但纯 self-test 3 failure 的修复未在本轮复核范围；最终以 coord 真实 csc 重建 + self-test + export-parity 为准。

## 门禁结论

| 门禁 | 状态 |
|---|---|
| Mac 纯 self-test（46 项，含 placement/替换禁用前置条件） | ✅ 通过，可进入受控 GUI |
| Mac 窗口真实可见性（主屏居中） | ⏳ 待 coord 截图验证（本审查未启动 GUI） |
| Windows 编译 + self-test + export parity | ⏳ 待 writer 收敛 Cases.cs 后由 coord 真实 csc 补证 |
| Windows GUI | ⏳ 依赖上两项 |

未因 Windows 等待虚构 Mac 阻塞；static gate 通过 ≠ GUI 验收完成，截图证据仍缺。
