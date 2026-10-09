# layered-mac-firstmouse-20260930 — Mac fixture first-mouse click-through 最小修复（非 GUI）

Worker：Claude Code（实现）。写入范围：`acceptance-fixture/macos/main.swift`、`acceptance-fixture/README.md`、本报告。未 GUI/SSH/信号/委派/commit；未触碰正在运行的 -e 产物（-e baseline 完整保留首次 miss，不做重试覆盖）。

## 结论

- TDD fail-first 完成：`ShapesView` override `acceptsFirstMouse(for:) -> true`，self-test 50/50（exit 0），export parity OK。
- **不宣称 GUI 已修复**；真实 GLM 后续验证由协调者完成。本修复仅针对 fixture 自身 click-through，不代表所有 Mac 应用 first-click 可直接激活控件。

## 证据回顾（诚实）

- -e baseline：case01 首次 green click 坐标在绿圆内但无 HIT/WRONG、无 oracle hit；case02..05 同类单击均 HIT；case01 text check 成功。排除坐标错与全局键鼠失效。
- 原因假设：`NSView.acceptsFirstMouse(for:)` 官方默认 false——inactive window 的第一次 mouseDown 只激活 window 不传给 view（NSButton 等标准控件可 click-through）。当时未记录 keyWindow 状态，**inactive 假设不能当已证实**，故本次同时补了 focus 状态日志供下一 run 关联。

## 改动（main.swift）

1. `ShapesView` 新增 `override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }`（本机 SDK 26.5 签名为 `NSEvent?`，非旧文档的 `NSWindow?`）。不双击重试、不改 Host/native 输入、不强制激活其他应用、不改 floating level。
2. focus 证据日志（仅自有 window，布尔元数据，无答案）：
   - `windowDidBecomeKey` / `windowDidResignKey` → evidence `window_key` 事件 `{key, active}`；
   - `coordinator_raise` 事件新增 `key`（`w.isKeyWindow`）/ `active`（`NSApp.isActive`）。
3. 未改绘制 clip 修复（-d/-f 之上无回退）、未改 UTF-16 校验、主屏定位、READY 日志、export/self-test 路径。

## TDD 过程

1. 先加真实 API 回归 `first-mouse-accepted`（调用 `ShapesView.acceptsFirstMouse(for: nil)`，非验证常量的伪测试）→ 修复前构建运行：`FAIL: first-mouse-accepted — acceptsFirstMouse(for: nil) = false`（1/50 failed，exit 1）。中间踩坑：override 签名按旧文档写 `NSWindow?` 编译失败，SDK interface 实为 `NSEvent?`，已修正。
2. 最小 override 后重构建 → `PASS: first-mouse-accepted = true`，`SELF-TEST PASSED (50 checks)`，exit 0。

## 验证命令与结果

- 构建 -f：`./acceptance-fixture/build-macos.sh .agents/runs/layered-fixture-build-20260930-f/macos` → built
- binary sha256：`5823306b714e00919157a70a8a53389dd57f2ef399d4a6c07398e45d63928099`
- `--self-test` → 50/50 PASS，exit 0
- `--export-cases …/cases-manifest-macos-20260930-f.json` + JSON diff vs Windows neutral manifest → `PARITY OK`

## 产物（唯一新目录 .agents/runs/layered-fixture-build-20260930-f/macos/）

- `ComputerUseAcceptance.app`（上述 hash）
- `cases-manifest-macos-20260930-f.json`（coordinator-only，含答案，不得进 Agent 发布目录）

## 状态口径

- -e 本轮首次 no-hit 与后续成功保持原样，不覆盖、不给正在运行 agent 重试，不称全矩阵结束。
- 下一步（协调者）：用 -f binary 真机跑 baseline，结合 `window_key`/`coordinator_raise` 的 key/active 元数据核对首次单击是否 HIT，验证 click-through 修复。
