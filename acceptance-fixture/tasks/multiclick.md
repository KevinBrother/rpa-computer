# GLM 原生 multiclick 靶场：一次首次尝试，十个唯一 case

协调者先启动原生 `--suite multiclick`，在 source-free 发布目录运行真实 Claude Code + GLM。
你是视觉验收者，不是源码实现者；响应 model 标签由协调者从实际转录验证，不能仅称自己是 GLM。

## 工具与边界

只允许现有 8 个 computer 工具：`computer_describe`、`computer_open`、`computer_observe`、
`computer_step`、`computer_get_step`、`computer_pause`、`computer_resume`、`computer_close`。
禁止 shell、文件读写、源码、oracle、导出清单、a11y/UIA、CDP、浏览器/其他 MCP 工具。
只能通过截图读取「Computer Use Acceptance」窗口；坐标只能从本次截图推导，不接收预制坐标。
不要操作其他应用、打开文件、关闭 fixture 或更改显示器设置。

## 流程

1. Describe → Open → Observe。只操作这一个应用；若不可见或被遮挡，停下报告，由协调者准备环境。
2. 对每个新 case，复述截图上的 **suite、caseID、trial、nonce、操作要求**；未知字符明确报告，不猜。
3. 只尝试一遍完整要求。单/双/三击分别用 `kind:"click"` 的 `count:1/2/3`；
   多击一次调用，不用分散调用冒充系统多击。A→B、左→右、左/右/左是要求的步骤，不是重试。
   慢速两个单击用一次单击后 `computer_observe(wait_ms:2100)` 再一次单击，不能靠 shell 睡眠。
   `multiclick-10` 尝试一次 `count:0`，记录 API 的拒绝，禁止回退为合法点击。
4. 使用最新 observation_id 作为 based_on、每个逻辑动作唯一 request_id；查结果用 GetStep，
   不因结果不确定重放/重试输入。Windows 第三击 count 不保证 3；报告可见原生选择，不把它改成预期。
5. 点击 **Check 恰好一次**，Observe，复述 MATCHED/MISMATCH 和可见结果。失败原样保留，不补点。
6. 点击 Next（最后一例 Finish）并 Observe，直到 **SUITE COMPLETE**。只表示遍历完成，不是成功10/10。
   绝不回绕或重启来凑通过率；未看到终态就报告 partial。
7. `computer_close` 关闭 computer session，但保留 fixture 窗口；列每例首次状态和任何未完成/unknown。

截图核验和工具政策审计由协调者另做，模型自述、派发成功、Check按钮收到点击都不是目标通过证据。
本套保留旧 catalog 的 slow_two/reset 流程；执行中监督者取消的 partial/release 不是这个任务的覆盖项。
