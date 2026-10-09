# Computer-use 完整验收用例规范

日期：2026-09-30
状态：完整用例规范，分批实施中。Windows 的 multiclick/drag/scroll、pointer/keyboard 夹具已交付并取得对应纯测试证据；focus 10 例夹具已交付并通过纯测试；取消契约9项已在修复后产物复验，另有4项真实管道EOF与2项stdout断开测试通过（测试辅助层首败及修复保留）；多屏反馈版本标识缺陷及两处测试同步问题已修复，Windows 完整核心回归和四组集成测试通过。geometry10夹具与分析器也已交付并通过Windows纯回归。具体状态见第 7 节；可执行或纯测试通过不等于真实 GUI 验收通过。

## 1. 统一规则

- 每平台、每suite的caseID唯一，单次运行不回绕。随机nonce/layout不是新的语义case。
- 原生fixture只记录自有窗口事件，不全局录键。Agent不能读oracle、源码、文件、a11y或CDP来绕过视觉。
- 先输入工具政策审计，再独立应用oracle/关键截图；API派发成功、模型说成功都不等于GUI成功。
- 状态：planned / implemented / pure_verified / gui_verified / failed / environment_blocked。重试和首次分开记录。
- 确定性工具验证与GLM任务验证分别统计，不能混成同一“模型成功率”。
- 每suite需有语义case定义、平台expectation、工具任务、GUI oracle、纯回归、构建及关键截图检查。
- 保留每平台/每基础动作至少10次首次真实操作的门禁。下列10项分组是语义coverage，不保证每个子类型已有10次；正式manifest须明确trial展开（按钮/方向/count/布局等）、足够数量与唯一trial ID。拒绝例、零量和重试不能用于凑动作成功次数。重复trial不冒充新的语义case。

## 2. 已有32条文本case（保留）

| Suite | 数量 | Windows实测 | Mac实测 |
|---|---:|---|---|
| baseline | 10 | 文本10/10、HIT10/10 | 文本10/10、HIT9/10（首例未观察到HIT） |
| known-input | 10 | 文本9/10，CRLF/LF预期差异 | 未完成 |
| punctuation | 6 | 文本5/6，模型漏4普通空格 | 未完成 |
| emoji | 6 | 文本5/6，模型把✅抄成☑ | 未完成 |

旧legacy0/9保持，不并入这32条的新成功率。修正平台expected后必须新run，不覆盖历史。

## 3. 新增确定性原生 GUI suites（分批实施，逐项验收）

### pointer-01..10

1. 中心移动；2. 边界内移动；3. 缩小截图坐标移动；4. 左键单击；5. 右键上下文菜单；6. 中键事件；7. 非激活自有窗口首次点击；8. 相邻小目标区分；9. screenshot padding拒绝；10. 越界坐标前置拒绝。

实际成功依据自有窗口命中区域与收到的button，拒绝例要求注入0次而非点击成功。

### multiclick-01..10

1. 左键双击收到count2；2. 左键三击收到count3；3. 单击count1；4. 双击自有文本控件选词；5. 三击按平台控件约定选行/段；6. 两次独立单击不能误称一次双击；7. 不同位置点击计数重置；8. 不同按钮不继承count；9. 取消多击有明确partial与release；10. 非法count前置拒绝。

Mac直接检查NSEvent.clickCount；Windows检查应用的MouseDown.Clicks/DoubleClick并结合可见结果。不能用“鼠标事件发了两次”替代。

### drag-01..10

1. 左到右；2. 右到左；3. 上到下；4. 下到上；5. 折线路径；6. 短距离拖动；7. 长距离/长时拖动；8. 目标边缘内释放；9. 可见文本拖选；10. 曲线路径连续性。

app记录down/dragged/move/up顺序、按钮、轨迹端点与结果位置；明确阈值，release后再次移动不得继续拖动。跨屏不在本suite假装验收，见multi。

### scroll-01..10

1. 纵向向下；2. 纵向向上；3. 横向向右；4. 横向向左；5. 双轴；6. 指定panel而非相邻panel；7. 不同tick幅度；8. 边界饱和仍如实报告；9. 零delta不注入；10. 超限/溢出前置拒绝。

实际成功依据内容offset/可见编号及原始滚轮事件，拒绝/零量不得算页面滚动。平台tick转native单位显式定义。

### keyboard-01..10

1. 普通按键；2. Enter/Tab；3. Ctrl/Meta+A；4. Shift选择；5. Alt/Option组合；6. 多modifier组合；7. 短hold；8. 长hold；9. modifier释放后普通输入；10. 非法键整体拒绝。

