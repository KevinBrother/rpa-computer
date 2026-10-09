# Windows 跨应用验收任务包实施计划

> **For agentic workers:** 使用 subagent-driven-development；用户分工优先：gpt-6.1-sol只负责代码/测试源码与compile-only，CC+GLM负责独立review及Windows运行。不worktree/commit/push。

**Goal:** 将已经批准的crossapp-01..06从场景文字落实为独立、安全、可复现的Windows任务包，带自有资产和文件结果检查；不把准备成功当作GUI验收。

**Architecture:** 新增独立acceptance工具，不改产品输入内核或旧fixture。准备器只创建一个全新owned目录，分离agent任务、可见应用资产和supervisor-only manifest/检查信息。真实GUI仍需交互会话及窗口所有权预检，本批不启动任何应用、Host或网络监听，也不修改权限。

**Tech Stack:** Python标准库（目标机已有Python3.12）、纯离线HTML/CSS/JS、现有Windows原生drag/focus fixture（只复用、不改旧源码）。

## 已批准场景的落地选择

不采用“仅追加六段泛泛说明”，也不新增带进程管理/应用控制的通用自动化框架。选择独立任务包：文件准备/结果检查可在Windows console验证，真实应用启动由既有CC监督流程在授权后执行，绝不从任务包中自动启动或杀应用。

1. **crossapp-01 / Notepad**：全新自有文档与含run nonce的输入任务，ASCII/Unicode/多行/选择及快捷键，保存到owned结果路径。明确Windows原生换行expected、编码识别和精确比较；不得normalize actual掩盖错误。禁止已有Notepad24332及共享窗口。不取得独立HWND/PID/创建时间/可见标题身份则needs_ownership。
2. **crossapp-02 / Calculator**：清零→可见按钮10×20=，三次独立观察和唯一trial ID，不拿重复读取同一帧凑三次。必须确认为本轮可操作的自有窗口；进程共享/所有权不明时停止。结果200需要实际截图与逐次工具trace，任务包不能自判。
3. **crossapp-03 / Explorer**：新目录内64个无敏感可识别文件和source/destination；选择、真实滚动、把指定文件拖到destination。仅作用于本轮文件。检查器只读owned目录内最终清单/hash，能判定文件是否实际移动，不据最终状态声称证明拖拽操作链。清理只针对准确owned目录，未知文件/身份变化不删。
4. **crossapp-04 / 离线浏览器页**：无需HTTP/外部资源的长页、横向区域、表单及拖放。页面显示case/run/nonce及可见结果；轻量清晰布局、大号文字、明显目标、无动效依赖。新隔离浏览器profile、owned下载目录由监督者配置并核实；不改全局浏览器设置。页面可显式导出有界事件记录到owned下载位置；若下载位置不可确认则不导出并停止。页面事件记录不等于独立GUI通过，禁止Agent读源码/DOM/CDP/a11y/本地记录。
5. **crossapp-05 / 原生canvas**：复用现有Windows `--suite drag` 的有向路径和曲线case（其中drag-10至少两次水平反转），真实按住/绘线/释放，按既有首次尝试流程逐例推进，不用click终点冒充绘图。明确这是同一原生fixture应用，不冒充另一外部绘图软件或新的应用数量。
6. **crossapp-06 / modal/menu/两窗口**：复用现有 `--suite focus`任务；所有权仅本轮A/B/D，焦点/菜单/modal/暂停恢复与取消证据按现有task。明确同进程多窗口不等于跨应用软件数量；整套还须有实际Notepad/Calculator/Explorer/browser证据。

## 公共边界

