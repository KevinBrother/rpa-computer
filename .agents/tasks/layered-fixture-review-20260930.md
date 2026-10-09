只读独立审查。写入仅 .agents/reviews/layered-fixture-20260930.md。不可改fixture或src，不GUI、不commit/push。当前writer正在修windows/Cases.cs suite-parse-loop add/add0参数编译错误，此项已知由coord重编译，不需重复泛化设计。请聚焦验收正确性：
- macOS/Windows四suite case目录同33条含legacy、ID/UTF16/CRLF预期一致，无重复回绕，最后SUITE COMPLETE不生成新case；oracle case信息与GUI当前一致，字符串比较不Trim、不NFC归一。
- --self-test / --export-cases 在NSApp/Application.Run前退出，无GUI；未知suite/缺值fail closed；Windows scheduledtask-Suite受ValidateSet，无shell/会话0。
- 字号/label高度/最长34中文样例、当前case header、Check结果静态布局不裁切；Theme默认文本颜色与背景不能隐形。
- 明确检查Windows TextBox multiline的AcceptsTab/AcceptsReturn；Mac NSTextView是否自动quote/dash/text/spelling replacement（验收neutral输入域不应混入自动替换），TAB/CRLF从native事件传入后应用真实返回文本是否会改变（Windows.Text常为CRLF，必须报告这层差异而不暗中normalize ActualText来通过）。
- knownCRLF/combining/NBSP语义可靠，oracle raw UTF16hex能证明实际差异，不借Clipboard/UIA。
读必要代码，运行现有Macself-test和pureparity，只读报告具体行号/确认反例/风险，不需要冗长背景。涉及GUI表现只标待截图验证，不推测已通过。Windows真实csc初始失败日志在 .agents/runs/layered-win-build-20260930.log；不声称Windows编译已通过。若发现阻塞请报告具体修复方向。
