# Windows focus StageB — CC independent pure verification — 2026-09-30

- 执行身份：CC（真实 Windows 机器 acer-win，经既有 SSH；**Session 0 纯 console**，无 GUI/原生输入/截图/start-windows/Host/renderer/网络监听；未运行 Mac/Linux 程序，Swift 仅作 parity 只读数据）。实际模型：glm-5.3-flash。
- 结论：**序列全绿（pure console 意义）。** 构建 exit 0；7 个 runner 步骤 native exit 全部 **0**、无超时；self-test 实际 **178 PASS / 0 FAIL**（StageA 原 143 逐行保留且全部仍 PASS，StageA 唯一 FAIL 行已按预期转 PASS）；旧 63 / B2 20 / focus 10 导出经独立 JSON 核对一致；focus-analyzer **35 tests OK**、basic-analyzer 回归 **30 tests OK**；legacy63 parity **PARITY OK**。测试后源码哈希复查 **31/31 无变化**。**GUI 一律未验证**：focus09 全局策略仍 unknown/needs_supervisor，focus10 无法自证无输入，10 例 GUI 与 10 次有效 trial 全部 pending，无任何审批绕过。
- 未修改任何源码/断言/构建脚本/全局设置/安全配置；未 commit/push/worktree；未触碰 B2/StageA 旧 snapshot 或 bin；Notepad24332 未触碰。frozen owner 已停止，本轮后未自行扩展。

## 1. 冻结源码复制与哈希

按 `.agents/reports/windows-focus-fixture-sol-20260930-final-compile-214952/source-sha256.txt` 全部 **31 文件** + coordinator 已批准 runner 2 文件（`scripts/windows-test-runner.ps1`、`scripts/windows-test-runner/Runner.cs`），共 **33 文件**经 **tar over ssh**（经确认 Windows 侧 OpenSSH 自带 bsdtar）复制到**新唯一 TEMP**，保留相对路径布局：

- 远端 staging（保留未删）：`C:\Users\Administrator\AppData\Local\Temp\computer-focus-green-cc-0c0f9bcb66974625b76e19d4274b5e54`
- 本地 shasum 预核对：31/31 与 manifest 一致。
- 远端 `Get-FileHash` 对 manifest 逐一核对（`remote-hash-verify.txt`，已归档）：**31 OK / 0 MISMATCH**；RUNNER 两文件 `ad2a607c…` / `aced18e6…` 与冻结表一致。
- B2/StageA/旧 GUI exe 从未覆盖：本轮全部产物在新 TEMP 与新 `greenout` 子目录。

## 2. 构建（frozen build-windows.ps1）

- csc（Framework64 v4.0.30319）经冻结 `acceptance-fixture/build-windows.ps1 -OutDir <staging>\greenout\buildout`：**build exit = 0**（`build-exit.txt`，真实 `$LASTEXITCODE`），build stderr 空。
- 新 exe：`greenout\buildout\ComputerUseAcceptance.exe`，SHA-256 `0cb2a66bae9fd90666788a3341a580aad024d5e16e23825f37ae57fe81d13035`（本地归档重算一致）。

## 3. 执行序列（单个 on-disk `master.ps1`，approved JSON runner，每步 timeoutSeconds=120，evidence 父目录预建、leaf 由 runner 原子保留）

| 步骤 | 程序/argv | native exit | timed_out | harness exit | stdout/stderr bytes | 结果 |
|---|---|---|---|---|---|---|
| selftest | 新 exe `--self-test` | **0** | False | 0 | 10803 / 0 | **178 PASS / 0 FAIL**，末行 `SELF-TEST PASSED (178 checks)` |
| export63 | `--export-cases legacy63.json` | 0 | False | 0 | 254 / 0 | 生成旧 63 manifest |
| export20 | `--export-platform-cases windows-basic20.json` | 0 | False | 0 | 184 / 0 | 生成 B2 manifest |
| export-focus10 | `--export-focus-cases windows-focus10.json` | 0 | False | 0 | 187 / 0 | 生成 focus manifest |
| focus-analyzer | 真 python `tests\focus_gui_analyzer.py` | 0 | False | 0 | 0 / 139 | `Ran 35 tests` **OK** |
| basic-analyzer-regression | 真 python `tests\basic_input_gui_analyzer.py` | 0 | False | 0 | 0 / 134 | `Ran 30 tests` **OK**（旧 30 项回归） |
| legacy63-parity | 真 python `tools\check-cases-parity.py legacy63.json` | 0 | False | 0 | 234 / 0 | `PARITY OK: swift == cs at UTF-16 level; spec satisfied; 63 cases across 8 suites` + `runtime manifest match: OK` |

Python 为真实安装：`C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe`（非 WindowsApps Store 占位符）。

### self-test 与 StageA RED 的逐行关系（实际核对，非猜测）

