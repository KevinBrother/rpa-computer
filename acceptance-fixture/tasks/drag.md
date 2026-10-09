# GLM 原生 drag 靶场：十个唯一 case，不靠重试

协调者启动 `--suite drag`，真实 Claude Code + GLM 只在 source-free 发布环境做视觉验收。
只允许 `computer_describe/open/observe/step/get_step/pause/resume/close` 这 8 个 computer 工具。
禁止 shell、文件、源码、oracle、export、a11y/UIA、CDP 或任何额外工具。只操作 fixture 自有窗口。

1. Describe → Open → Observe；若窗口被遮挡/不可见，停下，不操作其他应用。
2. 每例先复述 screenshot 上的 suite/caseID/trial/NONCE/要求，从当前截图推导 START、END、WAYPOINT。
3. 只进行一次 `kind:"drag"` 的完整动作（left、path、duration_ms；最新 based_on，唯一 request_id）。
   必须实际按下→按住运动→释放，不可用 move 再 click 终点代替。
   水平/垂直方向按要求；折线经 WAYPOINT；短距离仍有 held motion；长距离 case 保持至少 0.75 秒
   （可设 duration_ms:1000）；边缘释放在 END 内距边缘不超过45px且距中心至少40px。
   拖选 native sentence 的 quick brown。曲线 case 要有至少两次明显的水平运动方向反转，
   不是直线；不要要求系统为每个计划 path 点都回送一个事件。
4. GetStep 只取已有结果，不能重试输入；对 partial/unknown/cleanup错误停下、Observe并保留失败。
5. 点击 Check **一次**，Observe并复述真实状态。失败保留，禁止第二次拖动来升级“首次成功”。
6. Next/最后的 Finish → Observe，顺序做完十个不同 case。只有看到 SUITE COMPLETE 才说遍历完成，
   不把 visited10 当 matched10。未完成就报告 partial。
7. Close computer session、保留 fixture；逐例首次结果及未知/失败原样报告。

本套仅自有窗口/主屏；多屏、跨应用、取消监督者、输入故障恢复不在此任务里冒充通过。
原始 OS 事件/应用 oracle 和关键截图由协调者独立核验，派发成功不等于拖拽成功。
