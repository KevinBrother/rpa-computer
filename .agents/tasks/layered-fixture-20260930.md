# 拆分视觉识别与原生输入验收靶场（用户已批准）

你是真实 Claude CLI 实现worker，用当前sonnet→GLM配置；禁止GUI输入，不修改全局设置，不commit/push。写入范围仅 acceptance-fixture/** 以及 .agents/reports/layered-fixture-20260930.md 和 .agents/runs/layered-fixture-*。保留现有工作树、旧日志，产品src/crates/scripts禁止修改。

## 需求

现有fixture把一段含emoji/全角标点样例反复抄写，与输入注入混合，9次均失败无法合理归因。用户批准分层：已知字符串测输入，清晰普通文字测视觉识别，emoji/易混淆标点单独统计。两个平台同样case契约，不第三方依赖。

在现有WinForms/AppKit fixture增加 --suite 参数，四个值：baseline、punctuation、emoji、known-input。保留未指定时的legacy样例可选默认，明确记录legacy不得与新结果合并。每suite有固定case目录，case独立ID，Next trial逐个推进并用原有随机目标布局/nonce；总数 baseline10 / known-input10 / punctuation6 / emoji6，不回绕重复算独立案例。最后要显示SUITE COMPLETE，继续Next无新case。所有fixture启动及GUI不由你进行。

- baseline：10段短、清晰、不同的常用中文/ASCII/数字组合，无emoji、特殊空格、全半角标点混淆；1普通空格分词即可，不要固定同一句10次。适当出现12/34长度，最大不超过40字符，正常字形。供agent仅看图抄写。
- punctuation：6段短文字，独立覆盖全角/半角括号、中文/英文逗号、竖线、引号等；不夹emoji。特殊空格视觉不可辨，不纳入纯视觉“必过”矩阵，NBSP放known-input。
- emoji：6段非常短文字各含一个常用不同emoji（可含🌍✅）；放大显示字形，Windows避免Segoe UI不适合的fallback，可用Segoe UI Emoji；承认WinForms可能是单色，不能伪称截图能唯一判定码点。
- known-input：10条明确payload用于给agent原文而非视觉推断；覆盖ASCII、中文12/34字符、昨日38-scalar长样例、全角标点、NBSP、非BMP emoji、组合字符（如e+combining acute）和CRLF/Tab（CRLF按runtime契约期望LF，输入/期望显式分开）。GUI check期望指归一化后的expected_text；manifest字段 task_payload 与 expected_text 必须分开。若CRLF/Tab进入文本view行为不稳定也诚实记录，不能删除案例。

保持目标点选和nonce显示，清晰样例label字号约24-28，特殊emoji更大；样例换行不可裁切/超框，输入框与Check结果都可见。使用现有900x650窗口或有限增高，避免超1080画面。模块化，可以加两个平台case文件，build脚本必须列入compile。每文件1000行内。

oracle新增suite/case_id/case_index/case_total/expected_text（trial events亦需）、text_check包含actual_text、actual_utf16_hex、expected_utf16_hex；matched必须逐UTF16相等，不做Trim/Unicode归一化（Swift String==规范等价不够，改显式UTF16比较）。oracle仅coord文件，不自动暴露给agent；不要clipboard/OCR/UIA辅助。

CLI解析未知suite或缺值明确退出失败，不能无声回落legacy。提供 --self-test 无窗口/无截图/无输入模式检查case数量、唯一ID、nonempty、suite定义、CRLF normalization expected与payload差异、UTF16精确比较能拒绝NFC/NFD不同、legacy保留。提供 --export-cases PATH coordinator-only 导出case目录（全部suite、payload、expected）并退出，无窗口；这个文件不能进入GUI agent发布目录。

Windows start-windows.ps1增加受ValidateSet保护的-Suite（含legacy），只给exe传固定suite枚举，不传任意命令；原有session1身份验证、stop身份守卫保留。

更新fixture README区分三条链路，旧失败不洗白，数字不要留过时样例。

先读现状并实现pure selftest/regression，再代码；执行macOS build+self-test，Windows源码structural检查可做，但Windows compile/selftest由coord通过SSH执行。不自行远程部署或GUI。编译输出新目录 .agents/runs/layered-fixture-build-20260930（已有则换唯一suffix，不覆盖旧）。报告具体文件、命令输出、hash、未验证项。不要偷偷缩小suite/case数量。
