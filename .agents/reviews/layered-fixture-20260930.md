# Layered Fixture 只读独立审查 — 2026-09-30

只读审查：未改 fixture/src，未跑 GUI，未 commit。验证对象为当前工作树源码 + 9/30 mac 构建 `/private/tmp/computer-layered-mac-fixture-20260930/ComputerUseAcceptance.app`。

## 运行结果（本机实测）

- `check-cases-parity.py`：`PARITY OK: swift == cs at UTF-16 level; 33 cases across 5 suites`（exit 0）。
- 新构建 `--self-test`：**40/40 PASS，exit 0**（含 count/ids/CRLF/长度/NBSP/combining/non-BMP/tab/suite-parse/UTF16 语义全组）。
- 新构建 `--export-cases /tmp/cases-export-20260930.json`：exit 0，known-09 `task_payload='第一行\r\n第二行\r\n第三行'`、`expected_text='第一行\n第二行\n第三行'`（仅 LF），符合契约。

## 已确认正确（附行号）

1. **四 suite 目录同 33 条、双平台一致**：Cases.swift:77-128 与 Cases.cs:134-189 逐条 payload 一致（parity 工具已证），无重复 ID（Cases.swift:166-169 / Cases.cs:249-251）。
2. **UTF-16 长度样例**：中文 34 字样例 = baseline-05（34 UTF-16 units，实测）与 known-03（34）。baseline 最长 35 是 ASCII 条目 baseline-06（35），max≤40 成立；注释 `Cases.cs:132-133` "lengths include 12 and 34" 与实测 `[11,12,13,27,28,29,34,35]` 一致，无误导。
3. **比较不 Trim、不 NFC**：`Cases.swift:58-64`（Array(utf16) 逐 code-unit）；`Cases.cs:102-122`（显式逐 char ordinal）。self-test 覆盖 rejects-nfc-nfd / trailing-space / crlf-vs-lf / nbsp-vs-space，全 PASS。长度展示用 grapheme count（main.swift:398 / MainForm.cs:429），informational，不参与判定。
4. **SUITE COMPLETE 不生成新 case**：mac main.swift:340-343（startTrial guard）+ 420-432（只 log 一次 suite_complete，suiteFinished 置位）；win MainForm.cs:346-350 + 387-402。二次点 Next 双平台均静默保持 COMPLETE。⚠️ 小注：COMPLETE 后点 "Check text" 仍会按**最后一个 case** 记一条 text_check（main.swift:391-418 / MainForm.cs:421-453）——不是新 case，但 analyzer 需按 (nonce, case_index) 去重/区分。
5. **CLI 无 GUI 提前退出**：mac `--self-test`/`--export-cases` 在 main.swift:443-469 处理，`NSApplication.shared` 于 471 才创建；win 在 MainForm.cs:503-525 处理，`Application.Run` 于 532。未知 suite / 缺值双平台 fail closed exit 1（main.swift:52-61；MainForm.cs:484-501），且 suite 校验在 selfTest 分支之前。
6. **Windows scheduledtask-Suite**：MainForm.cs 全部经由 `--suite` 参数 + SuiteNames.TryParse（Cases.cs:44-55）白名单，无 Process/shell/服务代码；Program.Main 为普通 WinForms 入口，无 Session 0 相关调用。
7. **oracle 语义**：text_check 同时记 raw `actual_text` + `actual_utf16_hex` + `expected_utf16_hex`（main.swift:407-417；MainForm.cs:441-452），hex 可直接证明 NFC/NFD、NBSP(00A0)、combining(0301)、CRLF(000D) 等真实差异。全代码无 Clipboard/UIA 路径。
8. **Theme 可见性（静态）**：win form BackColor=White、label 默认黑字（MainForm.cs:238,260-266）；status/result 每次重置为 Black/White（MainForm.cs:368-371；main.swift:358-361）。无隐形前景色。

## 已确认问题（均为「中性输入域」，writer 正在修 — 确认其必要性）

- **A. Windows TextBox 未设 AcceptsTab/AcceptsReturn**（MainForm.cs:280-286）：两者默认 false。后果：known-10 的 Tab 经注入后**移动焦点**而非输入 `\t`；known-09 的 Enter 在无 AcceptButton 时可能不产生换行。这不是比较逻辑问题，而是 agent 无法忠实输入。修复方向：`_textBox.AcceptsTab = true; _textBox.AcceptsReturn = true;`。
- **B. Mac NSTextView 未禁自动替换**（main.swift:302-307）：程序化创建的 NSTextView 的 automaticQuote/Dash/Text/SpellingCorrection substitution 继承用户 smart-punctuation 偏好，未显式关闭。后果：punctuation-06 的直引号 `"` 可能被改写为弯引号、`--` 变 `—`，spelling correction 可能改写已输入文本 → 验收中性域被污染。修复方向：显式置 `automaticQuoteSubstitutionEnabled/automaticDashSubstitutionEnabled/automaticTextReplacementEnabled/automaticSpellingCorrectionEnabled/automaticGrammarCheckingEnabled = false`（及 `automaticTextCompletionEnabled`，若可用）。

## 必须如实报告的层间差异（勿 normalize ActualText）

- **CRLF**：Windows 多行 TextBox 的 `.Text` 对键入换行返回 **CRLF**；mac NSTextView 返回 **LF**。expected_text 设计为 LF-only（契约注释 Cases.cs:13-15 / Cases.swift:13-15）。因此 Windows 上 known-09 即使忠实输入也会 **MISMATCH**（actual 含 000A 000D vs expected 000A），`actual_utf16_hex` 会显式呈现。代码已确认**未做任何暗中 normalize**（MainForm.cs:423 直接取 `_textBox.Text`；main.swift:392 直接取 `textView.string`）——正确行为。请 analyzer 层把 "Windows known-09 MISMATCH + hex 含 000D" 解读为平台契约证据而非 agent 失败；不要在 fixture 内改 ActualText。

## 风险 / 待验证

- **D. build/macos 二进制过期（阻塞项之一）**：`acceptance-fixture/build/macos/.../ComputerUseAcceptance` 时间戳 9/24 18:53，**不识别 --self-test（静默忽略参数直接开 GUI，挂起 120s 才被我杀掉）**。验收/自测必须用 9/30 新构建或 writer 正在产出的 `layered-fixture-build-20260930-c`。建议 coord 侧固定以新构建路径为准，或刷新 build/。
- **E. Windows 编译未验证**：初失败日志 `.agents/runs/layered-win-build-20260930.log` 为 CS1593 @Cases.cs(333,17)（旧版本；当前源码 333 行 `add0(...)` 两参数合法）。**不声称 Windows 编译已通过**；out-fixed SHA77FABB90 未在本审查中复核。
- **静态布局不裁切**：34 字中文样例 + "Type exactly:  " 前缀在 844×142（win, 24pt）/ 860×144（mac, 26pt）估算 2 行可容纳，但**未经截图验证** — 待 coord 截图确认（mac 侧当前还有窗口落在非主屏的可见性问题，见 writer probe 任务）。
- 小瑕疵：Windows `--seed abc` → `ulong.Parse` 未捕获异常（MainForm.cs:477/479），崩溃 exit≠0，fail-closed 但不优雅；GUI 不会启动，风险低。
