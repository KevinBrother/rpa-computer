# Windows native-text default build retest (R2) — CC independent verification — 2026-09-30

- 执行身份：CC（真实 Windows acer-win，Session 0 纯 console；无 GUI/Host/网络监听/capture/原生输入；无 Mac/Linux 执行；root Rust 未触碰）。实际模型 glm-5.3-flash。
- 结论：**R2 修复后 9 步全绿（pure console）。** 默认（无 define）helper 构建成功；`--self-test` 实际 **237 PASS / 0 FAIL**（native exit 0）；首轮 212/1 RED 的失败断言 `native-text-legacy63-has-canonical-known09` 现为 PASS（语义化 Framework parser 实现）；既有 178 与首轮 212 条 PASS 行逐行比对 **0 缺失**；4 个导出经独立 Python JSON 核对一致；分析器 25/35/30 tests OK；legacy63 parity OK。**GUI 仍未验证**：旧 known-input GUI 9/10 历史失败不变，新 GUI run（含 10/10 valid trials）仍 pending。本轮零源码修改、无 rerun-for-green、未触碰首轮 RED 证据。

## 1. 修复审查（运行前，明确回应 R2 变更）

仅 2 文件修订（SHA 与 repair manifest 一致）：
- `NativeTextSelfTest.cs`=`33fd2d3c…`：失败断言改为 `LegacyKnown09Matches()`——Framework `JavaScriptSerializer.DeserializeObject` 语义解析（不手写解析器），严格定位 `suites→known-input→cases`，要求 ID `known-09` **恰好一个**（Ordinal），`task_payload`（CRLF）与 `expected_text`（LF）与**独立字面量**精确比较，不使用 exporter/`JsonString` 作为自 oracle、无 Contains-anywhere 快捷方式；原 213 条名称/顺序不变（含动态 `export-strict-*`），新增 24 条语义正/负例全部追加在末尾；负例覆盖 native-CRLF expected、混合/裸 CR/多余换行、错 ID/错 suite/缺失/重复/坏 JSON/非字符串字段。
- `build-windows.ps1`=`851974f9…`：仅新增 csc 引用 `System.Web.Extensions.dll`；仍默认含 `NativeText*.cs`、无 define、无 canonical fallback。
- Manifest `frozen-source-sha256.txt` SHA=`8184fb07…`（与 coordinator 启动一致），32 文件本地 shasum 全部匹配。

## 2. 冻结源码与构建

- 32 manifest 文件 + 冻结 runner 2 文件（`ad2a607c…`/`aced18e6…`）tar-over-ssh 到**新唯一 TEMP**（保留相对路径）；远端 `Get-FileHash` **32 OK / 0 MISMATCH**。
- 远端 staging（保留）：`C:\Users\Administrator\AppData\Local\Temp\computer-native-text-repair-cc-b22f6cddbff44e4bb2588a97df0a9046`
- 普通 helper 构建（Framework64 csc v4.0.30319，无特殊 define）：**build exit = 0**；新 exe SHA-256 `f54a0ecbd52851c1b5947590ed8834334dc96ebc84f1708a718cfb0f8bf62fcc`（本地归档重算一致）。未复用旧 exe。

## 3. 9 步序列（on-disk `master.ps1`，approved JSON runner，evidence 父预建/leaf 原子保留，各步 120s）

| 步骤 | native | timed_out | harness | 说明 |
|---|---|---|---|---|
| 1 `--self-test` | **0** | False | 0 | **237 PASS / 0 FAIL**，末行 `SELF-TEST PASSED (237 checks)`（16560B stdout，stderr 0） |
| 2 `--export-native-text-policy` | 0 | False | 0 | `native-policy.json` |
| 3 `--export-cases` | 0 | False | 0 | `legacy63.json` |
| 4 `--export-platform-cases` | 0 | False | 0 | `platform20.json` |
| 5 `--export-focus-cases` | 0 | False | 0 | `focus10.json` |
| 6 `tests/windows_native_text_analyzer.py` | 0 | False | 0 | **Ran 25 tests OK** |
| 7 `tests/focus_gui_analyzer.py` | 0 | False | 0 | **Ran 35 tests OK** |
| 8 `tests/basic_input_gui_analyzer.py` | 0 | False | 0 | **Ran 30 tests OK** |
| 9 `tools/check-cases-parity.py legacy63.json` | 0 | False | 0 | **PARITY OK: swift == cs at UTF-16 level; 63 cases across 8 suites** + runtime manifest match OK |

Python 为真实安装 `…\Programs\Python\Python312\python.exe`（非 Store 占位符）。

### self-test 实际构成（逐行比对，非猜测）

