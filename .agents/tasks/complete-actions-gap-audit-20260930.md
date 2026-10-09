# 只读动作完整性审计（设计阶段，不实施）

你是真实 Claude CLI 审查者。工作目录 rpa-computer；用户要求补齐通用 computer-use 动作、跨应用和多屏的案例与实现。当前是设计阶段：不要修改产品源码、测试、脚本、构建配置或用户设置，不运行GUI/鼠标键盘/截屏，不连接Windows。唯一写入路径 `.agents/reviews/complete-actions-gap-audit-20260930.md`。

独立检查：
1. click count1/2/3从Action到Plan/InputEvent到native driver是否保留正确语义，特别Mac当前CGEvent click_state=1；区分已证实代码缺口和需要真机确认的行为。提出最小且可抽取native-input抽象的设计，不为保兼容留下假支持。
2. drag原生按住/dragged事件/释放是否完整；scroll双轴符号、事件单位、溢出/超大输入取消与清理边界；key_hold/chord取消不会遗留输入。
3. 当前多屏capture target选择、input坐标/DPI、geometry freshness；分别列已有和未实现，不把主屏不同DPI测试当完整多屏。
4. 当前纯测试和真实GUI证据各覆盖了什么；指出要创建的确定性fixture vs真实应用用例，不用纯测试冒充GUI。
5. 实施写入分工：native-input、runtime/backend geometry、fixture三者如何分解避免同文件并行写和1000+行大文件。不要开展完整实现。

报告必须包含准确文件路径/行号、问题优先级、已验证/推测标识、建议测试门禁，以及明确目前Windows四suite通过率和Mac尚未完成项来自原始报告（不可洗白）。旧证据不改；禁止commit/push/worktree。报告中文；不输出凭据、个人屏幕内容。