控件只记录自有焦点；held/release根据app事件与驱动结果关联，不因没有可见重复字符就断言release完成。

### cancel-01..10（监督者取消，不是另一个GUI输入agent）

1. drag按下后暂停；2. drag途中关闭；3. key_hold途中暂停；4. chord途中取消；5. 长文本途中取消；6. 客户端EOF；7. 网络断连；8. 清理失败注入的准确结果；9. 重复close幂等；10. resume后旧观察拒绝/新观察恢复。

工具契约确定性验证与真实GUI验证分别记录。取消控制能在执行中送达；验证自有held输入释放、后续可恢复、没有假成功或盲重放。清理失败测试不能在用户真实桌面故意遗留modifier。

### focus-01..10

1. 自有窗A到B；2. B到A；3. 激活后文本；4. 点文本框后快捷键；5. 自有modal；6. modal关闭；7. 菜单退出；8. 最小化后重新通过GUI激活；9. 已有不相关用户窗口不编辑；10. 焦点未确认时不盲目输入。

区别“首次点击用于激活”与“控件收到业务点击”；不会把一律修改所有应用first-mouse行为当产品修复。

### geometry-01..10

1. 原始尺寸；2. 1920约束；3. 1440约束；4. 等比缩放；5. 高DPI；6. 负原生origin的纯映射；7. 分辨率变化旧观察拒绝；8. DPI变化旧观察拒绝；9. target移除安全停止；10. 返回图像真实尺寸与元数据一致。

其中纯映射case不能算多屏真机通过；改变用户系统布局的case等待人工环境准备，不擅自改设置。

## 4. 多显示器multi-01..12（先实现/纯测，实机待环境）

1. 枚举两屏及各自geometry；2. 显式主屏；3. 显式副屏；4. 副屏在左/负坐标；5. 副屏在右；6. 上下排列；7. 混合DPI；8. 全桌面截图布局/尺寸；9. 空隙与padding端点拒绝；10. 跨屏拖拽；11. 拔屏/重插旧观察拒绝且不fallback；12. 分辨率/主副切换后的重新观察和安全恢复。

oracle同时含各自window PID/display、拓扑snapshot、原始事件与最终可见状态。用户准备双屏后按实际具备的布局逐项标结果，缺布局仍pending，不能用同一双屏截图声称12/12。

## 5. 真实应用crossapp-01..06（不是专用软件适配器）

1. Windows Notepad / Mac TextEdit：新建自有文档，普通/Unicode/多行输入、选择、快捷键。
2. 系统Calculator：通过可见按钮清零→10×20=，三次独立观察结果。
3. 文件管理器：仅协调者准备的临时目录，选择/滚动及自有无敏感测试文件拖放；不删除用户文件。
4. 浏览器离线页：只通过computer截图/输入，长页纵/横滚动、表单、拖动；禁止CDP/a11y读取页面绕过测试。
5. 绘图或原生自有canvas应用：连续按住绘线/拖动；无需应用专用输入实现。
6. 自有modal/menu和两窗口切换：焦点、关闭/暂停与恢复；不关闭/修改用户已有窗口。

每任务包含窗口所有权条件、允许操作、停止条件与cleanup。Windows用户Notepad PID24332不可操作；仅自有窗口可以清理。不把不同应用的系统差异一律归为模型或注入问题。

## 6. 可选桌面反馈验收（已有实现与纯测试，真实 GUI 待验收）

- 未加载反馈模块时完整输入内核仍可用，不能依赖GUI初始化；开启时受控端显示而不是仅控制端显示，依据真实控制状态呈现。
- 默认操作标识/轻量鼠标高亮/点击反馈/有效停止；品牌可配置/替换，蓝色光晕等润色不作为基础门禁。
- observing/executing/paused/cleanup/fault/disconnected/closed的视觉状态对应权威runtime事件。
- 装饰区域不抢焦点、点击穿透；停止按钮可交互，确实取消并释放。
- 模型收到的截图不含提示UI，并有平台实际截图证明；不可凭透明窗口属性假定排除。
- 用户指针与AI反馈可区分；取消/崩溃后的恢复、双屏覆盖、断连未知状态不误报。

## 7. 当前交付状态

以下为本轮 Windows 工作状态（几何回归的原始执行记录跨至 2026-10-01），不是完整产品通过清单。当前不执行 macOS / Linux 测试；保留后续平台验收，不据此缩小产品范围。代码、测试源码及构建脚本由 Codex + gpt-6.1-sol subagent 实现；测试执行、独立审查和真实桌面操作由 Claude CLI + GLM 负责。

