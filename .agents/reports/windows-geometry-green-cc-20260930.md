# Windows geometry StageB — CC independent review + pure execution — 2026-10-01

- 执行身份：CC（真实 Windows acer-win，Session 0 纯 console；全部步骤为 WinForms 初始化前 console 路径；未运行 `--suite geometry` UI、无 GUI/Host/capture/原生输入/网络；无 Mac/Swift 执行，Swift 仅只读 parity 数据）。实际模型 glm-5.3-flash。**实际执行时间跨 Asia/Shanghai 午夜：2026-10-01T00:00:14–00:00:26+08:00**（远端 root_start_utc 为 2026-09-30T16:00:18–16:00:25Z）；按交接要求保留指定 20260930 运行路径/报告名，不重跑。
- 结论：**12 步全绿（pure console）。** 44/44 冻结源码远端哈希核对一致；默认 helper 构建 exit 0；`--self-test` 实际 **287 PASS / 0 FAIL**（native 0）：既有 237 逐行保留（missing=0），StageA 两条 RED 需求现均 PASS，新增 geometry 50 项全 PASS；5 个导出独立 JSON 核对一致；分析器 78/25/35/30/26 tests OK；legacy63 parity OK。**无任何 GUI/DPI/系统变更/物理多显示器验收声明；十次有效 trial 仍 pending。** 零源码修改、无 rerun-for-green。

## 1. 独立语义审查（执行前，无阻断）

- **geometry10 独立**：`geometry-01…10` 唯一有序，`schema=windows-geometry-v1`，不混入 legacy63。modes：gui 6（01/02/03/04/09/10）、manual_environment 3（05 high_dpi、07 resolution_stale、08 dpi_stale）、**pure_mapping 1（06 negative_origin，status=not_gui，无需输入）**。
- **诚实门控**：statuses 仅 `pending_CC_GUI / needs_environment / not_gui`；`GeometryJudge.Local` 输出 needs_capability/needs_environment/pending_tool_evidence/unknown——**无 hit 即 pending_tool_evidence 而非 pass，缺事件=unknown，不缺事件即过**。manifest `gui_verified=false`、`ten_valid_trials_per_action_gate=false` 固定。
- **09 target_removal**：语义移除，不冒充系统 topology 变化（纯测试 `target-not-topology`）。**07/08** 需 supervisor 真实分辨率/DPI 变更 + OLD based_on 提交被拒且无输入，未做任何自动 DPI/display 改变。**10 png_dimensions** 要求 supervisor 校验真实 PNG 字节/CRC/解压尺寸，非元数据自证；作者声明不支持的 PNG 编码为 unknown（有界 8-bit 非交错 RGB/RGBA/灰度）。
- **纯映射**：`GeometryMath` 支持负 origin、clamp、floor；无效/越界/溢出返回 null；正负用例均覆盖（`map-negative-origin/map-invalid/map-overflow` 等）。
- **既有面保留**：239 StageA 断言 + native-text/focus/B2/gesture 回归与旧 63/basic20/focus10/native-policy 导出均未改动（运行逐行验证，见 §3）。
- StageA 两条断言保留原名并在 `GeometrySelfTest.Run()`，`GeometryRegression` 50 项追加其后。

## 2. 冻结源码与构建

- Manifest `…artifacts-234523/frozen-source-sha256.txt`（SHA `ca5b1d8a…`，44 条含 runner 两文件）本地/远端 `Get-FileHash` **44 OK / 0 MISMATCH**（运行前后各一次，`remote-hash-verify.txt`/`remote-hash-recheck.txt`）。
- 远端 staging（保留）：`C:\Users\Administrator\AppData\Local\Temp\computer-geometry-green-cc-9aecf97185a34525accdc296b75dc3cd`
- 默认 helper（Framework64 csc v4.0.30319，无 define、无旧 exe 复用）：**build exit 0**；新 exe SHA-256 `263e78d636377f20731848f8cac09ecc116a3cbed38e49272b988e92651cd420`（归档重算一致）。

## 3. 12 步执行（on-disk `master.ps1`，approved JSON runner，evidence 父预建/leaf 原子保留，各步 120s）

| 步骤 | native | timed_out | 结果 |
|---|---|---|---|
| 1 `--self-test` | **0** | False | **287 PASS / 0 FAIL**，末行 `SELF-TEST PASSED (287 checks)`（20562B stdout，stderr 0） |
| 2 `--export-geometry-cases` | 0 | False | `geometry10.json` |
| 3 `--export-cases` | 0 | False | `legacy63.json` |
| 4 `--export-platform-cases` | 0 | False | `platform20.json` |
| 5 `--export-focus-cases` | 0 | False | `focus10.json` |
| 6 `--export-native-text-policy` | 0 | False | `native-policy.json` |
| 7 `tests/geometry_gui_analyzer.py` | 0 | False | **Ran 78 tests OK** |
| 8 `tests/windows_native_text_analyzer.py` | 0 | False | **Ran 25 tests OK** |
| 9 `tests/focus_gui_analyzer.py` | 0 | False | **Ran 35 tests OK** |
| 10 `tests/basic_input_gui_analyzer.py` | 0 | False | **Ran 30 tests OK** |
| 11 `tests/gesture_gui_analyzer.py` | 0 | False | **Ran 26 tests OK** |
| 12 `tools/check-cases-parity.py legacy63.json` | 0 | False | **PARITY OK: swift == cs at UTF-16 level; 63 cases across 8 suites** + runtime manifest match OK |

