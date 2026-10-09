# 独立复核：Mac first-mouse 最小修复（2026-09-30）

复核者：独立复核 Agent。只读源码/证据；未改源码/测试、未 GUI/SSH、未发信号、未委派（任务文件要求"不委派"，故未执行 resume-worker 启动命令，由本会话直接复核）、未 commit。未触碰正在运行的 -e 产物与 -f 发布产物；所有重编译均输出到 /tmp。

## 结论

新增 first-mouse 最小修复**真实、最小、无副作用，无阻塞缺陷**，予以通过保留。但与 worker 报告口径一致：**GUI 是否真修复未验证**（nextGLM 实机验收未跑），"window inactive 是首击 no-op root cause"仍是假设而非已证明。

## 逐项复核（任务点名）

### 1. acceptsFirstMouse override 与 SDK 签名
- `ShapesView` 新增 `override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }`（main.swift:234）。能编译即证明本机 SDK 签名为 `NSEvent?`，与报告"SDK 26.5 实为 NSEvent?，非旧文档 NSWindow?"一致。
- 仅作用于 fixture 自有 canvas view，等价标准可点击控件行为；无双击重试、无 level 提升、未触碰 Host/native 输入。

### 2. key/focus 日志：仅自有 window，无外部副作用
- `windowDidBecomeKey` / `windowDidResignKey` 只记录自身 window 的 `{key, active}` 布尔元数据（main.swift:320-326），无答案内容、无其他 app/窗口访问。
- `coordinator_raise` 事件补 `key`/`active` 字段（main.swift:307-310），此前逻辑不变。
- mouseDown 命中检测逻辑未改；无外部窗口操作、无输入合成。

### 3. 真实 API pure 回归（fail-first 1/50 → 50/50）
- 源码自检为真实调用 `firstMouseTarget.acceptsFirstMouse(for: nil)`（main.swift:620-624），非验证常量的伪测试。
- fail-first 证据在 worker 转录中独立核对（.agents/runs/layered-mac-firstmouse-worker-20260930.jsonl）：中间构建（binary sha256 `96e22bfd…`）self-test 输出 `FAIL: first-mouse-accepted — acceptsFirstMouse(for: nil) = false`；最终构建（`5823306b…`）50/50 PASS。
- 审查者独立重编译到 /tmp 复跑：`SELF-TEST PASSED (50 checks)`，exit 0，含 `first-mouse-accepted = true`。
- 注：审查者 /tmp 直接 swiftc 编译的 binary 与 -f 产物字节不同（build-macos.sh 打包路径差异，预期内），但 `--export-cases` 产物与 -f 的 `cases-manifest-macos-20260930-f.json` **逐字段完全一致**（PARITY 独立复核通过）。

### 4. -f SHA 与导出可复核
- 实测 -f binary sha256 = `5823306b714e00919157a70a8a53389dd57f2ef399d4a6c07398e45d63928099`，与报告一致。
- -e 目录（.agents/runs/layered-fixture-build-20260930-e/，mtime 11:28）在修复期间未被改动；-e baseline 首击 miss 证据未被覆盖。

### 5. 旧保障保留（自检独立复跑确认）
strictUTF16（utf16-exact-6 项）、主屏定位 6 项、绘制裁剪 3 项、case 计数/内容检查、READY/会话日志、export 警告——全部仍在且 PASS（50/50）。

## 必须保留的口径（与任务一致）

1. -e 首次 case click 坐标正确但无 HIT、后续单击均 HIT：**window inactive 是假设，未证明**。当时未记录 keyWindow 状态；本次补的 `window_key`/`coordinator_raise` key/active 元数据正是为下一 run 关联验证。实机验证前不得写成"root cause 已确认"。
2. nextGLM 真机验收未跑，-f binary 尚未经 GUI 验证。
3. `acceptsFirstMouse(for:) -> true` 只覆盖本 fixture 自有 view，**不代表所有 Mac App 的 first-click 都能激活控件**（NSButton 等本来就接受；自绘 view 默认不接受）。

## 非阻塞观察

- `acceptsFirstMouse(for:)` 的 `event` 参数未使用（有意 opt-in 全部事件类型，行为正确）。
- fail-first 的中间 binary（96e2…）未保留在 runs 目录（仅转录证据）；不影响可复核性，因 fail-first 逻辑由 /tmp 重编译同路径可复现（先去掉 override 即 1/50）。

## 门禁状态

- macOS 纯回归：50/50 独立复核通过；SHA/导出可复核。
- GUI live-gate：**未闭合**，待协调者用 -f binary 实机验证并结合 key/active 日志核对首击行为。