| 范围 | 实现与证据 | 尚缺的验收 |
|---|---|---|
| multiclick / drag / scroll | 已有原生夹具、任务和分析器；历史纯测试见 `.agents/reports/gesture-fixture-windows-cc-20260930.md` | 本轮真实 GUI 首次有效操作门禁；取消、跨屏不能由这些夹具自动证明 |
| pointer / keyboard | Windows 独立 20-case manifest；143 项夹具纯自测和 30 项分析器测试通过，见 `windows-basic-fixture-cc-20260930.md` | GUI 验收与每基础动作至少 10 次有效操作；pointer-09 仍为 blocked/needs_capability，拒绝例不凑成功次数 |
| focus | Windows csc 成功；self-test 178 项、focus analyzer 35 项、B2 analyzer 30 项通过；导出 legacy63/basic20/focus10，旧63 parity通过，7步 native0。报告 `windows-focus-fixture-green-cc-20260930.md` | 10例真实 GUI/每动作首次有效trial未验收；focus09全局保护仍需supervisor，focus10须完整工具/截图/Close后心跳证据 |
| cancel | 原9项生产Worker取消契约、真实匿名管道4项EOF＋2项stdout断开已通过，历史首败保留。新增Windows回环TCP/TLS断连4/4通过：生产remote/stdio＋Test Backend，8个worker正常退出并完成4次新worker重连，见 `windows-tcp-disconnect-cc-20261001.md` | LAN环境、完整Host main、真实OS/GUI held释放与恢复仍待；新4例不等于所有网络故障覆盖。stdout仅下次write发现断开，不保证立即取消；mock释放不是OS释放 |
| geometry | 独立geometry10原生夹具、GUI任务与有界PNG/trace分析器；Windows default csc0，综合selftest287/0，geometry analyzer78通过，旧63/20/10/native策略回归保留。报告 `windows-geometry-green-cc-20260930.md` | 6例GUI、3例人工环境、1例纯映射；均不据纯回归宣称GUI通过。05/07/08需实际DPI/系统变化证据；首次有效trial未完成 |
| multi | 正式完整generation token与测试同步修复后root定向31项、topology27项通过；完整root lib275/0/6，四integration12/35（另1ignored）/3/9通过。报告 `windows-root-race-retest-cc-20260930.md`；新Host/Client Windows release已构建且未部署 | multi12实际场景/物理双屏、混合DPI、热插拔未验收；ignored项未执行，不计通过 |
| 可选桌面反馈 | Host/core/process/renderer 已有实现；Windows renderer 235 项纯测试通过，见 `windows-renderer-desktop-green-cc-20260930.md` | 实际标识/指针可见性、Stop 取消释放、穿透/焦点、截图排除、多屏和故障收尾；`capture_exclusion=requested` 不是排除通过 |
| crossapp-01..06 | 六场景任务包、隔离资产、准备器与只读文件检查器已实现。CC＋GLM Windows 49＋13项纯测试通过；实际prepare/check成功，已有目录两次预期拒绝native2，追加前后74文件/目录清单完全一致。见 `windows-crossapp-stage-b-cc-20260930.md` §9–10 | 六例真实GUI均未执行；browser JS未运行；窗口所有权、截图/工具trace关联、实际输入/拖拽结果和清理仍待。初始check六例均needs_evidence，GUI及10次首次有效trial门禁均false；自有两窗口不能冒充跨应用覆盖 |
| 平台原生文本 expected | Windows默认夹具已接入版本化known-09 CRLF预期；actual不normalize，旧canonical63不变。Windows csc0、综合selftest237/0、新分析器25项与focus35/B2 30项通过、旧63 parity通过；见 `windows-native-text-policy-retest-cc-20260930.md` | 新版真实GUI known-input重新跑；旧9/10及新增检查器首轮212/1失败保留，不追认通过；此策略仅known-09，不代表任意文本场景 |

上表报告的完整路径均位于 `.agents/reports/`。最新运行细节与历史偏差见 `complete-actions-progress-20260930.md`，原始输出见 `.agents/runs/`。Windows 桌面此前被系统网络授权弹窗遮挡；未确认用户已处理前不重复启动 Host/GUI，不自动修改系统授权。纯测试可独立继续。

完整范围仍为上述全部动作、12 条多屏、6 条跨应用和可选反馈要求。阶段交付、预期失败、blocked case、API 派发和模型自报均不能替代真实验收，也不能缩小范围后宣布完成。

权限只读补充：`windows-network-authorization-readonly-cc-20260930.md`记录两个既有Host路径各有Private/Inbound/Block本地规则。本次查询不代表全局有效访问或当前弹窗状态，不证明用户之前点了什么；规则和权限未更改，不通过换路径绕过。

