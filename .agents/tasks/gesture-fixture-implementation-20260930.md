# 实施：三套可执行原生动作靶场（不缩小完整后续范围）

你是真实Claude CLI实现者，rpa-computer仓库。用户已批准补齐通用computer-use案例/实现。读`.agents/README.md`、`docs/computer-use-acceptance-cases.md`、新实施计划阶段B。无worktree/commit/push；保留已存在修改。禁止真实GUI输入、截图、启动窗口或SSH；可构建及运行严格无GUI self-test。产品原子输入worker在Rust目录并行工作，不触碰其文件。

## 独占写集
`acceptance-fixture/**`（原生fixture、catalog、build脚本、该目录README、任务目录）；新增`scripts/analyze-gesture-gui.py`、新增`tests/gesture_gui_analyzer.py`；`.agents/reports/gesture-fixture-implementation-20260930.md`以及专用`.agents/runs/gesture-fixture-*`。
禁止改src/**、crates/**、Cargo文件、现有text分析器/测试、CONTRACT、根设计/计划/用例规范。源码/测试直接修改，不只是写建议。

## 本轮交付
新增`multiclick`、`drag`、`scroll`三个独立suite，每个10个唯一case，Mac AppKit与Windows WinForms同一语义catalog。旧legacy和四套text case原样保留。本任务没有完成pointer/keyboard/cancel/focus/geometry、多屏或跨应用suite，报告必须明确剩余，不能把30例当全部范围完成。

- 新模块承载case状态/纯判定与自有view/form，现有main只路由接线；单文件不得无理由1000+。C#支持现有.NET Framework csc/C#5，不引第三方GUI库。Mac保持主屏布局、边界clip、当前已修acceptsFirstMouse/owned raise约定；未请求--coordinator-raise不擅自反复抢前台。
- 页面清晰可读：suite、caseID、trial、nonce、自然语言操作要求、大目标区域/进度/结果、Check/Next/Finish。随机布局不泄露agent坐标，不把oracle路径/UTF16/答案调试数据画给agent。控件间不重叠，不遮挡标题。
- app-only oracle记录session/run/suite/caseID/nonce、开始时间、配置expected语义、真实OS原始事件+归一事件、自有控件可见状态、首次check/最终check。只记录自有窗口，不全局录键。明确事件来源；不能按工具成功自动MATCHED。
- 多击记录target内真实down/up与原生clickCount。Mac可要求native count1..N；Windows WM_*DBLCLK是特定消息，官方序列DOWN,UP,DBLCLK,UP，不得假定第三下MouseEventArgs.Clicks一定3或把第二下Clicks2算两个DOWN。Windows按真实down/up个数+双击消息+可见目标状态验证；保留原始字段并明确平台expected，不假造count3。超时/位置变化/区域外事件不能误计成成功。每case reset真实计数，Click Check/Next不能记入target。
- drag：实际down→held motion→up，验证起止命中、方向/可见轨迹/位置、释放状态；事件可合并，不能要求每个计划采样点都一定收到，也不能仅看到终点就算拖拽成功。覆盖水平/垂直/折线/短长距离等10例，要求/阈值明确。
- scroll：同页两panel/可见offset编号。覆盖上下左右双轴、不同位置/幅度与边界行为；native delta与contract正右/下分别记录，零量/非法参数不伪装滚动通过。至少所有正向case有可见实际位移证据，记录未变化/错误panel。Windows横向WM_MOUSEHWHEEL、MacscrollWheel都真实接线。
- 各平台native表述不同是显式expected，不normalize away actual，也不污染旧textknown09期望。`--self-test`、`--export-cases`绝不创建窗口或post事件；覆盖catalog个数/ID、纯状态机负例/边界/JSON转义/错误顺序等。
- 扩展build脚本包含新模块、不覆盖旧产物；export/parity检查保留旧33case并增加新cases。Windows实际csc由协调者copy后跑，你只审查编译可行性。

## 任务与分析器
- `acceptance-fixture/tasks/{multiclick,drag,scroll}.md`：真实GLM只使用现有8computer工具，禁止shell/files/a11y/CDP；读截图操作指定case；每例只首次动作/Check一次，失败保留，不靠重试凑成功；Next至SUITE COMPLETE；关闭computer session但留fixture。
- 新gesture分析器独立模块，不改现有text分析器；校验真实model标签、唯一case、时间/nonce/事件关联。工具参数关联无足够证据就unknown，不能按第N次工具调用对第N条check。first/final分开；planned/trial不是通过。工具政策审计仍由已有审计脚本单独做，截图还由协调者实际看。
- 分析器pure测试覆盖空/不完整/错nonce/重复check/未归属事件/伪case/result无终态等负例，不将partial run显示100%或全部完成。

## TDD和构建
先记录有意义的纯状态机或分析器fail-first，再实现。无窗口Mac构建/self-test、Python纯测试、case导出及文档状态准确性。不要运行任何已编译GUI正常入口，不启动LLM GUI agent；没有实际Windows编译/GUI时如实留待验证。
最终报告改动文件、命令/日志/结果、平台语义、缺项；不能自称30case已通过真实GUI。