- 新总数 **178**（StageA 143 + focus 新增 35）。
- StageA 的 143 条 PASS 行**逐行比对全部仍存在（missing=0）**。
- StageA 唯一 FAIL `focus-args-accepts-focus-suite ? … ArgumentException: unknown suite: focus` 现为 `PASS: focus-args-accepts-focus-suite ? required --suite focus; parser returned suite: focus`。
- 新增 focus 35 条全部带 `? pure synthetic; not GUI proof` 标注；`focus-unknown-suite-still-rejected` 等负例门保留。

## 4. 导出独立核对（Windows Python 独立解析 JSON，`json-verify.txt` 已归档）

- **legacy63.json**：8 suites（baseline10/drag10/emoji6/known-input10/legacy1/multiclick10/punctuation6/scroll10），TOTAL **63**，unique IDs 63，`baseline-01`→`scroll-10`。目录数据仅来自 `macos/Cases.swift`、`macos/GestureCases.swift` 只读源。
- **windows-basic20.json**：pointer 10 + keyboard 10，TOTAL **20**，unique IDs 20，specs 各自唯一（`legacy_manifest_unchanged=True`）。
- **windows-focus10.json**：top-level `cases`，TOTAL **10**，unique 有序 `focus-01`…`focus-10`；specs `a_to_b, b_to_a, activate_text, editor_shortcut, own_modal, modal_close, menu_exit, minimize_restore, protected_scope, unconfirmed_stop`；`status=implemented_pending_CC`、`trials_per_run=1`；顶层 **`gui_verified=False`、`ten_valid_trials_per_action_gate=False`**。无 `needs_supervisor`/`blocked` 字段值出现在 manifest（门控在 analyzer/策略层）。
- 三个 manifest 均带 coordinator-only warning/schema 标注；**不得据此宣称真实 GUI 行为**。

## 5. wrapper 尝试失败记录（全部保留，未删未覆盖）

1. 首个远端 collector 内联命令输出再次被 SSH/cmd 控制台吞掉（0 字节）→ 改为落盘文件读取（与 RED 轮相同返工模式）。
2. `collect.ps1` 第一次 JSON 解包把顶层对象当单元素（legacy63/basic20 显示 count=1）→ 改用 Windows Python 独立解析修正，属核对脚本缺陷，非被测物失败。
3. `jsonverify.py` 首跑假设 focus10 为 suites 嵌套（KeyError 'suites'）→ 修正为 top-level cases 后通过。
4. tar 传输后一次 `cmd dir` 计数返回 0（cmd 转义问题）→ PowerShell `Get-ChildItem` 确认 33 文件在位。
5. 无任何被测步骤 native 失败；无 Stop-序列事件。

## 6. 残留核对（read-only，无任何 kill）

按精确 owned 身份（本轮 buildout exe 全路径；python 调用路径）枚举 `Win32_Process`：**无残留**（`residual-check.txt`）。7 个 runner summary 全部 `cleanup_state=root_exited`。未使用名称/全局 kill。

## 7. 测试后源码复查与归档

- 测试后 `Get-FileHash` 复查（`remote-hash-recheck.txt`）：**31 OK / 0 MISMATCH**，源码未被测试过程改动。
- 本地归档：`.agents/runs/windows-focus-fixture-green-cc-20260930/`（`remote-staging.tgz` + 解包全量：33 源码副本、双份 hash-verify/recheck、`master.ps1`/`hashverify.ps1`/`collect*.ps1`/`jsonverify.py`/`residual.ps1`、build 日志与 exit、exe、7 组 config/evidence/stdout/stderr/native-exit、`legacy63.json`/`windows-basic20.json`/`windows-focus10.json`、`collect.txt`/`collect2.txt`/`json-verify.txt`/`residual-check.txt`）。远端 TEMP 全部保留。本报告原始 raw 同名 `.agents/reports/windows-focus-fixture-green-cc-20260930-raw.txt`（collect/verify 汇总原文）。

## 8. 明确限制（不隐瞒）

1. **本轮零 GUI 证据**：所有 self-test/导出/analyzer 均为纯 console synthetic 或源码级校验；不证明任何真实 WM 焦点/activation/modal/menu/restore 行为。
2. **focus-09（protected_scope）**：own 输入证据无法证明全部用户窗未输入，`global_protected_apps=unknown`，结论必须保持 **needs_supervisor**。
3. **focus-10（unconfirmed_stop）**：无输入场景不能自我认证安全；仍需完整真实 tool trace + 图像 observation + Close 后心跳才可评 unknown/safe，本轮未执行。
4. **10 例 GUI case 与每动作 10 次有效 trial 全部 pending**，GUI 授权弹窗阻断未解除，无审批绕过；`gui_verified=False`、`ten_valid_trials_per_action_gate=False` 维持。
5. Runner 无 descendant containment；本轮 root 均自行退出且无残留，但不构成后代回收声明。
6. 首个 native 失败即停逻辑已就位但未触发（零失败）。

**STOP：报告完毕。GUI 验证与 ten-valid-trials 门禁等待协调者单独授权，实现 owner 已停止，本轮未做任何源码改动。**
