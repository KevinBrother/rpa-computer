# Windows预检前模型路由核对 — 2026-10-08

用户确认认证通过后启动的首个外层CC命令请求literal `glm-5.3-flash`，实际assistant.message.model却为`kimi-k3`。协调者发现后终止确切自有进程PID63345；exec96162终态143。转录显示仅本地报告/任务/SSH配置读取，未启动SSH、Host或GUI，不算GLM预检；原始记录保留在runs/windows-reauth-preflight-cc-20261008。

只读检查当前本地Claude settings：sonnet alias的显示映射为glm-5.3-flash；不能以标签替代运行证据。单独无工具单轮 `--model sonnet` probe已完成exec25386 exit0、实际message.model=glm-5.3-flash，原始在runs/windows-reauth-preflight-sonnet-cc-20261008/model-probe.jsonl。未修改全局配置/凭据。随后外层预检exec12857实际模型GLM；远端内层仍需独立验证其实际响应，尚不据本地probe宣称远端模型正确。

此项是GUI前模型路由修正，不是GUI首败重跑；不保证代理对所有未知literal模型的通用行为。
