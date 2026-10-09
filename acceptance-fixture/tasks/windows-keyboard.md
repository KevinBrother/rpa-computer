# Windows B2 keyboard — GLM 首次尝试任务（仅 CC 调度执行）

真实模型标签要求 `glm-5.3-flash`，由 CC 实际转录确认。协调者先准备原生
**Computer Use Acceptance — WINDOWS B2 KEYBOARD**，你只操作它的 EDITOR 和管理按钮。
源码/oracle/export manifest 不进入 GUI Agent 发布目录、cwd 或 prompt。

## 工具、安全与 STOP

只允许 `computer_describe/open/observe/step/get_step/pause/resume/close` 这8个既有工具。
禁止 shell/文件/脚本/网络/a11y/UIA/CDP/OCR工具/其他MCP。只能看返回截图。
禁止 Alt-Tab、WinKey、Explorer、系统设置/权限调整、打开任何其他应用。
**首帧目标窗缺失/被遮挡或出现系统、网络授权、安全弹窗：STOP并 computer_close，报告，不点击弹窗。**
任何时刻 focus无法确认、出现未知窗口、input_outcome为partial/unknown或cleanup失败，停止并Close，不盲目重放。
关闭的是 computer session，不是fixture或用户窗口。

## 每个 case

1. Describe→Open→Observe；从截图复述 suite/caseID/trial/nonce/要求。
2. 单击自有 **EDITOR — TARGET** 内，Observe确认编辑框焦点；不碰下方红色DECOY。
   这次聚焦单击是准备动作，不计键盘有效输入。
3. 依据下表仅一次首次目标动作序列。每次 step 用最新 observation_id 作 based_on，唯一request_id，之后Observe。
   `key_chord` 不允许空 modifiers；普通键使用短 `key_hold`，绝不用 `text_input`/粘贴替代原生按键。
4. **Check once 一次**→Observe→复述可见状态、文本/选择/安全命令指示。失败不修正、不重试。
5. Next/末例Finish→Observe。记录 SUITE COMPLETE 后 computer_close；保留fixture。

| ID | 目标操作（全部是自有窗口安全组合） |
|---|---|
| keyboard-01 | `key_hold` key `a`, duration_ms `80`；期望真实down/char/up、文本a |
| keyboard-02 | `key_hold` key `enter`, duration_ms `80`；Observe；再 `key_hold` key `tab`, duration_ms `80`。此自有多行 EDIT 捕获Enter/Tab为CRLF和TAB，不跳其他应用 |
| keyboard-03 | `key_chord` modifiers `["ctrl"]`, key `a`；选择 ALPHA BRAVO 全部 |
| keyboard-04 | 先点击 EDITOR 同行文本末尾之后，确认caret位于 ALPHA BRAVO 末尾；再 `key_chord` `["shift"]`, key `left`，只选最后 O |
| keyboard-05 | `key_chord` `["alt"]`, key `j`；仅点亮 **OWN SAFE COMMAND**。允许的Alt仅此自有组合，不使用任何系统Alt组合 |
| keyboard-06 | `key_chord` `["ctrl","shift"]`, key `j`；点亮自有MULTI命令，不打开任何系统菜单 |
| keyboard-07 | `key_hold` key `shift`, duration_ms `120`。无文本是预期，但不能以无重复字符证明release；等待工具结果，Check由真实down/up时间判定 |
| keyboard-08 | `key_hold` key `shift`, duration_ms `900`；同上，原生down/up才是长hold/release证据 |
| keyboard-09 | `key_chord` `["ctrl"]`, key `a`；Observe；`key_hold` key `b`, duration_ms `80`。应替换为b，同时需Ctrl真实release在b之前 |
| keyboard-10 | 仅一次负例：`key_chord` `["ctrl"]`, key `__invalid__`；需整个请求明确拒绝，不能先派发Ctrl或部分有效键。绝不改成合法键。明确拒绝后Check显示NEEDS_TOOL_EVIDENCE；若结果模糊或出现输入，STOP/Close并报告 |

期望默认Latin键盘产生精确a/b；如实际布局不同，保留原始结果，不切换系统键盘布局去凑分。
无原生release证据就是unknown/mismatch，不补发release伪造成原case通过（由Close按契约尽力cleanup）。
拒绝不是有效键盘输入，聚焦/Check/Next不是目标动作；10个语义case不满足每基础动作10有效trial门禁。
工具dispatch、可见结果、oracle、政策审核与截图审阅分别记录，绝不自称完整GUI验收通过。
