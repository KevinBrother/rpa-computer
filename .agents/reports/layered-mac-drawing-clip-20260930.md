# layered-mac-drawing-clip-20260930 — Mac fixture header 空白最小修复（非 GUI）

Worker：Claude Code（实现），范围严格限定于任务文档：`acceptance-fixture/macos/main.swift`、`acceptance-fixture/README.md`、本报告。未操作桌面、未启动真实窗口、未 commit。

## 结论

- 完成最小修复 + 离屏回归（fail-first 演示），self-test 49/49 通过（exit 0）。
- catalog parity：macOS 新导出与 Windows neutral manifest 逐键一致（`PARITY OK`）。
- **不宣称 GUI 已修复**；trialLabel/nonceLabel/instr 是否恢复显示须由协调者拍图验证。

## 改动（main.swift）

1. `ShapesView.draw`：白色填充由 `dirtyRect.fill()` 改为 `bounds.intersection(dirtyRect).fill()`；图形绘制前对 `cgContext` 显式 `clip(to: bounds)`（saveGState/restoreGState 包裹）。
2. `ShapesView` 两个 init 中显式 `clipsToBounds = true`（LSMinimumSystemVersion 11.0，10.14+ API 可直接用）。
3. header：suite 模式 trialLabel 改为 `Trial i/N — <caseID> — suite: <suite>`（legacy 保持 `Trial N`）。UTF-16 校验逻辑零改动。
4. 主屏定位、`--coordinator-raise`、READY 日志、`--self-test`/`--export-cases` 路径全部保留。

## 假设验证（诚实陈述）

- 离屏回归证明：在未预 clip 的上下文中，旧式 `dirtyRect.fill()`（oversized dirtyRect）确实会把白色画到自身 bounds 之外（`draw-clip-harness-detects-unclipped-escape` PASS，fill (-200,-200,1400,700) 越界）——即该 bug 类别在 layer-backed 渲染下真实存在，harness 能捕捉它。
- 但**离屏无法复现真机上 trialLabel 空白**（frames 本不重叠，离屏渲染两版都正常显示 label）。修复是防御性的（clip + clipsToBounds 双保险）；若真机拍图后 header 仍空白，则另有原因（label 颜色/布局/z-order），届时再依证据排查，不顺手重做 UI。

## 新增 self-test（3 项，共 49）

- `draw-clip-harness-detects-unclipped-escape`：未 clip 的旧式 fill 越界可被检出（harness 灵敏度）。
- `draw-fill-within-bounds`：修复后白色 fill 严格限于自身 bounds。
- `draw-shape-clip-within-bounds`：图形绘制期间的有效 clip ⊆ bounds。

实现方式：`NSBitmapImageRep` + `NSGraphicsContext(bitmapImageRep:)` 离屏上下文，外层 clip 故意大于 bounds 模拟不预 clip 的渲染；`ShapesView` 增加 recordDrawing 测试钩子记录实际 fill rect 与 clip bounding box。无真实窗口、无截图、无输入。

## 产物（均在 .agents/runs/layered-fixture-build-20260930-e/macos/）

- `ComputerUseAcceptance.app`（binary sha256 `3457b4bd…22a760`）
- `cases-manifest-macos-20260930.json`（coordinator-only，含答案，不得进 Agent 发布目录）

## 验证命令与结果

- 构建：`./acceptance-fixture/build-macos.sh .agents/runs/layered-fixture-build-20260930-e/macos` → built 成功
- `--self-test` → `SELF-TEST PASSED (49 checks)`，exit 0
- `--export-cases …` + JSON diff vs `.agents/runs/layered-cases-manifest-windows-neutral-20260930.json` → `PARITY OK`

## 补报（任务文档要求的顺手补报）

先前 backdrop probe 的瞬时 stacking index 落后不能单独否定置前：晚一点的 MCP 截图（baseline-d preflight frame）已能看到 fixture window 在主屏。当前真正待解问题是 header 三个 label 空白；本次仅收紧自有 view 绘制边界并让 header 显式带 caseID，未改用户窗口/系统配置，未扩展功能。

## 下一步（协调者）

用新 -d binary（`--coordinator-raise`）真机启动 + SIGUSR1 置前后拍图，核对 header（caseID/nonce/instr）与既有元素；若仍空白，回传截图与 label 相关证据。
