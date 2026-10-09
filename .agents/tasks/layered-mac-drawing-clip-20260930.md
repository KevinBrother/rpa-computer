继续已批准的分层验收，最小修复Mac fixture显示bug。只写 acceptance-fixture/macos/main.swift、必要小型fixture测试（自测优先）、acceptance-fixture/README.md、你自己报告 .agents/reports/layered-mac-drawing-clip-20260930.md。不改Host/产品输入/Windows/cases，禁止GUI操作、启动真实窗口、输入、信号、SSH、commit/push。新产物唯一 .agents/runs/layered-fixture-build-20260930-e/macos。

真实证据：新-d binary启动且--coordinator-raise正常记录事件后，MCP截图 .agents/runs/layered-mac-baseline-d-preflight-frame-20260930.png 能看到自有window主屏、四图形、sampleLabel、输入框、三个按钮，但trialLabel/nonceLabel/instr全部空白，不可进行agent验收。源码这些label先addSubview，然后ShapesView，再sampleLabel等。ShapesView.draw(dirtyRect)直接dirtyRect.fill白色，没有限制到bounds，没有明确clipsToBounds。优先验证假设：较新AppKit默认不clip，绘制越界覆盖前面兄弟labels。不要把假设当已证实，检查实际AppKit行为，写回归先看到fail再最小修复。可以用非live NSView/offscreen绘图或static geometry/self-test，不创建真实window/不改变用户桌面。确保draw白色填充严格限于自身bounds且图形绘制clip，避免遮盖任何相邻view；明确设置NSView clipsToBounds适用性。若另有原因如label颜色/布局，需给证据而非顺手重做UI。

要求header可读并显示当前caseID（例如Trial1/10 — baseline-01 — suite:baseline），nonce/target、HIT/MATCHED/SUITE COMPLETE都必须可见，不降低UTF16校验。现有主屏定位、--coordinator-raise开关、READY日志和自测/export路径都保留。输出产物self-test和catalog parity（跨Windows已有 .agents/runs/layered-cases-manifest-windows-neutral-20260930.json），不要宣称GUI已修复，由coord拍图验证。

顺便只在本次写入范围补报：先前backdrop probe瞬时stacking index落后不能单独否定置前，晚一点实际MCP截图已见window；真正现存问题是header缺失。本次设计是限制自有view绘制边界、明确显示caseID，不改用户窗口或系统配置，不扩展功能。