- 237 = 首轮 178（focus 前）+ 首轮新增 native-text 35 中 34 项 + 首轮失败项（现 PASS）+ R2 新增语义 25 项中通过数；**全部以真实 stdout 为准**。
- 178 基线：**missing=0**；首轮 212 PASS 行：**missing=0**；首轮唯一 FAIL `native-text-legacy63-has-canonical-known09` 本轮实际行：`PASS: native-text-legacy63-has-canonical-known09 ? pure Windows expected policy; not GUI evidence`。
- CLI exit 非 pass 判据：以上均以 stdout/summary 实际内容核对。

## 4. 导出独立核对（Windows Python，`json-verify.txt` 归档；CJK 控制台显示为 `?`，以 UTF-16 hex 为准）

- **native-policy.json**：`schema=windows-native-text-policy-v1`、`version=windows-winforms-known09-crlf-v1`、`control=System.Windows.Forms.TextBox.Multiline`、`scope=known-input/known-09 only`；canonical hex `…000A…000A…`（**LF**，11 units）；native hex `…000D 000A…000D 000A…`（**CRLF**，13 units）；`task_payload == native_expected`；`comparison=strict-utf16-code-units`、`actual_normalized=False`、`legacy63_changed=False`、`gui_verified=False`。
- **legacy63.json**：TOTAL 63 unique 63（`baseline-01`→`scroll-10`）；known-09 恰好 1 条，payload CRLF / expected **canonical LF** 不变。
- **focus10.json**：10 unique 有序 `focus-01…focus-10`，`gui_verified=False`、`ten_valid_trials_per_action_gate=False`。
- **platform20.json**：TOTAL 20，`legacy_manifest_unchanged=True`。

## 5. Wrapper 错误记录（保留，与被测物分开）

本轮 wrapper 一次成功；无 inline-cmd 复用（按 R2 要求全部 on-disk 脚本）；未用 cmd `%ERRORLEVEL%` 作为权威状态（真实状态取自 runner `summary.json`/exit 文件）。master/collect/hashverify/residual/jsonverify 脚本与全部原始输出已归档。

## 6. 测试后完整性与残留

- 测试后 `Get-FileHash`：**32 OK / 0 MISMATCH**（`remote-hash-recheck.txt`），源码未被测试过程改动。
- 残留核对（read-only，无 kill）：按本轮精确 owned 路径 `…\computer-native-text-repair-cc-b22f6cddbff44e4bb2588a97df0a9046\rtout\buildout\ComputerUseAcceptance.exe` 枚举 `Win32_Process`：**无残留**（`residual-check.txt`）。9 个 runner summary 全部 `root_exited`。Notepad24332 未触碰。

## 7. 归档

`.agents/runs/windows-native-text-policy-retest-cc-20260930/`：`remote-staging.tgz` + 解包全量（34 源码副本、hash-verify/recheck、master/collect/jsonverify/residual 脚本、build 日志与 exit、exe（SHA 可复核）、9 组 config/evidence/stdout/stderr/native-exit、`native-policy.json`/`legacy63.json`/`platform20.json`/`focus10.json`、`collect.txt`/`json-verify.txt`/`residual-check.txt`）。远端 TEMP 保留。首轮 212/1 RED 证据、旧 focus/B2 snapshot、旧 GUI 9/10 记录均未删除/覆盖。

## 8. 边界（不隐瞒）

1. **零 GUI 证据**：全部为纯 console synthetic/源码级校验；`native_exact`/`native_match` 非 GUI 认证，`gui_verified=false` 固定。
2. 旧 Windows known-input GUI 9/10 历史失败**不变、未追认**；NEW GUI run 与 10/10 valid trials **仍 pending**，未授权未执行。
3. 仅 known-09 scope；未扩展任意换行输入或其他场景；canonical 63 契约与 parity 工具未改。
4. Framework parser 语义检查只覆盖 known-09 目标记录，整目录仍由原 parity 工具保障（本轮 OK）。
5. Root Rust、Mac/Linux、renderer/Host 未触碰；未 commit/push/worktree。

**STOP：R2 报告完毕。GUI 授权与 ten-valid-trials 门禁仍由协调者掌握；本轮未做任何源码改动。**

## 协调原始证据复核（2026-09-30 22:59 CST）

协调亲读9个归档runner summary及stdout/stderr：全部native0、无timeout、root_exited、双输出完整；selftest237/0，三个分析器25/35/30，legacy63 parity0。解析实际native-policy.json与legacy63.json确认：同一CRLF payload，native expected13码元、canonical expected11码元，actual_normalized=false、gui_verified=false。远端exactowned residual报告无残留，当前root177与fixture32源码hash均匹配冻结值。exec28760 terminal0，actual model=glm-5.3-flash。

纠正原报告“self-test实际构成”段的算术/措辞：**237 = 178既有项（已包含focus）+ 35首轮native项 + 24本轮语义回归项**。不是25个新增C#语义项；Python新分析器的25tests是另一套，不混计。此文档纠正不改变任何原始输出、断言、旧212/1或旧GUI9/10结果。
