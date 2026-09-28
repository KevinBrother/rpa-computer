# 真实隔离 Claude 只读预检 1（非 GUI 成功验收）

日期：2026-09-24 20:30–20:31 CST。

## 执行与隔离
- 真实本机 Claude Code，沿用用户模型 `kimi-k3`；未替换模型/认证。
- source-free release：`/private/tmp/computer-blackbox-win.Wc1fF7`，只包含bridge运行工具、配置、任务和本次证据，不包含Rust/fixture源码。
- `scripts/run-blackbox-claude.sh --print --max-turns 10 --max-seconds 240`：运行前同一sandbox profile执行源码读写负向探针，失败会拒绝启动；实际运行通过此路径。profile输出未单独保存在证据中，不能声称已有独立profile文件指纹证据。
- init 实际工具清单恰好8个computer MCP工具，server `computer` status=connected，无内置Read/Bash或其他MCP。
- token以0600文件供bridge读，模型prompt/工具参数/转录无token值；模型只能看到任务和工具结果。

## 实际轨迹与结果
1. computer_describe：windows能力返回成功。
2. computer_open：session-23296-1，ready。
3. computer_observe：capture_error，GDI capture拒绝访问0x80070005。
4. computer_observe重试一次：相同错误。
5. computer_close：closed，cleanup_outcome=released。

没有computer_step，零鼠标键盘输入，没有图片block。模型明确报告：**未获得截图，未验证视觉能力。**

真实CLI结果is_error=false只表示本次只读预检任务正常结束，不表示GUI截图成功。runner退出0，转录策略审计17checks通过：8tools/5calls/6turns/42472ms。

## 证据
- 完整转录：`/private/tmp/computer-blackbox-win.Wc1fF7/evidence/20260924T123036Z-11945/transcript.jsonl`
- SHA256：`bab37bfc4fddd0e5ad4f4ad2f332753b2bc94cae22ccf79887c4eb5c425a128d`
- runner日志：`.agents/runs/blackbox-windows-preflight-launch.log`
- Host启动：`.agents/runs/blackbox-windows-preflight-start.log`（PID23296，Session1）
- 被测编译快照哈希：`.agents/runs/windows-host-diagnostic-binary-3.sha256`
- 先前同Host版本的协调者只读协议：`.agents/runs/windows-host-diagnostic-protocol-3.json`
- 环境排查：`.agents/runs/windows-capture-access-diagnostic.log`，Session1有LogonUI.exe；用户已被请求手动解锁。未尝试密码输入或旁路。

## 不计入验收成功次数
此run证明真实CLI→隔离MCP→SSH bridge→交互式WindowsHost的工具调用及错误如实处理链路。图片理解、点击、计算器、Unicode输入和10次成功率矩阵全部仍未满足。最终产物仍需全量门禁后重建，此处是诊断快照。
