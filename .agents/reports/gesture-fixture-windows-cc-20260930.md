# Windows gesture fixture — CC independent verification（2026-09-30）

角色：真实 CC 测试执行（Windows only）。工作目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。
Mac 侧仅编排/复制/记录；**没有执行任何 Mac 测试、self-test、export、GUI、启动 Host/renderer**。
用户 Notepad PID24332 未受影响；无进程终止、无源码/构建脚本/测试修改、无 commit/push/worktree、无 subagent。

## 1. Windows 源码独立审查（非 GUI 路径）

- 入口 `windows/MainForm.cs:476-548`：`--self-test` 与 `--export-cases` 在 `Application.Run` 之前返回，
  不构造任何 Form/Control；`--suite` 未知值 fail-closed exit 1（ MainForm.cs:508-510）。
- `windows/GestureSelfTest.cs`：纯 synthetic 回归，不构造控件（全文核对）。
- `windows/GestureNative.cs` 质量核对：
  - WndProc 真实分类 WM_LBUTTONDOWN/UP/DBLCLK（含右键）；一条 DBLCLK 记一个 down，`NativeCount` 为消息分类，
    不伪造 Clicks=3；managed `MouseEventArgs.Clicks` 单独保留，不可用时 `-1` + 来源标注（GestureNative.cs:22-24）。
  - drag held 状态取自 raw wParam `MK_LBUTTON/MK_RBUTTON`（GestureNative.cs:53），非编造样本；
    move 仅在 DragMode 且 down 已按下时记录；up 后 Capture 释放。
  - WM_MOUSEHWHEEL 显式处理，signed delta + 原始 wParam/lParam + GetMessageTime；
    垂直正=向上、水平正=向右分别保留（GestureNative.cs:107-115）。
  - GestureSentence 在 native EDIT 处理**之前**记录 WM 消息，避免 nested tracking 吞事件（GestureNative.cs:84-89）。
- `windows/GestureJudge.cs`：first/final 由 oracle/分析器侧区分，judge 只输出 matched/reason/observed；
  scroll 判定要求实际 offset 位移、错误 panel 变化直接失败、saturate 要求 `AVEnd>=AVMax && av>0`（GestureJudge.cs:116）。
- 分析器关联（scripts/analyze-gesture-gui.py 静态阅读）：按 run/session/case/nonce/时间包络关联，不按全局第 N 配第 N。

### 审查发现的问题（原样记录，未修复）

1. **规范偏差**：README 称“未知参数一律 fail-closed (exit 1)”，但 Windows `Main` 的参数循环
   对无法识别的参数**静默忽略**（MainForm.cs:483-505 无 default 分支）。
2. 次要：`GestureSelfTest.cs` 多处按位置索引取 case（如 `MulticlickCases[8]`=slow_two、`DragCases[9]`=curve），
   与 catalog 顺序耦合而非按 flow 查找；catalog 重排会静默改变 self-test 语义。
3. `GestureCanvas.WndProc` 中 `record` 对 move 用 `wasHeld`（处理前状态），首帧 down 后紧接的 move 依赖
   `ButtonHeld` 已置位——逻辑正确但依赖 DragMode 分支内赋值顺序，建议真实 GUI 阶段以轨迹样本核实。

以上问题不影响本轮非 GUI 验证结论；修复责任在 Codex。

## 2. Windows 主机与环境（只读查询）

- 主机：`NODE1`，OS `Microsoft Windows [Version 10.0.26200.9457]`。
- TEMP：`C:\Users\Administrator\AppData\Local\Temp`。
- Python：**3.12.10**（`C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe`），无需安装。

## 3. 源码快照与复制（SHA-256）

