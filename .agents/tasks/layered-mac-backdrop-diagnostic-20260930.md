独立bounded诊断/测试helper任务，不修改fixture/src，不GUI输入，不commit/push。可写 .agents/runs/layered-backdrop-* 和 .agents/reports/layered-backdrop-20260930.md。coord当前新fixture PID73685，精确exe /private/tmp/computer-layered-mac-fixture-20260930-c/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance；背景helper PID73709，exe .agents/runs/native-gui-helpers-20260929/macos-backdrop。CG主display2 bounds 0,0,1920,1080；旧fixture次屏x2430原因已修，当前fixture启用匹配CGMainDisplayID的NSScreen.frame居中；自检46pass。新的真实GLM screenshot .agents/runs/layered-mac-c-first-frame-20260930.png 只有中性空白+fixture菜单，GLM仅凭菜单错误说fixture可见（coord已看图，窗口不可见）。不能据模型宣称通过。

先复制现有只读probe .agents/runs/layered-fixture-mac-window-probe-20260930.py 为你范围内的新probe，加 --pids 和 --output 参数（不要修改原probe），输出目标窗口bounds、onScreen、layer、alpha及CGWindowList数组中的stacking index，不输出无关用户标题。同样输出CG显示bounds，以及用只读Swift/系统API查询NSScreen screens的frame/deviceDescription显示ID（不创建NSApp窗口）。只读运行证明：当前fixture是否实际落到另屏/是否在background后面/是否窗口没在当前Space；不要一上来改helper。

若确定只是backdrop遮挡，改你范围内新helper（从 .agents/runs/native-gui-helpers-20260929/macos-backdrop.swift 复制）实现安全的owned-helper和owned-fixture激活顺序让fixture在中性background前，保留PID+exe校验、20min上限/退出fixturewatchdog/SIGTERM；只能激活这两个owned应用，不用AppleScript/AX/键鼠/调用其他app。已有helper BOTH fixtureApp.activate() + fixtureApp.activate(options:[.activateAllWindows])；如果目标已active这两者并未实际raise，试先让自己helper成为active再通过fixture activateAllWindows。不能浮动topmost遮挡后续Calculator，不能修改用户窗口或系统权限。仅编译；由coord启动新helper，无GUI worker擅自运行。

若是NSScreen/CG坐标主屏定位偏差，报告准确frame/bounds反例，不改fixture（writer持有），coord会把证据给writer；不要写没必要的helper改动。

结果短报告到指定路径，列出根因、commands/outputs、是否需要修主线程/置前/坐标。不长篇重设计。
