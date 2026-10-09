# GLM 原生 scroll 靶场：双 panel、真实位移、十个唯一 case

协调者启动 `--suite scroll`，真实 Claude Code + GLM 在隔离的 source-free 发布目录执行。
只允许 `computer_describe/open/observe/step/get_step/pause/resume/close` 8 个 computer 工具。
禁止 shell、文件、源码、oracle、export、a11y/UIA、CDP、其他工具；不操作其他应用或显示器设置。

1. Describe → Open → Observe，复述每例 suite/caseID/trial/NONCE/要求、Panel A/B 的可见行号与offset。
   从本次截图推导目标panel内位置，不使用预制坐标；看不到窗口则停下报告协调者。
2. 每例只做一次要求的 `kind:"scroll"` 动作。参数为 position、delta_x、delta_y、unit:"wheel_ticks"；
   **正x=向右，正y=向下**。双轴在同一次调用给x/y，不分步作弊。不同幅度按截图要求。
   `scroll-08` 大幅下滚到最大边界；边界未移动不能叫滚动成功。
   `scroll-09` 一次 delta_x:0、delta_y:0，页面必须不动，不把它算滚动输入成功。
   `scroll-10` 一次超限 delta_y:101（unit仍wheel_ticks），记录 API 拒绝，禁止改成合法值补做。
3. 使用最新 based_on、唯一 request_id；GetStep只能取结果，不重放输入；未知/partial/cleanup错误如实停下。
4. 点击 Check **恰好一次**，Observe，复述 MATCHED/MISMATCH、前后可见行号/offset；
   指定B的case必须验证A未动。失败保留，不重滚凑通过；不要把鼠标放对panel当真实位移。
5. Next/最后Finish → Observe，十个不同 case 顺序完成，直到 SUITE COMPLETE。
   visited10不是通过10；缺终态/缺截图/未做的case保持partial/unknown。
6. Close computer session、保留 fixture，逐例报告首次状态；拒绝/零量单列，不计页面滚动成功。

Windows 横向必须依赖真实 WM_MOUSEHWHEEL，Mac 依赖真实 scrollWheel。
原始delta与统一的正右/下语义分开记录；工具成功/模型自述不代替应用oracle和关键截图的独立审核。