## 首次 trial 计划与提交记录计数：Windows 纯验证补充

`acceptance-fixture/trials/` 与 `scripts/windows-first-trials.py` 已交付，状态 **pure_verified，非 gui_verified**。CC + GLM 在Windows执行原36/36、新25/25，原始native退出码均0。首次RED及StageB编排错误均保留，报告：`.agents/reports/windows-first-trial-stage-b-cc-20260930.md`（以末尾协调更正为准）。

固定计划为12组、120个slot、60个语义case；正例候选95：move12/click25/drag10/scroll14/text_input10/key_chord12/key_hold12。另25个拒绝/零量/blocked/coverage-only仍留分母。只保证七种基础action的预排次数，不声称每种按钮×count×方向组合十次。

计数器分开首败、缺失、重试、重复与冲突；输入记录及证据引用仅是待审核的提交声明，不能自动认证真实截图、实际模型、工具政策或窗口身份。空记录实际CLI结果为120 missing/0候选，`gui_verified=false`及`action_trial_gate_satisfied=false`。**本批120个GUI slot一个也未执行**，不改变第2节的旧文本实测结果，不补记GUI通过。

真实Windows操作仍需网络授权障碍处理后的新只读预检，再依次执行；多屏需用户提供环境。macOS/Linux保持暂停。

## 2026-10-08 Windows multiclick 首轮真实执行补充

授权遮挡已由用户处理并经新只读截图复核。当前Windows源码产物已重新构建/部署，10个multiclick case完成一次Check：6个输入例匹配、1个无输入拒绝例匹配、3例失败（04选区尾空格与判据冲突；05未形成选区且说明被截；08GLM漏第三左击）。首次失败不覆盖，未并入正式120slot计数。

默认尺寸截图另发生4次TLS连接关闭；模型重开会话并缩小捕获后才开始输入，这不符合初始故障停止要求，不能据工具审计通过宣称严格任务合规。整体gui_verified维持false，细粒度原始事件/截图为后续定位证据。源码已定位TLS零接受被判致命的处理问题，正在先验证回归RED，尚无生产修复通过声明。

证据与更正：`.agents/reports/windows-current-multiclick-cc-20261008.md`、`windows-multiclick-evidence-cc-20261008.md`；原始截图/转录及协调核对在对应runs。其余GUI套件、反馈停止/穿透/排除、多屏与跨应用仍待，macOS/Linux继续暂停。

### 同日修复验证补充（不改首轮成绩）

TLS回压处理已修正，原3回归在Windows由2失败+1通过转为3通过，另3个传输用例通过。默认参数两次观察在同会话成功返回且新Host日志无原writer0错误，但未看到测试夹具，画面时效/桌面准备仍待核查；不能据图片成功返回就认证正确实时桌面。

选词/整行判据与完整文本布局代码已修正：Windows原48replay合同和287selftest均通过。原04/05/08首轮失败不追认、不重算，05空选区仍是不满足整行目标；08模型漏步骤仍保留。修后layout实际测量与新fixtureGUI尚待，正式120slot以及其余完整门禁未完成。

### 修后真实Windows布局测量已补齐（2026-10-08）

CC+GLM在交互Session1运行StageA --layout与StageBLayout，分别50和4162个布局断言全部通过，native退出均0。后者覆盖3套件的63个ready/checked/finished布局状态，不代表鼠标动作已执行。case05全文实际测量148px、可用150px，原75px裁切问题的布局合同转绿。截图可见性、画面时效和实际选区效果仍待。

场景诊断勘误：任务栏日期实际为10/8而非10/3；WmiMonitorID有实例，之前“无显示器”是查询解读错误。不能因此认定物理合盖或特定截图故障。场景仍未建立，正在补backdrop退出直接证据；不据这些布局数据回填正式120slot。详见本轮layout-green报告及scene-diagnosis报告最后勘误。

### 新场景截图门禁已恢复（2026-10-08，同日后续）

新StageB multiclick夹具随机nonce24Q9DM在远端CC+GLM唯一一次默认尺寸observe截图中清晰可见，与本轮oracle一致，协调独立目检及原始图像字节核对完成。当前这个新场景的截图链路通过；没有点击/输入/重开会话/缩小回退。原场景失败根因未全部确定，不追认旧结果；此证据不是连续刷新/真实动作验收，也不回填120slot。报告：`.agents/reports/windows-scene-nonce-cc-20261008.md`。所有自有运行已结束、清理已核对，后续从修后动作测试继续。
