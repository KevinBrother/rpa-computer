# Windows geometry fixture 与真实 stdio EOF 验证实施细化

> 执行已批准 complete-computer-actions 的 geometry01..10 与 cancel06 剩余项，不缩小其余范围。源码/测试源码/构建实现由 gpt-6.1-sol；真实测试和独立review由CC+GLM。仅Windows，不worktree/commit/push。

## 设计与证据边界

Geometry选择独立原生夹具+离线关联分析器，不给输入内核混入验收UI，不用固定假数据冒充真实DPI/显示器。保持旧canonical63、basic20、focus10及native-text策略不变，新导出独立geometry10。夹具只记录自有窗口/目标的坐标、DPI、原始鼠标消息、layout generation、唯一trial/nonce及可见结果。

- geometry01/02/03/04：真实观察的原始/1920/1440/等比约束与目标输入关联；必须结合tool observation、真实编码图像尺寸和app事件。图像元数据自比不能证明PNG尺寸一致。
- geometry05：高DPI必须有真实平台DPI证据，否则needs_environment，不能UI缩放模拟通过。
- geometry06：负origin是纯映射用例，独立声明not_gui，不凑有效GUI trial。
- geometry07/08：分辨率/DPI外部变化旧观察拒绝；不自动改用户系统配置。没有人工环境变化/前后topology证据则needs_environment；不能只移/resize夹具冒充系统变化。
- geometry09：自有target移除是应用语义变化，不一定使runtime desktop topology过期。不得伪称内核自动识别目标消失；验证观察后Agent安全停止/不盲点，需要完整tool trace和移除后心跳，证据不足unknown。
- geometry10：离线校验实际返回PNG IHDR宽高与observation，不能以工具JSON字段互相吻合代替字节解析；分析器仅监督者可见，不回流GUI agent。

保留基础动作首次有效10trial门禁，新suite10个语义case不自动满足。所有GUI未实测即false。明确哪些场景需用户准备DPI/布局。

EOF选择独立test-only child fixture +父测试真实匿名stdin/stdout管道；child构建真实Worker(Test Backend)并调用公开生产stdio::run，不直调handle_eof冒充reader EOF。父进程等待Backend已接受synthetic Press后关闭自己的ChildStdin；子进程只能由生产reader接收到EOF触发cancel/shutdown，不靠父cancel或故意设置全局flag。

要求：真实协议初始化/open/observe/step请求链；单独诊断通道（stderr或owned file）记录真实派发、cancel/cleanup/held/quit，不能污染stdout JSON-RPC；有界wait/read/drain/精确owned child清理；断言取消发生在hold期间、已派发结果不误报not_started/完整成功、释放调用及held状态、worker/stdout收尾。如生产EOF策略不保证未发送的step reply，不能把reply缺失单独当错误，须区分可见reply与本地权威终态。覆盖idle EOF、active hold EOF、partial-frame EOF、stdout断开（若现实现可安全覆盖；不是TCP cancel07）。所有held证据为mock，不能冒充真实OS按键释放。禁止Host/DesktopBackend/窗口/网络监听/真实输入，纯console子进程可Session0。

## 写集与流程

- Geometry owner：Windows Geometry*.cs、BasicArguments.cs/MainForm.cs/build-windows.ps1最小接线；独立scripts/analyze-geometry-gui.py、tests/geometry_gui_analyzer.py、tasks/windows-geometry.md。冻结Rust/旧oracle/其他suite实现只读。
- EOF owner：新tests/windows_stdio_eof.rs及专用support，必要examples/windows_stdio_eof_fixture.rs与examples独立helper（不能污染默认product bins）；原则上不改产品。若发现真实生产缺陷，保留RED并报告协调者后最小授权修复。
- [x] Geometry StageA先加parser/export需求的纯回归，保留237项；CC在Windows记录预期RED后再做StageB。
- [x] EOF先编写真实管道测试与可审查fixture；对已有生产路径增加覆盖，无需制造产品修改或假RED；任何实际FAIL按RCA处理。
- [x] 两owner compile-only、冻结source/exe hash；/Volumes/doc空间小，编译target用/tmp独占，不删除旧证据。
- [x] CC分别独立review、Windows csc/Rust test exe运行；准确数值nativeexit、first failure、hash/timeout/owned process身份归档。
- [x] 受影响回归、更新矩阵。GUI与物理多屏不因此通过，完整goal保持未完成。

## 本批证据与剩余边界

Geometry StageA 237PASS/2FAIL已留存；StageB default Windowscsc0、自测287/0、新分析器78及关联回归通过，见windows-geometry-green-cc-20260930.md。EOF真实子进程4/4native0，见windows-stdio-eof-cc-20260930.md。当前Windows产品release2bin已构建但未部署。

本流程勾选仅代表本批实现/纯验证完成。stdout断开未覆盖，TCP cancel07、所有真实GUI/OS held释放、multi12/crossapp6、有效trial门禁、反馈实测仍未完成；不声称完整goal实现。
