# Windows known-input 第9例 CRLF 差异只读诊断（2026-09-30）

- 审查角色：独立只读诊断。未改任何源码/测试/catalog、未 GUI/SSH、未 delegate、未 commit。
- 证据源：`.agents/runs/layered-win-known-input-final-{oracle,transcript,analysis}-20260930.*`（oracle/transcript 独立逐行解析，未读 40MB 图像 base64）、`src/runtime/plan.rs`、`src/backend/dispatch_core.rs`、`src/backend/keys.rs`、`acceptance-fixture/windows/{MainForm,Cases}.cs`、`acceptance-fixture/macos/main.swift`。

## 结论（先行）

**不是 input 程序重复/丢字**。是目标控件的原生换行表示（Windows Edit 控件每次 Return 键点击插入 `\r\n`）与 fixture 的 canonical-LF 预期（`ExpectedText = NormalizeCRLF(payload)`）不一致。9/10 结论保留，不洗白、不变 10/10。

## 完整证据链（task_payload → checker）

1. **task_payload（catalog）**：`known-09` payload = `'第一行\r\n第二行\r\n第三行'`（含真实 CR LF 对）；expected = `'第一行\n第二行\n第三行'`（纯 LF）。两平台 fixture 共享同一构造：`Cases.cs:90` `ExpectedText = CaseContract.NormalizeCRLF(payload)`（注释明确 "payload normalized CRLF -> LF (runtime contract)"，Cases.cs:187 显式标注 "CRLF; expected uses LF only"）。
2. **模型原始参数（transcript）**：`call_cded8cf2a63f48069ea8d6b0` @ `03:51:16.379Z`，text_input text = `'第一行\r\n第二行\r\n第三行'`（repr 含 `\r\n` 转义，即真实 CRLF 字符）。**逐字等于 task_payload，layer1 raw 精确通过**（final-analysis 中 `raw_payload_vs_task_payload=True`）。
3. **plan 层一次归一化（src/runtime/plan.rs:43-58, 217-226）**：`normalize_newlines` 把每个 `\r\n` 对归一为单个 `\n`（注释 "CRLF is normalized once here (single Return click per pair)"），每个 Unicode scalar 一个 `TextScalar` 事件 → 已知-09 产生恰好 **2 个 `\n`**（不是 2 对 `\r\n`）。
4. **注入层（src/backend/dispatch_core.rs:96-101, 147-155）**：`inject_text_scalar`：`'\n' | '\r' => click_key(Key::Return)`，`\t` → Tab，其余走原生 scalar 路径。即 2 次 Return 键点击；`keys.rs:171-196` validate_text 允许 TAB/LF/CR。**无重复 Return（CRLF 对只产生一次），无丢字**。
5. **目标控件原生表示（Windows Edit 控件语义）**：WinForms Multiline `TextBox`（MainForm.cs:280-283）每次 Return 插入 `\r\n`——这是 Win32 Edit 控件的标准文本表示，非本注入程序行为。
6. **checker（MainForm.cs:427-458）**：`got = _textBox.Text`（as-is，注释明确 "actual text is logged as-is, never normalized"），`Utf16ExactMatch(got, expected)` 严格 UTF-16 比对。
7. **oracle 实证**：known-09 `matched=false`，actual = `'第一行\r\n第二行\r\n第三行'`（hex 尾 `…000D 000A 7B2C 4E09 884C`，含 `000D 000A` = 真实 CRLF）；expected = LF 版本。**字符数核对：actual 共 13 个 UTF-16 code units（3×3 汉字 + 2×CRLF），恰好对应 2 次 Return 点击，无重复、无丢失**。
8. **analyzer 判定（final-analysis）**：layer1=True（raw 精确）、layer2=False（actual CRLF ≠ normalize(payload)=LF）、layer3=False（expected LF ≠ actual CRLF）→ verdict=`input_divergence`，`exact=false`，utf16=ok，check_sequence 单条 proven_exact=false。suite：`case_count=10, attempts=10, first_attempt_exact=9, final_exact=9`。**注意**：`input_divergence` 标签在此 case 语义上略有误导——输入层忠实复现了 raw CRLF，分歧发生在"控件原生表示 vs canonical-LF 预期"；但 analyzer 保守判定（不归一化 actual）是正确方向，不应为消除标签而软化。

## 其余 9 例旁证

- known-04（复杂 Unicode）、06 NBSP、07 non-BMP（🦄，hex 可见 surrogate 对）、08 NFD、10 Tab（hex `0009`）均 matched=true，且 actual hex 与文本自洽——排除通用注入管线缺陷；known-10 Tab 经同一条 `\t → click_key(Tab)` 路径且精确通过，进一步证明标量映射管线本身正确。

## 跨平台 catalog parity 差异

- 两平台 ExpectedText 同源推导（NormalizeCRLF），设计上不漂移；但**控件原生表示不同**：macOS `NSTextView` Return 插入 `\n`，Windows Edit 控件插入 `\r\n`。因此 known-09 在 macOS 结构上可通过（LF==LF），在 Windows 结构上不可能通过 raw-exact（CRLF≠LF）。这是平台文本表示的客观差异，不是任一侧的 bug。

## 建议的合法修复方案（仅建议，本次不执行）

1. **平台专属 expected（推荐）**：expected/target native representation 显式按平台声明——Windows `ExpectedText = payload`（保留 CRLF，即控件原生表示），macOS `ExpectedText = NormalizeCRLF(payload)`；或 catalog 增加 `expected_win_text` / `expected_mac_text` 字段。前提：
   - 保留本次旧 run 的 9/10 失败证据不改写（`layered-win-known-input-final-*20260930.*` 原样保留）；
   - 修复后必须以**未来新 run** 验证，不得以本诊断替代；
   - 禁止 `Normalize(actual)` 后冒充 raw exact——任何归一化只能体现在"预期值的平台声明"，actual 始终 as-is 参与 UTF-16 精确比对（analyzer 与 fixture `Utf16ExactMatch` 均保持不归一化 actual）。
2. analyzer 侧可选改进（非必须）：为 layer2 增加"控件原生表示"信息字段（如 Windows CRLF），但 verdict 门控不得放宽。
3. 跨平台结论汇报时应分别给出 win/mac 的 expected 语义，避免用单一 catalog 声称双平台 parity。

## 边界声明

- 本诊断只读，不执行修复/重跑；strict 9/10 保留，不宣称 10/10，不宣称双平台 GUI 结束。
- `input_divergence` 标签的语义歧义已如实指出，未据此调整任何计数。
