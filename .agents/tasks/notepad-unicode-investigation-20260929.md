# Windows 现代记事本 Unicode 异常调查

状态（2026-09-29）：第一阶段定位已完成，未修改产品输入逻辑，最终根因仍待进一步隔离。

报告：`.agents/reports/windows-unicode-diagnosis-20260929.md`。
原始证据 `.agents/reports/glm-gui-actions-20260929.md` 及 `.agents/runs/glm-notepad-20260929/` 保留。

本轮固定MCP输入17例，15例匹配、2例真实文档损坏；独立C#原生SendInput18例，11例匹配、7例损坏。长文本原生batch可复现重复末尾字符，逐字符10ms也存在漏字；不含emoji/换行的原生样例同样失败。模型识图、MCP/Runtime/Enigo均不是产生此类现象的必要条件；Windows/IME/应用具体责任尚未区分。

后续：同条件标准WinForms Edit/Notepad配对对照；采样目标线程键盘布局、IME、修饰键和控件焦点，增加多时点文本回读；原生事件构造与目标消费的证据需区分。单独为enigo代理对keyup、换行事件构造写回归，禁止将其直接认定为BMP样例根因。

仅GLM实现/审查，协调者串行安排GUI；禁止剪贴板替代掩盖text_input错误。不得盲加延时或重试后宣布修复。独立Enter动作modifiers=[]被拒绝为另一问题，不能混合归因。

14:22:17 CST已验证自有记事本文档不保存关闭、本轮Host/背景/计划任务和8399监听清理；下轮实验须重新核验桌面与所有权，不复用旧PID。
