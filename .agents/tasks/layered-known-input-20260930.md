Operate only the visible Computer Use Acceptance fixture using the 8 computer MCP tools. No shell, files, browser, clipboard, APIs, OCR service or accessibility text extraction. Stop and close if locked/login/permission screen or fixture absent; never unlock/change permissions.
This run tests INPUT, not OCR: the task intentionally supplies exact payloads below. Open max_width1920/max_height1080 and observe. Verify suite known-input and read current case ID/nonce. Use the case's specified JSON string value as text_input (decode JSON escapes, do not type quote delimiters or escape backslashes). Do not infer/retype the sample from pixels. Ordinary spaces and NBSP are different; combining marks must remain decomposed. For case known-09 send CRLF as supplied; the host normalizes CRLF to LF. For known-10 send actual tabs. Never replace control characters with spaces/newline escape text.
For each case click the green circle once based on screenshot, inspect HIT/WRONG, click input field and select all (meta+a on macOS / ctrl+a on Windows), issue exactly ONE text_input, click Check text once, observe actual MATCHED/MISMATCH, record. Do not retry or silently correct mismatches. Next trial advances; after case10 click Next and verify SUITE COMPLETE. Unique request_id/latest based_on per action. Single keys use key_press, nonempty key_chord for modifiers. At end computer_close; leave fixture open. Dispatched is not app success. Record all case outcomes and unfinished cases honestly.

Exact payloads by case ID (JSON string values):
- known-01: `"Order 6642 shipped via DHL Express"`
- known-02: `"键盘输入验收测试样例文本"`
- known-03: `"分布式原生输入验收涵盖中文长文本逐字精确比对三十四字样例内容全部完毕"`
- known-04: `"你好，世界 🌍 | Unicode 测试 ✅ | 本文档共 3 段（含本段）"`
- known-05: `"注意：括号（全角）与问号？感叹号！"`
- known-06: `"NBSP\u00A0分隔\u00A0样例"`
- known-07: `"🦄 独角兽 non-BMP 样例"`
- known-08: `"resume\u0301 和 cafe\u0301 组合音符样例"`
- known-09: `"第一行\r\n第二行\r\n第三行"`
- known-10: `"姓名\t部门\t工号"`
