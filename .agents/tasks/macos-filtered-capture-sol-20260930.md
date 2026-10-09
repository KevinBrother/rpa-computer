# macOS反馈层截图排除：独立ScreenCaptureKit适配库实现

用户已确认反馈层不得进入提供给模型的截图；当前renderer诚实报告unsupported，不能把这个缺口作为永久完成状态。目标是以原生过滤实现功能，不改图、不hide-then-capture，不依赖NSWindow.sharingType假保证。

工作目录本项目；用户最新要求Codex+gpt-6.1-sol写代码，CC+GLM执行测试。源码/测试代码可写，开发compile check可做，不执行测试、不截图、不启动GUI/Host、不SSH、不改TCC、不调用其它agent、无worktree/commit/push。直接编辑独占目录 新 `crates/macos-capture/**` 和专用报告 `.agents/reports/macos-filtered-capture-sol-20260930.md`；**不要改root Cargo/src、native-input、feedback库或renderer**，另有Host集成owner。

## 技术路线

- 新独立workspace Rust库，目标macOS原生ScreenCaptureKit，使用维护者官方objc2/objc2-screen-capture-kit等安全声明；查本机SDK和Apple primary文档核实API，不自己臆造选择器/所有权。
- Apple SCContentFilter的display+excludingApplications+exceptingWindows用于排除Host实际owned renderer的PID所属应用，SCScreenshotManager按filter抓一帧。最小系统版本由真实API availability明确，例如macOS14+；旧系统显式unsupported，不静默退回包含UI的截图。
- 不引入输入/LLM/网络/renderer依赖；普通off截图不受新库启动/权限影响。
- API建议：`CaptureClient::new()`无截图/不弹权限；`capture(&mut self, request: CaptureRequest) -> Result<CapturedImage>`，request包含display_id(u32)、exact excluded_process_id(u32)、width/height(u32)、timeout(Duration)。图片结果含PNG Vec<u8>与实际width/height；可用ImageIO原生encode确保色彩/row-stride/方向正确。若返回RGBA而非PNG必须与协调者锁定接口，不能伪造维度。
- 选定display必须真实存在；只排除明确owned renderer PID，不按名称/窗口标题过滤、不排除整个Agent/用户软件。PID不出现在可伪造Agent请求里（由Host传入）；找不到所需renderer application时fail closed、不要退化无过滤捕获。
- 先screen recording preflight，不擅自请求弹窗/设置权限；permission/no_display/capture_failed/timeout/unsupported要结构化且无敏感app/window信息。
- 异步completion桥接有总deadline（共享内容发现+截图合计），回调所有权安全，timeout以后迟到callback不可访问栈/已释放资源。避免每次timeout后无限派生未完成捕获/线程：每client最多一个flight，超时后直到确认完成前拒绝新请求或sticky poison；不得无限重试。不用每次spawn专属永久线程。
- width/height/总像素预算明确校验，实际PNG dimensions核对，禁止返回名义尺寸。不运行任何实际capture来测试；GUI由CC后续完成。

## 测试源码与交付

编写可由CC执行的pure callback/deadline/late callback/flight门禁、选display/过滤exact PID、missing target、尺寸/像素预算、错误分类测试代码；不能用这些测试证明实际API排除成功。必要seam应是生产采用的逻辑，不平行oracle。

实际cargo check --all-targets等开发编译检查可做，但cargo test/clippy/任何截图留CC。最终报告依赖版本、API摘要、changed文件、编译结果、CC命令与明确GUI待测。root后续由Host owner接线，需把error/availability/PNG契约写README。
