# Windows known-input — 十个已知 payload 的首次视觉输入任务

协调者先在隔离的 source-free 发布目录准备原生 `--suite known-input` 靶场。
真实模型标签要求 `glm-5.3-flash`，由协调者从实际转录独立核验，不靠自报。
本任务仅提供输入 payload，不附 oracle、源码、manifest、坐标或程序化读取方式。
协调者在发布前核对本任务的独立 StageB 冻结 hash；禁止给 GUI Agent 暴露源码目录。

## 工具边界与停止条件

只允许八个既有工具：`computer_describe`、`computer_open`、`computer_observe`、
`computer_step`、`computer_get_step`、`computer_pause`、`computer_resume`、`computer_close`。
禁止 shell、文件读写、脚本、网络、UIA/a11y、CDP、OCR 工具或任何额外 MCP 工具。
只看 computer 工具返回的截图，坐标必须来自最新截图，只操作本套自有靶场。
禁止 Alt-Tab、WinKey、Explorer、开始菜单、系统设置、权限变更或操作用户应用。

**首帧窗口缺失、不可读、被遮挡，或出现系统/网络授权/安全弹窗：立即 STOP、
computer_close 并报告阻断，不点击弹窗或绕路启动/提权。** 中途遇未知窗口、焦点
无法确认、geometry 改变、partial/unknown、cleanup 失败，同样停止，不能盲目重放。
Close 只关闭 computer session，不关闭 fixture 或任何用户窗口。

## 固定首次流程

1. Describe → Open → Observe；保留实际 observation_id/尺寸/surface_id/geometry_version。
2. 从截图复述 suite、caseID、trial、nonce。若与下面语义 case 对不上，停止，不猜。
3. 单击自有输入框，Observe 确认焦点。使用 `key_chord`，modifiers `["ctrl"]`、key `a`
   选中自有输入框内容，然后 Observe；不得对别的窗口发送这个组合键。
4. 从本任务取当前 case 的 JSON 字符串，**解码后**用一次 `text_input` 输入完整 payload。
   不要输入外层引号或反斜杠转义的字面文本，不手动替换空白或规范化 Unicode。
   每次 step 使用最新 based_on 和唯一 request_id；输入后 Observe。
5. 点击 **Check once 恰好一次**，Observe，如实复述可见结果。失败原样保留，不修正、
   不重试、不同一个逻辑操作再发请求。GetStep 只取已提交结果，不重放输入。
6. Next（最后 Finish）→ Observe，依次覆盖十例。见 SUITE COMPLETE 只表示遍历完成，
   不代表全部 matched；最后 computer_close 并保留 fixture。

聚焦点击、Ctrl+A、Check/Next 是准备/管理操作；本套目标只计 `text_input`。
普通文本不得拆成键盘动作来冒充精确输入；本接口没有 `key_press` 工具动作。

## 10 个 payload（JSON 字符串；必须解码）

| caseID | JSON payload |
|---|---|
| known-01 | `"Order 6642 shipped via DHL Express"` |
| known-02 | `"键盘输入验收测试样例文本"` |
| known-03 | `"分布式原生输入验收涵盖中文长文本逐字精确比对三十四字样例内容全部完毕"` |
| known-04 | `"你好，世界 🌍 \u007c Unicode 测试 ✅ \u007c 本文档共 3 段（含本段）"` |
| known-05 | `"注意：括号（全角）与问号？感叹号！"` |
| known-06 | `"NBSP\u00a0分隔\u00a0样例"` |
| known-07 | `"🦄 独角兽 non-BMP 样例"` |
| known-08 | `"resume\u0301 和 cafe\u0301 组合音符样例"` |
| known-09 | `"第一行\r\n第二行\r\n第三行"` |
| known-10 | `"姓名\t部门\t工号"` |

known-04 的 `\u007c` 解码为竖线 `|`，只是避免 Markdown 表格分栏，不改变 payload。
保留 known-06 的 NBSP，不换为空格；保留 known-08 的分解组合重音，不转 NFC；
保留非 BMP 字符、Tab 和 CRLF。Windows native text 政策是
`windows-winforms-known09-crlf-v1`：known-09 在自有多行 TextBox 保持 CRLF，
不是“Host 一律把 CRLF 归一为 LF”。不读取 oracle、推导 expected 或修改实际输入。

提交每例首次结果、blocked/unknown、partial 和未完成项；派发成功或自报 matched
不等于 GUI 认证。真实事件/工具链关联、实际模型/八工具政策、关键截图及窗口所有权
仍由 CC/协调者独立审核，本任务不授权追加轮次或重启来凑成功率。
