最小Mac fixture交互公平性修复，仅写acceptance-fixture/macos/main.swift、fixtureREADME、你新报告 .agents/reports/layered-mac-firstmouse-20260930.md，输出唯一新-f .agents/runs/layered-fixture-build-20260930-f/macos。不GUI、SSH、信号、委派、commit/push；不要动正在运行的-e二进制，当前baseline允许完整结束保留首次miss。

实际-e baseline正在跑：第一次green click position[1277,473]确在绿圆内（图片 .agents/runs/layered-mac-baseline-e2-first-click-frame-20260930.png），没有HIT/WRONG及oracle hit；case01textcheck成功，其后case02..05同样singleclick均HIT。不是坐标错或所有键鼠失效。ShapesView继承NSView，未override acceptsFirstMouse，只有acceptsFirstResponder。Apple官方NSView acceptsFirstMouse(for:)文档明确default=false、inactivewindow第一mouseDown只激活window而不传view；NSButton等标准控件可接受click-through。当前证据符合这个原因，但未记录当时keyWindow状态，不能把具体窗口inactive当已证明。

设计：让靶场自有ShapesView像可点击标准控件一样接受firstmouse（override acceptsFirstMouse(for:) return true），不改变Host/native库任何输入、不双击重试、不强制其他应用激活、不改floatinglevel。添加只针对自有window的 windowDidBecomeKey/windowDidResignKey日志及raise事件key/active布尔metadata（不含答案），便于下一run验证focus状态。无需新真实window，不单独读取/激活用户应用。

TDD：纯自测实际调用ShapesView.acceptsFirstMouse(for:nil)，先在现状见false导致期望true失败，再override最小修复并跑全self-test/export parity33。不要添加只验证常量的伪测试，禁止真实窗口/输入。当前已有49tests保持，新增至少一个真实API行为回归；编译-f并给hash。文档明确这仅测试fixture click-through行为，不代表所有Mac应用first-click可直接激活控件；真实后续GLM验证由coord完成。旧-e本轮firstnohit与后续成功不覆盖，不给正在运行agent重试，不称全矩阵结束。
