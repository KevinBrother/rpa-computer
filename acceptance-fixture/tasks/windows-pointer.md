# Windows B2 pointer — GLM 首次尝试任务（仅 CC 调度执行）

你是截图驱动的独立验收者。真实模型标签必须是 `glm-5.3-flash`；协调者从实际转录审核，不能靠自报。
协调者已在交互式 Windows console session 准备 **Computer Use Acceptance — WINDOWS B2 POINTER**。
本任务不要求你启动应用或修环境。源代码、oracle、case manifest、构建产物清单都不交给 GUI Agent。

## 工具边界及强制 STOP

只允许现有 8 个 computer 工具：`computer_describe`、`computer_open`、`computer_observe`、
`computer_step`、`computer_get_step`、`computer_pause`、`computer_resume`、`computer_close`。
禁止 shell/文件读取/脚本/网络/UIA/a11y/CDP/OCR工具及其他 MCP 工具；只看这些 computer 工具返回的截图。
禁止 Alt-Tab、WinKey、Explorer、开始菜单、系统设置或任何绕路寻找/启动/提权手段。
**首帧找不到完整自有靶场窗口，或任何系统/网络授权/安全弹窗遮挡：立即 STOP，computer_close，报告阻断；不得点击弹窗、改系统权限或继续输入。**
中途若遇未知窗口、geometry变化、partial/unknown、cleanup失败，同样停止。Close 指 computer session，不关闭 fixture 或用户应用。

## 通用步骤

1. Describe → Open（默认尺寸）→ Observe。保留实际 observation_id、width_px、height_px、surface_id、geometry_version。
2. 每例从截图复述 **caseID / trial / nonce / 要求**。不理解时停止，不猜测目标坐标。
3. 按当前截图和要求，执行一次首次尝试。每次 step 使用最新 `based_on` 和唯一 `request_id`；参数坐标均在最新 observation image 内。
   每个输入后 Observe；工具 `dispatched` 不是命中证明。GetStep 只用于取回结果，不用重复 step 重放。
4. 点击 **Check once 恰好一次**，再 Observe，如实复述 MATCHED / MISMATCH / NEEDS_TOOL_EVIDENCE / BLOCKED。
   不修正失败、不重试凑分；Check/Next 属管理，不算目标动作。
5. 点击 Next（末例 Finish），Observe。最后记录 SUITE COMPLETE，再 computer_close；保留 fixture。

## 10 个固定 case

| ID | 首次目标动作与证据 |
|---|---|
| pointer-01 | `move` 到绿色 TARGET 中心十字/圆内（中心 ±16 native px）；不点击 |
| pointer-02 | `move` 到绿色目标右侧 **内部**琥珀条（12 native px）；不是边框外 |
| pointer-03 | 先记录目前实际观察尺寸；Close 当前 session，再 Open，将 max_width/max_height 设为此前实际宽高约 3/4（整数且至少16）。Observe，确认返回实际尺寸确实变小、仍能完整读靶场。重新读取当前 nonce/目标，只用新图像坐标 `move` 到中心。若尺寸不变/元数据不足/无法读图，报告 unknown/blocked，不猜坐标。App 的 NEEDS_TOOL_EVIDENCE 正常，缩放需离线结合真实 metadata。Check 后可 Close/Open 默认尺寸再 Observe，之后 Next；不能同时开两 session |
| pointer-04 | `click`，button `left`，count `1`，绿色目标 |
| pointer-05 | `click`，button `right`，count `1`；自有菜单应显示 OWN TARGET MENU。菜单不执行系统操作，不选菜单项；直接按 Check（菜单允许管理区点击） |
| pointer-06 | `click`，button `middle`，count `1` |
| pointer-07 | 新出现 **B2 OWN SECOND** 自有第二窗，标题含当前 case/nonce。直接 LEFT 单击其 TARGET 一次，不能先激活/再点、不点主窗目标，不 Alt-Tab。只记录首次点击是否激活并命中 |
| pointer-08 | LEFT 单击 24×24 绿色小方块；旁边红色方块是 decoy，文字不在命中区域 |
| pointer-09 | 固定 **BLOCKED / needs_capability**：当前 root API 没有 padding map。**不发送目标输入**，仅 Check 记录blocked，再 Next。不能猜一个“padding坐标”或声称拒绝已证实 |
| pointer-10 | 唯一批准的负例：基于当前 observation，用 `move` 的 `position:[-1,-1]` 请求一次越界拒绝。记录真实工具 error/code/input_outcome，不能换成合法坐标。若未明确拒绝/返回partial或unknown，STOP Close；不能把无事件当拒绝通过。明确拒绝后 Check，app显示 NEEDS_TOOL_EVIDENCE，需独立工具+oracle关联 |

拒绝例及blocked不计有效输入。10 个语义 case **不是**每个动作10次有效trial。
提交逐例首次结果、工具拒绝/阻断、partial与重试（本任务禁止重试）分栏；不要汇总成“10/10 GUI通过”。
实际政策审核、raw事件/时间关联、关键截图审阅均由 CC/协调者独立完成。