本轮 Windows 目录：**`C:\Temp\computer-gesture-cc-20260930-185517\`**（全新、唯一）。
本地→远端复制后远端哈希逐一核对，**全部一致**：

| 文件 | SHA-256 |
|---|---|
| windows/Cases.cs | 6bf1a8e1b33016f5dddde78a7350550b82914b8753dabd475bc217d40af44909 |
| windows/GestureCases.cs | 456108a7ab12640ca1abbddc7a27fa2f8923af185e926d1ccb5208050bddbebe |
| windows/GestureForm.cs | 6772609f0869615ce9ca400f0089a7a22727f5a4b2bc7fc11f39f1b852434082 |
| windows/GestureJudge.cs | 63920ae7d9667ac8696220c7e61211be89a2d16e24911b13e63adead6ac299b4 |
| windows/GestureNative.cs | 686212d5d0d6622504de5e3d6eaadd40a7990209ce1d4ae374abb6a1c8d256d2 |
| windows/GestureSelfTest.cs | 612d2e8fd5bb8c506429402ba2e89d20273756fbb5a5e1635ce9f777c04e5d0a |
| windows/MainForm.cs | 46d3c3337f564d1c14aab69453cc5a1222cc83e7c3f57238e81bb2f32821d2ff |
| build-windows.ps1 | ebb0dd541a1447f4638d32593ca7f25fecfe5de30cecf1abce2545ab097a4f6a |
| scripts/analyze-gesture-gui.py | f86cf62c373fbb224b5896485259e144a132c9e2a68e501ac9212b046f4c0c04 |
| tests/gesture_gui_analyzer.py | dc6cb2e86315cf0a7382f89ce9faa403789abe9dce19de89310c5251996811f1 |
| tools/check-cases-parity.py | 0e1d7422362ca2cfd0049c46d9d8b186931bbf86c1486de850fafd21be038549 |
| macos/Cases.swift（离线 parity 源） | DDEAC570DCB252ACF6C3870EB6FF176CC9C38D71586322ACBA90AB871C17A41A（远端） |
| macos/GestureCases.swift（离线 parity 源） | C5D3A98E9AE09B7C87C4D1F70A4BF4E3688B56F1847748D3406434A081B8E0D0（远端） |

## 4. 构建（真实 .NET Framework csc）

```
powershell -NoProfile -ExecutionPolicy Bypass -File C:\Temp\computer-gesture-cc-20260930-185517\src\build-windows.ps1 -OutDir C:\Temp\computer-gesture-cc-20260930-185517\build
```

- 退出码 **0**（Framework64 v4.0.30319 csc，`/langversion:5 /codepage:65001`，无 NuGet）。
- 产物：`C:\Temp\computer-gesture-cc-20260930-185517\build\ComputerUseAcceptance.exe`
  SHA-256：`3A5FA228477E176E42D05A614F20AD8E8187A7E6CD6217B84EE9411514F526FF`。
- 备注：第一次构建因 ssh 引号拼接在 `...\windows-build;\` 生成了产物；已删除该临时目录，
  重建于干净 `build\`。无旧产物覆盖。

## 5. --self-test（无 GUI，Windows 执行）

```
C:\Temp\computer-gesture-cc-20260930-185517\build\ComputerUseAcceptance.exe --self-test
```

- 本机退出码 **0**；输出 **SELF-TEST PASSED (89 checks)**，全部 PASS。
- 内容：旧 text/legacy 43 项 + gesture 46 项 synthetic 回归（含 DOWN,UP,DBLCLK,UP,DOWN,UP 三击、
  Clicks=2 不算两次、无 DBLCLK 失败、超时/位移失败、drag 合并/折线/曲线反转/长拖 held、
  HWHEEL 语义、saturate 零位移失败、zero/invalid 无事件）。
- 全部标注 "synthetic; not GUI evidence"。日志：`windows-self-test.log`（runs 目录与远端 evidence 各留一份）。

## 6. --export-cases（无 GUI，Windows 执行）

```
...ComputerUseAcceptance.exe --export-cases C:\Temp\computer-gesture-cc-20260930-185517\evidence\win-cases.json
```

- 退出码 **0**；manifest SHA-256：`8268FE50B70C92CC574981042FF96FA6EC026A874DE993094C9AC172C6AC1118`。
- 结构：`format=acceptance-fixture-cases, version=1, suites=8`（含 coordinator-only 警告）。
- Mac 侧没有执行 export；两平台 runtime 对照见 §8。

## 7. tests/gesture_gui_analyzer.py（Windows Python 3.12.10）

命令：`cd C:\Temp\computer-gesture-cc-20260930-185517 && python tests\gesture_gui_analyzer.py`

- **方法数 = 26**（与规范一致；unittest 输出 `Ran 26 tests`）。
- **第一次失败（保留，`python-analyzer-test.log`）**：exit 1，`FileNotFoundError` —
  测试以 repo 相对路径加载 `scripts/analyze-gesture-gui.py`，初版复制布局未还原 repo 结构。
  此为我方布局问题，仅调整目录布局（未改任何源码/断言）后重跑；原日志保留。
- **第二次失败（保留，`python-analyzer-test-2.log`，本任务核心缺陷）**：exit 1，`Ran 26 tests ... FAILED (errors=24)`。
  首个失败断言/异常：

  ```
  File "...tests\gesture_gui_analyzer.py", line 19, in evidence
      record("input_event", 2, kind="down", ...)
  TypeError: record() got multiple values for argument 'kind'
  ```

  根因（静态核实）：`tests/gesture_gui_analyzer.py` 的共享 helper `record(kind, t, **kw)`
  第一个位置参数名为 `kind`，而所有 `evidence()` 调用又传 `kind="down"` 关键字 → 冲突。
  24/26 测试在 `evidence()` 处 error；2 个测试通过。**这是真实源码缺陷，未修复，交 Codex 修复后需在 Windows 重跑。**

## 8. tools/check-cases-parity.py（Windows Python，离线）

命令：`cd C:\Temp\computer-gesture-cc-20260930-185517 && python tools\check-cases-parity.py evidence\win-cases.json`

- 前两次失败均为我方复制布局缺 `macos/Cases.swift`、`windows/Cases.cs`（`parity-check.log`、`parity-check-2.log` 保留）；
  仅补齐 repo 相对布局，不改脚本。
- **最终 exit 0**（`parity-check-3.log`）：
  `PARITY OK: swift == cs at UTF-16 level; spec satisfied; 63 cases across 8 suites`
  `runtime manifest match: OK (evidence\win-cases.json)`
- 即：Swift/C# 源 catalog UTF-16 一致，Windows runtime 导出 manifest 与源 catalog 匹配（63 条 / 8 suite，含旧 text+legacy）。

## 9. 语义 case 数与 trial 门禁（区分）

- **30 个 semantic gesture case**（multiclick/drag/scroll 各 10）：仅 catalog/self-test/export/parity 层面核实存在与唯一；
  **没有任何一条经过真实 GUI 输入**。
- **每基础动作 ≥10 次首次有效输入 trial**：完全未展开（本轮禁止 GUI）；multiclick-10 非法 count、
  scroll-09/10 zero/invalid 属拒绝/无事件案例，不计入该门禁。

## 10. 保留的失败与 gap 汇总

| 项 | 结果 |
|---|---|
| Windows csc 构建 | exit 0 |
| Windows --self-test | exit 0，89/89 PASS（synthetic，非 GUI 证据） |
| Windows --export-cases | exit 0，manifest 已哈希 |
| analyzer 26 测试 | **FAILED：24 errors（record() kind 冲突）— 首次失败已保留，待 Codex 修复** |
| 离线 parity（Win manifest vs 两源 catalog） | exit 0，PARITY OK 63/8 |
| 真实 GUI 三 suite（multiclick/drag/scroll） | 未执行（禁止）；无任何 GUI 通过率声明 |
| 跨平台 runtime parity（Mac manifest vs Win manifest） | pending，Mac 未 export |
| EDIT 三击真实 selection、WM_MOUSEHWHEEL 路由、wheel 精度与 runtime backend 对照 | 待 GUI 阶段核验 |

## 11. 证据位置

- 本地：`.agents/runs/gesture-fixture-windows-cc-20260930/`
  （windows-self-test.log、windows-export.log、python-analyzer-test.log、python-analyzer-test-2.log、
  parity-check.log、parity-check-2.log、parity-check-3.log、win-cases.json）
- Windows：`C:\Temp\computer-gesture-cc-20260930-185517\{src,build,evidence,tools,tests,scripts,macos,windows}`
  保留原状（build 日志在 powershell 会话输出中，产物哈希见 §4）。
- 源码冻结快照哈希见 §3；未修改任何仓库源文件（本报告与 runs 目录除外）。

## 结论（handoff）

Windows 非 GUI 验证链路：**构建 ✓ / self-test ✓（89 synthetic）/ export ✓ / parity ✓ / analyzer 测试 ✗**。
唯一真实失败是 `tests/gesture_gui_analyzer.py` 的 `record()` 参数冲突（24/26 errors），原始日志与精确异常已保留，交 Codex 修复。
30 个 gesture semantic case 的真实 GUI 验收、≥10 trial/动作展开、跨平台 runtime parity 仍 pending，需协调者另行排期。
