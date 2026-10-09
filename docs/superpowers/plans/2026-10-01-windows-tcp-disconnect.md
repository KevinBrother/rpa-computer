# Windows TCP断连取消集成验证计划

> 已批准完整范围的cancel-07续项，不替代GUI或remote-17完整产品入口验收。代码与测试源码由gpt-6.1-sol实现；独立review与实际Windows运行仅CC＋GLM。无worktree/commit/push。

**Goal:** 补充真实TLS/TCP回环断连→生产remote supervisor/pump/stdio reader→取消→模拟held释放→owned child终止及后续连接的证据。

**Architecture:** 独立Windows test-only executable调用现有公开remote::host::run；生产current_exe子进程派生仍走真实代码，但该测试exe的stdio模式只能构造Test Backend。既有computer-host.exe、DesktopBackend、GUI/反馈renderer、实际OS键鼠均不启动。只可监听127.0.0.1临时端口，不用0.0.0.0/LAN/公网、隧道或系统权限变更。此测试不是换路径恢复被阻挡的桌面Host，而是全隔离模拟输入测试；不得据此声称真实Host网络授权已解决。CC独立审查隔离是否成立；若不成立或出现授权要求则停止，不点击/改规则。

**Tech Stack:** 现有Rust/rustls、生产remote与stdio/Worker、真实Windows socket与子进程，复用只读测试证书及可复用helper。Windows cross-compile仅编译，本机不执行测试。

## 本批写集与边界

- 新增 tests/windows_tcp_disconnect.rs 及 tests/windows_tcp_disconnect/ 下小模块。
- 新增 examples/windows_tcp_disconnect_fixture.rs 及同名子目录。
- 新增独立报告/冻结证据；原7个stdio源码、177项root冻结、现有transport/tests/证书/runner只读。
- 不改Cargo、src或产品语义；如发现生产缺陷，保留首败，先提出RCA/最小修复方案，不直接改断言或延长deadline。
- 可复用既有测试helper，禁止复制大块文件造成双维护；必要适配限定新增模块，依赖纳入冻结。

## 场景与验收

1. 已认证idle连接真实关闭socket；stdio reader观察EOF，worker自然清理/子进程native0，supervisor可重用。
2. 已派发key_hold真实收到模拟Press诊断后，关闭实际TCP socket（不发送computer_pause/close，不置cancel flag，不关闭supervisor控制stdin）；要求在hold未自然结束前取消，准确模拟Release/held空/partial语义，禁止把正常hold完成冒充取消。
3. 同样active场景通过TLS close_notify断开，区分TLS关闭与裸TCP EOF；保持诊断、promptness和清理断言。
4. partial-frame TCP断开路径（明确生产实现会否补newline/丢弃，不臆造协议承诺），证明不会执行残缺输入、已held输入正确清理；若与已批准行为冲突则提出问题，不改生产掩盖。
5. 至少上述active场景在子进程自然退出/生产reap证据后，再认证连接获取新child PID/创建身份并执行initialize/open/observe/close，证实非旧worker复用。无真实OS输入。

每项保留真实连接标识、run nonce、fixture/build/exe SHA、父子PID/创建时间/session、dispatch/关闭/取消/释放的单调时钟、子进程终态和supervisor收尾。不要求已断开的socket收到step reply；以child内部真实终态判定，不用自己写success flag代替。秘密token不写日志。

父进程只在以上证据已收集后用独立控制通道请求test supervisor退出。正常通过不得强杀；失败RAII仅精确owned句柄，硬deadline保护子进程，无法证明清理则unknown。不得全局/name kill；保护用户Notepad24332。线程有界join、日志有界、无无限read/全靠sleep同步。测试端自己drain完整不冒充所有生产线程显式joined。

## 执行步骤

- [x] sol实现新增测试与test-only支撑，compile-only、冻结原始源码/依赖与compiler返回的精确Windows artifacts；不SSH、不运行。
- [x] CC＋GLM独立spec/quality审查，尤其mock-only/loopback/无桌面、生命周期和断言不被supervisor stop污染；发现阻碍则返回作者。
- [x] CC用冻结runner在NEW Windows TEMP运行--list及各场景，首次native失败立即停止、保留日志；禁止重跑碰绿。纯console可Session0。
- 不适用：本批四例首次native执行均通过，无运行首败需要RCA；编译首错/包装器错误另行保留，不伪造RED。
- [x] 协调核对实际模型标签、summary/native code、终态/时序/源hash；更新矩阵，保持OS held释放、完整Host入口与GUI验收pending。


## 编译交接（2026-09-30 UTC；文件名保留跨时区批次标记）

sol已冻结10份新增源码、153份依赖和两个Windows PE；协调已逐项核对source/dependency/artifact SHA，最终integration-3与fixture-3编译exit0。首轮fixture E0277与修正前源码保留；这不是Windows运行失败，也没有运行通过结论。作者已关闭；CC＋GLM最终review及逐例运行已交给exec99615，终态另行记录。

精确清单：idle_tcp_eof_reaps_child_and_reconnects、active_tcp_eof_cancels_hold_and_reconnects、active_tls_close_notify_cancels_hold_and_reconnects、incomplete_json_tls_close_cancels_hold_without_dispatch。第四项覆盖TLS正常关闭时真正不完整JSON的尾帧，不声称覆盖全部网络故障组合。


## 本批终态

exec99615 terminal0 / actual glm-5.3-flash。独立review后Windows四例各1 passed/native0（1.28/1.01/1.01/1.02s），--list4另native0，无timeout、输出完整。协调核对原始runner、4套parent/8worker终态、源/PE冻结后更新取消矩阵及总矩阵。原编译错误和包装器错误仍在；不再以“终态另行记录”段落作为当前状态。详细范围校正见CC报告§6，原始数据目录 `windows-tcp-disconnect-cc-20261001`。

全部本批actor均终态/关闭。此计划完成仅指回环真实传输＋模拟Backend取消，不代表完整goal或GUI验收完成；继续保留LAN授权、OS释放、Host main、GUI/多屏/跨应用和每基础动作10次首次有效trial门禁。
