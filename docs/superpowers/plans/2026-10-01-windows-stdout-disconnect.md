# Windows stdout 断开验证（已批准取消/transport范围的续项）

本批只验证 Windows；不启动 macOS/Linux 测试，不重启仍有授权阻碍的 GUI/Host，不改用户权限、防火墙、模型配置或桌面。代码/测试代码由 gpt-6.1-sol 编写与 compile-only；真实执行及独立审查由 Claude CLI + GLM。原位工作，不 worktree/commit/push。

## 最小契约

- 使用真实 Windows anonymous pipes + 生产 `stdio::run`/`Worker`，backend只记录模拟输入；不调用 `handle_eof` 替代真实 transport。
- 父收到 open/observe 或 mock Press 的确认后关闭全部 stdout 读句柄；stdin保持打开直到子进程结束，排除输入EOF触发cancel造成假阳性。
- 空闲用例关闭输出后发送 ping，主动触发下一次生产 write。活动用例在已接收Press的 hold 内关闭输出；不声称没有任何write时就能立即发现输出断开。
- 观察 transport 返回与 caller `Worker::shutdown` 两个阶段。不能将stdin EOF专有的 reader即时cancel断言无条件复制到stdout路径；退出与held清理必须有真实证据。
- 父stdout读线程须明确 close/join/ACK，不能仅drop另一个复制handle；成功路径不得kill子进程。所有等待有界、异常只清理精确owned child。
- 保留原4项EOF语义/原始结果；新增用例不视为TCP/真实按键/GUI通过。

## 流程

- [x] sol先完成测试规格、测试代码、compile-only及源码/产物冻结；先不改生产代码。
- [x] CC独立静态规格/质量审查，检查真实关闭所有read handles且不以stdinEOF完成。
- [x] CC在Windows执行新用例及受影响旧EOF回归，保存numeric nativeexit、stdout/stderr、identity/nonce/hash、terminal/parent诊断。
- [x] 若出现真实首败，保留证据，RCA后才授权最小修复；不能放宽oracle/反复重跑碰绿。
- [x] 协调亲读原始结果并更新验收矩阵。缺乏即时断输出检测需明确记为能力边界，而非藏进“全通过”。

## 本批结果

首次Windows idle native101：测试父reader关闭2s超时，未触及产品断输出路径。匹配rustc源码确认测试使用的异步pipe与同步取消API不匹配；仅修io_threads.rs为唯一owner读取available并协作stop，未放宽超时/oracle/改生产。CC修后两新+四旧6/6、四runner native0、无超时；协调核对原始终态和源码hash。原RED保留，报告windows-stdout-disconnect-retest-cc-20261001.md。

勾选仅代表本批真实pipe+mock Backend验证，不代表完整computer验收；GUI/OS释放/TCP/完整Host入口/多屏/跨应用/可选反馈/有效trial仍待。
