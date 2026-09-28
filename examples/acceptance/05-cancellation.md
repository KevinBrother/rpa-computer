# 验收任务：运行中取消（hold 期间取消）

**用途**：证明停止路径有效：取消后不再发送新的任务输入，且清理结果如实上报。

## 给执行 Agent 的任务

1. `computer_open` + `computer_observe`。
2. 启动一次 `computer_step`，动作为 `key_hold`（按键自选，如 `shift`，`duration_ms` 取 4000–5000）。
3. **协调者**会在 hold 执行期间通过 MCP cancellation notification / 本机停止入口发起取消（这步不是 Agent 自己做的）。
4. 取消后尝试再执行一个 `move` 动作，报告该请求的结果（应被拒绝/标记取消）。
5. `computer_observe` 观察并描述画面；`computer_close` 结束，报告 close 返回的清理结果。

## 硬性约束

- 只使用 `computer_*` 工具；不打开源码、终端、浏览器。
- 如实转述每个工具返回的 `input_outcome` / `cleanup_outcome` / 错误码，不得把失败描述成成功。

## 协调者核对方式

- 检查服务端/工具轨迹：取消信号后无新的任务输入事件（清理性 release 除外）。
- 取消后的新输入请求被拒绝；`cleanup_outcome` 若为 failed/unknown 必须可见，不得包装成 "released"。
- 目标：正常路径下取消信号到达后 1 秒内不再发送新的任务输入。