- 每case独立run/case/trial/nonce，前三帧/动作前/动作后/结束截图与工具callID/requestID/observationID需关联；固定坐标、计划行为、Agent自报不算证据。
- GUI Agent只允许8个computer工具，禁止shell/文件/源码/oracle/外部网络/a11y/CDP；准备器、manifest、检查器不得放入Agent cwd或工具资源。Agent任务允许明确已知输入内容，但不得包含机器oracle或私有结果。
- supervisor在每次启动后记录exe绝对路径+hash、PID+creation time、HWND、交互session、标题/可见nonce；未录入时是待预检而非捏造0值/假owned。OS应用无case UI的由自有文档标题/目录标识与监督者绑定补足；Agent不可自行猜进程。
- 任何授权弹窗、未知窗口、遮挡、partial/unknown/cleanup失败、失焦且归属不明则STOP+关闭computer session，不点击安全弹窗、不切换到用户窗口。
- 准备输出必须是全新绝对本地目录；拒绝existing/nonempty、UNC、路径穿越与symlink/junction/reparse逃逸。无文件覆盖、无递归删除、无进程启动/kill、无系统配置写入。失败可留owned部分目录并记录，不删未知资产。
- 文件诊断输出只可称artifact_match/mismatch/unknown/needs_evidence，固定gui_verified=false；不因有六个case就满足每基础动作10次首次有效trial门禁。
- 数值/文本结果、事件记录、文件变化与原始工具trace/截图分别核验；本批检查器不代替全部图像/动作证据关联器。

## 写集

- `acceptance-fixture/crossapp/`：独立Python实现模块、任务模板/离线页资产/说明。
- `scripts/prepare-windows-crossapp.py`、`scripts/check-windows-crossapp-artifacts.py`：薄CLI入口。
- `tests/windows_crossapp_pack.py`（必要独立helper文件）：标准库unittest，无GUI测试。
- `acceptance-fixture/tasks/windows-crossapp.md`：监督者/Agent边界及六例索引。
- 不改src/Cargo、旧C#fixture/旧分析器/旧任务oracle；需接入原任务时复制生成受限任务文本、在supervisor manifest注明来源hash和suite，不能弱化旧scope。

## 执行步骤

- [x] sol先给可执行测试与最小待实现接口，冻结StageA，不实现功能。
- [x] CC+GLM在Windows记录能力缺失RED，保留原始numeric exit和断言；不在Mac执行测试。
- [x] sol按上述契约实现准备器/六任务/离线页面/只读文件检查，source冻结，compile-only。
- [x] CC独立spec→quality review；Windows纯回归与CLI实际prepare/check：初始化结果必须pending/unknown而非GUI通过；显式owned临时样本写入只属于checker自测，不冒充GUI动作。
- [x] 检查资产完整性、隔离/路径安全、旧root/fixture未变；更新验收矩阵与原始证据索引。

GUI授权、物理多屏与真实跨应用首次trial仍待，不因本批结束就缩小完整computer-use目标。


## 本批执行结果（Windows-only）

- StageA：原49测试在Windows首次执行为65个subTest失败、0 error/skip、native1；5个待实现API的RED原始证据保留。见 `.agents/reports/windows-crossapp-stage-a-cc-20260930.md`。
- StageB：gpt-6.1-sol实现并冻结r3；CC＋GLM独立review后在Windows执行原49＋新增13测试，均OK/native0；未改原49断言。实际prepare/check native0，六例均needs_evidence，所有权尚待预检，GUI与动作trial门禁false。
- 两次existing-root prepare均预期拒绝native2；追加hashproof的前后74文件（相对路径/长度/SHA256）及目录清单字节一致。共6个runner、62项unittest；不把预期拒绝计为通过的GUI动作。
- 原始结果：`.agents/runs/windows-crossapp-stage-b-cc-20260930/evidence/`、`hashproof/`；解释与纠正见 `.agents/reports/windows-crossapp-stage-b-cc-20260930.md` §9–10。
- 未运行六例GUI或browser JS，不启动Host/应用、不改网络授权、不碰用户Notepad24332。待用户确认系统授权后，仍需CC＋GLM执行真实桌面验收；多屏与其余完整范围不缩减。
- 冻结边界：上述测试针对r3归档的24文件。验证后仅本计划与总验收矩阵推进文档状态；原r3归档、manifest、远端源及原始证据不改，其余22文件维持冻结匹配。不能将更新后的本地文档声称为原24/24快照。
