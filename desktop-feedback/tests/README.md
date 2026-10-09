# 测试交接（仅测试代码，不代表验收完成）

按 2026-09-30 最新分工，Codex 只编写测试并进行编译检查；所有测试执行交给 Claude Code + GLM。

- macOS: `desktop-feedback/build/desktop-feedback-macos --self-test`
- Windows: `desktop-feedback\build\desktop-feedback-windows.exe --self-test`

两者 self-test 在 GUI/DPI 初始化之前分支，只测试纯协议、状态、坐标、样式、有界读取/合并/输出结构，不创建窗口、不请求 ready、不注入输入。
真实焦点、点击穿透、Stop、EOF、心跳、截图排除必须另外实机验收，不能以 self-test 代替。
