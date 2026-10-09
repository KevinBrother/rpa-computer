# Windows B2 原生靶场：pointer + keyboard（完整矩阵的有界下一批）

实现者Codex+gpt-6.1-sol；所有测试/GUI由CC+GLM执行。独占写集 acceptance-fixture/windows/**、acceptance-fixture/{build-windows,start-windows}.ps1、acceptance-fixture/tasks/windows-{pointer,keyboard}.md、acceptance-fixture/README.md、scripts/analyze-basic-input-gui.py、tests/basic_input_gui_analyzer.py、本report .agents/reports/windows-basic-fixture-sol-20260930.md。不改Mac/Linux源码/构建/测试、rootRust、旧gesture/text分析器及其测试、其它crates。旧Windows手势产物已固定独立部署，本次不能覆盖旧bin、oracle或首轮失败。无worktree/commit/push/reset，禁止SSH/GUI/截图/测试执行/启动其他agent，必要compile-only允许。

用户已批准完整动作cases见 docs/computer-use-acceptance-cases.md pointer01..10、keyboard01..10与完整设计第3节。先读真实tool schema/runtime计划（只读）对齐已有8computer API参数/键名、observations约束，不想象不存在功能。保留所有legacy+4text+3gesture源码语义及原--export-cases 63条；Windows新两suite单独明确--export-platform-cases manifest并带platform=windows（不要给Mac旧export伪加两套或破坏旧63case parity）。未来Mac实现这些suite仍pending，这不是改变总目标。

实现：
- 两套每套10个固定caseID，独立nonce/trial、check-once/next/end，清楚易读原生WinForms测试UI。模块拆小，MainForm入口只路由，不新增1000行大类。
- pointer: 真实own HWND鼠标move/down/up+button/nativeMessage/raw coordinates+timestamp，中心/边缘/小目标/右键自有context menu/中键/非激活自有第二窗首次点击。拒绝例必须工具证据明确拒绝+own target event0；不能event0自己就判PASS。缩小观察图需求依赖actual MCP observed size，app不可假设API已缩放。Padding feature当前root尚未接时该case明确blocked/needs_capability，不跳成passed，不凑有效输入数；case本身完整规范和后续判定必须有。
- keyboard: 自有focus文本/选择状态，真正WM_KEYDOWN/UP、SYSKEY、CHAR等记录，按键/Enter Tab/CtrlA/Shift选择/可安全捕获的Alt组合/多modifier/短长hold/modifier释放后普通输入/非法键全拒绝。只监听本进程own窗口，不装全局键盘钩子/不查询或保存其他应用键盘内容。Alt不能引导agent按系统危险快捷键；own有界菜单/组合。在正确focus的编辑控件上记录真实selection/text/native events；hold必须原生down/up时间证据，不能只凭没有字符重复称release成功。
- app-only证据包含run/session/suite/caseID/nonce/trial/spec/platformExpected、UTC毫秒与原生相对时间、event index、check(first/final)、可见结果。Check/Next/Finish管理操作不作为目标输入成功事件，明确记录区域/操作边界。不伪造OS events，不在check时补发键鼠。
- 新独立离线analyzer只处理这两个suite；沿用真实call/result时间包络+唯一关联/unknown原则，不按第N配第N，不递归/输出图片；固定gui_verified=false、验证actual model glm-5.3-flash，partial/blocked/first/final/retry/valid_input分开。可以只读复用旧分析器通用解析函数，不改旧SPECS/旧结果；若复用导致耦合不安全，解释小范围独立代码而非大复制。
- 新CC任务只8computer工具，明确首帧目标窗口缺失/系统弹窗就STOP close、禁止AltTab/WinKey/Explorer绕路；只操作这两套own窗口。首次尝试/Check一次，不能重试凑分，拒绝/blocked不算10次有效输入。只模板不运行。
- 补C#5纯SelfTest与新Python analyzer测试源码，覆盖missing/raw mismatch/wrongcase/nonce/timestamp/重复first/错误focus/held不释放/伪工具成功/blocked不升级/非法suite & args等。保证 --self-test / export分支在任何GUI初始化前返回。旧89selftest保留，不弱化旧断言。可将现有Main未知参数静默忽略问题修为独立纯Arguments parser/清晰exit1，并加入异常/缺值/重复参数测试源码；保留默认legacy与既有合法入口，不为此改其他平台。
- 不在本任务声称完成cancel/focus/geometry整套、multi/crossapp/全部有效trial10次门禁。记录尚待实现部分，不能把只提供case名字当实现完成。

只做必要C#5语法编译/Python语法编译，不执行测试/程序集/GUI。报告源码冻结、精确变更/编译/待CCWindows命令、runnable与blocked能力列表、原生消息判定边界。不碰已部署可执行文件或旧raw证据。