Python：真实 `…\Programs\Python\Python312\python.exe`（非 Store 占位符）。分析器选择依据：manifest 内交付的测试集（geometry/native-text/focus/B2/gesture）；manifest 未含独立 layered-analyzer 测试文件，故无该项（非跳过既有要求）。

### self-test 实际构成（逐行比对，非猜测）

- **287 = 237 既有 + 50 geometry 新增**；前轮 237 PASS 行 missing=0。
- StageA 两条 RED 需求现均为 PASS（原样 stdout）：
  - `PASS: geometry-args-accepts-geometry-suite ? required --suite geometry; parser returned suite: geometry`
  - `PASS: geometry-args-accepts-independent-geometry-export ? required independent --export-geometry-cases PATH; parser accepted; no file operation; existing console modes must stay unset`

## 4. 导出独立核对（Windows Python，`json-verify.txt`/`k09-check.txt` 归档）

- **geometry10**：10 唯一有序；`gui_verified=False`、`ten_valid_trials_per_action_gate=False`、每例 `trials_per_run=1`；modes 6/3/1 如上；**无任何 passed/green status**。
- **legacy63**：63 唯一（baseline-01→scroll-10）；known-09 恰 1 条，payload **2×CRLF/13 units**、expected **2×LF/11 units**、`expected≠payload`（canonical 契约未变）。备注：首次核对脚本自身转义误报 `payload_has_CRLF=False`（wrapper 错误，已保留），以 `k09-check.txt` 的 `chr(13)+chr(10)` 计数纠正。
- **platform20**：20，`legacy_manifest_unchanged=True`。
- **focus10**：10 唯一，`gui_verified=False`、gate=False。
- **native-policy.json**：`windows-winforms-known09-crlf-v1`，native 13 units / canonical 11 units，`actual_normalized=False`、`gui_verified=False`。
- **导出非 GUI 证明**：所有导出均为目录/契约数据，不构成任何 GUI/视觉验收。

## 5. Wrapper 错误记录（保留）

1. jsonverify 首版 `'\\r\\n'` 4 字符字面量误报 known-09 payload 无 CRLF → 以正确 `chr(13)+chr(10)` 计数重核（数据本身无误，脚本缺陷保留归档）。
2. 其余 wrapper（master/collect/hashverify/residual）一次成功；无 cmd `%ERRORLEVEL%` 作权威状态；无 inline-cmd 复用。

## 6. 测试后完整性与残留

- 测试后哈希复查：**44 OK / 0 MISMATCH**。
- 残留核对（read-only，无 kill）：按本轮精确 owned 路径（`gbout\buildout\ComputerUseAcceptance.exe` 与 staged python 全路径）枚举 `Win32_Process`：**无残留**（`residual-check.txt`）。12 个 runner summary 全部 `root_exited`、双输出 drain 完整。Notepad24332 未触碰。

## 7. 归档

`.agents/runs/windows-geometry-green-cc-20260930/`：`remote-staging.tgz` + 解包全量（44 冻结源码副本、双份 hash-verify/recheck、master/collect/jsonverify/k09/residual 脚本、build 日志/exit、exe（SHA 可复核）、12 组 config/evidence/stdout/stderr/native-exit、5 个导出 JSON、`collect.txt`/`json-verify.txt`/`k09-check.txt`/`residual-check.txt`）。远端 TEMP 保留；geometry StageA RED、native-text R2、旧 focus/known-input snapshot 与报告未触碰。

## 8. 边界（不隐瞒）

1. **零 GUI 证据**：本轮全部为 console 路径；geometry 10 例中 6 例 GUI、3 例 manual_environment、1 例 pure_mapping——均未做 GUI 执行；**未声明任何 geometry GUI/high-DPI/系统变更/物理多显示器验收**。
2. **十次有效 trial 仍 pending**；`gui_verified=false`、`ten_valid_trials_per_action_gate=false` 保持。
3. 05/07/08 的真实 DPI/人工环境证据、10 的 supervisor PNG 字节审核、不支持的 PNG 编码（unknown）均留待 GUI/监督阶段。
4. 未构建/测试产品 Rust 代码；Rust subprocess/EOF 验证由 EOF CC 独立负责。
5. Root Rust、Mac/Linux、Host/renderer 未触碰；未 commit/push/worktree；实际模型 glm-5.3-flash。

**STOP：StageB pure console 全绿已报告。GUI/授权/trial 门禁由协调者掌握；本轮未做任何源码改动。**

## 协调原始证据复核（工具记录时间 2026-10-01 00:07 CST）

协调逐一亲读12个runner summary及stdout/stderr：全部native0、无timeout、root_exited、双输出drain完整；selftest287/0；分析器78/25/35/30/26与旧63parity均通过。几何导出10个ID唯一有序，6 gui pending、3 manual_environment、1 pure_mapping/not_gui，两个GUI/trial完成标志均false。当前root177、新EOF6、Geometry44源码均hash匹配冻结。CCexec1456 terminal0，转录实际model=glm-5.3-flash。

计数纠正：**287=237既有+2个StageA需求断言+48个GeometryRegression断言**，geometry合计50，不是StageA239之外再追加50。原“GeometryRegression50”表述有误，不改变任何原始执行结果。存在事件本身也不足以判通过，仍须相应工具/PNG/环境/nonce证据；不可把缺/有事件直接当GUI成功。未运行GUI程序、未做物理DPI/多屏验收。
