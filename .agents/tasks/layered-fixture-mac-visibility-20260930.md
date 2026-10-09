继续fixture线程：只可写 acceptance-fixture/**、.agents/runs/layered-fixture-* 与你报告。禁止真实鼠标键盘输入、修改系统设置或操作用户窗口、不commit/push。协调者已完成Windows真实编译，out-fixed SHA77FABB9042A24D98E22AEE4960C45C98A30CE9885DCF811E7A0C09CF34580CAC。现在定位macOS fixture可见性：

新的baseline fixture PID66565（exe /private/tmp/computer-layered-mac-fixture-20260930/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance），正常live且有trial oracle，backdrop PID66608执行后真实GLM截图在primary macos:2只看到中性空白，fixture没在画面中。截图 .agents/runs/layered-mac-newfixture-first-20260930.png；日志 .agents/runs/layered-mac-newfixture-visibility-20260930.log。本机两显示器online/active，CGMainDisplayID=2，无锁屏。怀疑window.center()落到另一display，但不要未经证据直接改。

请先在 .agents/runs/layered-fixture-mac-window-probe-20260930.py 写并运行只读CoreGraphics窗口/显示bounds探针（可复用已有native-mac-display-probe的CF基础），只输出PID66565和66608的window owner/bounds/onScreen/layer+各CGdisplay的bounds/main/active；不截图不全局列用户窗口标题，不读取凭据。或用只读Swift系统API也可。报告实际bounds与主display2的关系，确认遮挡、其他display还是其他问题。你不得激活/移动任何现有窗口；只有coord会重启修改后的fixture。

如果确认fixture落在非CG主屏：仅修改fixture新窗口初始定位，使用CGMainDisplayID匹配NSScreen.deviceDescription的NSScreenNumber，选择这个screen.frame居中（用AppKit坐标，不手动猜Y转换）；不修改用户显示器设置，也不改变Host primary screenshot策略。找不到目标屏时诚实失败，不静默成功。增加pure定位helper测试：含负origin/横排/竖排/主screen非列表first，窗口frame完整位于所选screen，或不足fit时报失败不裁切。若是别的原因只做证据支撑的最小fix。

读 .agents/reviews/layered-fixture-20260930.md（若已存在），其中confirmed GUI中性输入域问题也由你修：Windows TextBox AcceptsTab=true/AcceptsReturn=true；Mac NSTextView明确禁automatic quote/dash/text/spelling replacement。实际CRLF返回差异只记录不偷偷normalize ActualText，keep strictUTF16比较。

重新构建唯一 .agents/runs/layered-fixture-build-20260930-c/macos 并self-test/parity（noGUI）。报告命令/hash/probe结果，注明实际可见性待coord截图，不声称完整验收。只做该小范围实现，无长篇重设计。
