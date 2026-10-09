# 分层 GUI 验证（2026-09-30）

状态：执行中；不覆盖2026-09-29旧0/9证据，不宣称所有门禁已完成。

## 测试设计

- 普通视觉baseline10：不给agent样例答案，只根据清晰截图识别普通中文/ASCII/数字；不夹emoji/特殊空格。
- 已知输入known-input10：给明确payload，oracle逐UTF16校验；包含长度/全角/NBSP/非BMP/组合字符/CRLF/Tab。原始task_payload与预期文本分开。
- 标点punctuation6与emoji6：独立统计，失败保留；不能从Windows单色emoji外观强行证明唯一码点。
- 3方对照：期望文本 → 模型text_input参数 → 应用actual_text；first-pass与重试分开。工具策略审计与GUI正确性分开。

## 已发生的实际验证

### macOS可用性

09:38 source-free发布目录的真实Claude CLI实际模型glm-5.3-flash，只有8个computer工具。open→observe→close，17项审计通过，3 calls/4 turns/29.6s。截图显示自有legacy靶场，没有锁屏或权限弹窗。本轮没有输入操作。

证据：`layered-mac-availability-launch-20260930.log`；原转录 `/private/tmp/computer-native-mac-20260929/evidence/20260930T013815Z-55144/transcript.jsonl`。

### macOS计算器首次补测（未完成，不算3/3）

09:40启动，480s预算到期rc124，缺final result所以策略审计失败。转录中前两轮agent声明看到200，第三轮尚未完成；截图序列保留，协调者已逐帧查看req-018-t1-equals与req-025-t2-equals截图，均为10×20/200；仍只能称部分证据。启动途中有一次空modifier key_chord工具参数错误，GLM依据返回错误改用点击。不能将超时重写成通过。

证据：`layered-mac-calculator-first-transcript-20260930.jsonl`、`layered-mac-calculator-first-final-frame-20260930.png`、`layered-mac-calculator-launch-20260930.log`。legacy fixture已按PID+启动时间+exe核验退出，backdrop已自行到期，无全局kill；Calculator窗口所有权不充分，不由协调者关闭用户应用。

### Windows环境

用户Notepad PID24332仍在Session1，不操作。原Host/Client SHA与2026-09-29发布产物一致。09:44新自有TLS Host在Session1启动，端口8399；当前动态探测LAN地址用于启动，不将DHCP地址视为永久配置。尚未运行本轮分层agent。

## 待补

fixture/离线分析器构建、自测、独立审查、双平台四suite真实GLM轨迹与应用oracle、清理。

## 中间审查/环境定位（不构成最终验收）

- 分层case目录33条含legacy，Mac pure self-test40/40；Windows csc首次CS1593修复后编译成功，但真机自检3/40失败，定位为C#自检谓词/代理对计数/标点范围错误，待修并重跑，不据Mac通过推测Windows。
- 离线分析器独立审查发现exact门控/时间配对等缺陷，修复后26个pure tests通过，二次独立复核中；旧错误统计未用于GUI结论。
- Mac新fixture首次只读可见性检查发现主屏只有backdrop，不是锁屏。原生只读probe证明fixture窗口bounds x=2430，位于secondary display（主屏CG display2 bounds x0..1920）；因此是靶场初始window.center落到另一屏，不是截图黑图或模型看不懂。正在固定自有fixture初始定位到CG主屏，不改用户显示配置。
- 再测时backdrop有效期要覆盖整个agent预算，首次计算器5分钟backdrop到期后背景曾露出用户浏览器，因此该轮不能宣称完全中性桌面隔离；后续每run重启有20分钟上限的自有backdrop，保留失败轨迹。

## 2026-09-30 11:30 CST 核实进展（不是全套结束）

### Windows baseline 最终证据

同一次 agent 正常结束（exit_code=0、timed_out=false），没有为凑成功率重启/重试。10 个不同普通文本案例：first_attempt_exact=10/10、final_exact=10/10、target HIT=10/10、WRONG=0；10 次 text_input 与10次应用text_check均可按case/时间证明对齐，unresolved=0，三方原始UTF16一致。响应 model 标签全部 glm-5.3-flash。转录工具审计17项通过（8 tools、67 calls、69 turns、901248ms），最终截图实际显示 SUITE COMPLETE 10/10 / MATCHED。旧2026-09-29复杂样例0/9保留不变。

证据：`.agents/runs/layered-win-baseline-final-{transcript,summary,oracle,analysis,audit,frame}-20260930.*`。最后一帧由coord实看，不拿最终文字result替代图像。

重要环境限制：旧Windows backdrop自身timer只有5分钟，虽ScheduledTask expiry25分钟，最终图像背景已露出owned coordinator启动terminal。因此这次有普通文字功能/应用oracle证据，但**不能称全程中性桌面隔离，也不能升级为Windows OS文件隔离的完整黑盒门禁**。即使统计通过，环境欠缺仍保留，新的helper会改明确25分钟上限以覆盖完整run，不重写旧结果。

分析器真实轨迹统计边界修复后coord执行30个pure tests全部通过。observed_case_count仅数实际oracle case，首/末unknown check不会借后/前成功洗白；独立复核仍需补。

### macOS -d 可见性核验

-d fixture SHA256 `3eca09ef9c835c2021915f750b1b4cf9f4dd89ff74abef86b402dde6a827bff4`，owned PID92245启动日志READY已核对，coord按exe/启动时间核验后只发自有SIGUSR1。oracle确有coordinator_raise事件。瞬时window-list probe仍显示fixture stacking index43/background0，但稍后**同一MCP实际主屏截图已看到900x792完整靶场window**，所以不能仅凭瞬时probe断言置前永远失败。

实看 `.agents/runs/layered-mac-baseline-d-preflight-frame-20260930.png` 又发现：顶部trial/nonce/target labels缺失，图形、样例、输入框、按钮可见。这次未启动GLM输入、未计任何case为通过；-d oracle无text_check。最小修复writer在定位自有ShapesView绘制越界假设，协调者会以新产物真实截图而非纯自测验收。owned fixture92245/background92557均身份核验退出，无全局kill。

### 下一步

新的bounded背景helper、Mac绘制修复正在独立写入范围并行实现（都禁止GUI操作）。Windows known-input10 / punctuation6 / emoji6、Mac四suite、Mac calculator3仍未完成。不能宣告总goal结束。

## 11:45 CST 后续环境/中间证据

- Mac -e新产物self-test49/49及catalog parity通过；coord自测复跑49/49。SHASUM `3457b4bdee9c21ed9779ef88e1b2d530a69f5046e6ed59ba240adaf09722a760`。离屏证明绘制边界回归，但writer诚实承认离屏无法复现顶部缺失，不能宣称根因已确定。新产物真机预检 Host initialize rc4/no_display，readonlyprobe显示两屏is_active=false/is_online=true；因此本次没有可用实拍，没有GLM输入、没有text_check，不计通过。证据prefix `layered-mac-baseline-e-*`。owned fixture97946/background97948身份核验后已停，不自动唤醒/解锁、不改TCC。
- 新helpers均按25min明确时限/1..30 failclosed；coord纯参数/静态self-test30项通过。Windows新helper已用真实PowerShellParser ParseFile无错误、在Session1启动，task expiry30min；旧5min失败不覆盖。Mac新helper尚不能凭inactive probe证明实际全屏覆盖，正常display时需实拍再确认。
- Windows known-input真实GLM已启动：fixture19764、backdrop19064、runner24368与Claude8196均Session1；agent GUID677e4adb-ca91-4532-8ad9-4d6b112b888d，oracle run-87462645aa5c4904ace8a91d7ad196ed。运行中快照前6case text_check=true，包含常用中文/长文本/Unicode/NBSP已知payload，不是最终结论，仍等待最后4case及final result/audit。
- 分析器最新独立复核未发现阻塞；旧live统计产物保留，另生成 `layered-win-baseline-live-corrected-analysis-20260930.json`，observed4/expected10/checked3，不覆盖历史错误artifact。

### Windows背景归因补充（保留旧结论但不沿用错误单一归因）

known-input第7例真实截图 `layered-win-known-input-live-frame-20260930.png` 中新25min backdrop仍在运行，却仍看见owned启动terminal。故背景露出**不仅**由旧5min到期引起：后启动的agent powershell新console也可覆盖normal backdrop。旧baseline后半隔离不足的事实不变，但不能只给timer这个原因。新测试目前仍是受控诊断，不是声称完全中性。为后续runs另新增WindowStyle Hidden的launcher副本，严格身份record/InteractiveSession不变，不关闭/隐藏任何用户现有window、不打断当前GLMrun；是否在真机WindowsTerminal环境隐藏需下一次实际截图确认。

## Windows known-input 最终核实（2026-09-30）

同次run exit0、timed_out=false；glm-5.3-flash、8tools/67calls/69turns/1070663ms，17项工具审计通过。10case每例恰好一次text_input/一次text_check，没有失败重试；10/10 target HIT、raw UTF16 exact首次及最终均9/10、unresolved0。最末截图实际 SUITE COMPLETE10/10 / MATCHED(Tabcase)，coord已实看。证据prefix `.agents/runs/layered-win-known-input-final-*20260930.*`。

- 精确通过known01..08及10，包含原2026-09-29旧0/9的复杂Unicode样例known04（38 Unicode scalar / 39 UTF-16 code units）、12/34字中文长文本、全角标点、NBSP、nonBMP独角兽、NFD组合音符、Tab；不同于纯视觉，本组明确提供原始JSON payload，不能合并视觉识别成功率。
- 唯一严格失败known09：任务原始与模型原始参数一致 `第一行\r\n第二行\r\n第三行`；应用actual也同样CRLF。checker expected及运行计划canonical为LF，即 `第一行\n第二行\n第三行`，原始码元比较失败，expected11/got13。分析器保守标input_divergence；这是该分层字段的描述，不自动等同SendInput重复/丢字。运行计划CRLF规范化一次、目标三行原始CRLF表示需要独立源链核查，暂不把此例改算通过、不Normalize(actual)洗白。独立诊断报告进行中。
- Agent最终表多数nonce填“—”，因此没有把“逐例完整nonce口头记录”也算满足。oracle独立记录全部case/nonce并完成可证明配对，但这不等于模型报告的nonce字段完整。
- controlled diagnostic环境不足仍保留：新backdrop没有全程挡住agent启动console。新hidden launcher真实PowerShellParser通过，但需下一run实拍，不能提前宣称中性桌面门禁通过。

## 12:10 CST 诊断/后续run状态

known09独立只读诊断已完成，coord也逐段核查源码并实际看了 `layered-win-known09-check-frame-20260930.png`：确是3行内容、无额外空行，严格MISMATCH11/13。模型raw参数正确，计划把CRLF对规范成一次LF，注入每LF一次Return，目标应用原始CRLF记录与canonical-LF预期不同。报告 `.agents/reviews/layered-known-crlf-diagnosis-20260930.md`。因此归因到**靶场跨平台换行预期与Windows目标控件表示不一致**；旧9/10不改、不软化actual检查。报告中的“排除通用管线缺陷/结构上Mac可通过”只作为本组路径分析，不是所有输入/所有应用保证，Mac实际仍未跑。

Windows punctuation新run已启动（fixture29692、backdrop20564、runner15852、Claude536，均Session1），GUI input与其他agent串行。新hidden launcher完整record保留且可用旧stop严格核验；运行中第6例截图 `layered-win-punctuation-live-frame-20260930.png` 已实际看到完整中性背景、无新agentconsole遮挡，这一轮的可见环境修复有实拍证据，不反向洗白之前runs。

run GUID `e37f1a5f-e2b4-4238-8fa5-f385bdd77a5c`，oracle `run-294f66cfdddf4c02b53fd91c0520a76c`。中间5check均raw exact、6target HIT，正在第6例，未最终统计。Windows OS文件隔离仍不足，不因背景修复宣布完整黑盒门禁。

Mac两屏只读复查仍inactive，没有自动解锁/改变电源/TCC。此时仍可推进Windows emoji与其他证据，不将整个goal标blocked；Mac四suite和计算器3次仍必须补完才能结束。

## Windows punctuation 最终核实（2026-09-30）

新hidden launcher这轮真实运行正常结束，exit0/timed_out=false；17项工具审计通过，8tools/41calls/42turns/509786ms，响应glm-5.3-flash。6case一次text_input/一次check；target6/6 HIT、首次raw exact5/6、final5/6，unresolved0。最终真实图像 SUITE COMPLETE6/6，失败case红字MISMATCH17/13，coord亲自看图，背景中性且无agentconsole覆盖。

唯一失败punctuation06：期望 `引号 “全角” 与 "半角" 对比`，模型text_input参数及应用actual都是 `引号“全角”与"半角"对比`。四个普通空格在调用输入工具之前已经丢掉；layer1=false / layer2=true / layer3=false，是模型识别/抄写阶段差异，不是本次键盘注入丢字。直/弯引号本身、全角/半角括号、中英文逗号、竖线其余5例都精确通过，不能声称“全部标点识别失败”。证据prefix `layered-win-punctuation-final-*20260930.*`。

cleanup已按新hidden record与旧stop严格身份契约执行；旧脚本/旧证据均保留。随后串行启动Windows emoji6新fixture，Mac仍待亮屏后实拍 -e header 验证及四suite/Calculator3。整体goal仍active。

## 12:45 CST Mac从环境等待转为真实输入验证

两屏只读状态恢复active；coord未自动唤醒/解锁/改TCC。新-e2 preflight实际截图 `.agents/runs/layered-mac-baseline-e2-preflight-frame-20260930.png` 显示caseID/轮次/nonce/target/header完整，-e绘制修复这次有真机图像证据。新helper真实双屏bounds分别[0,0,1920,1080]、[1920,0,1920,1080]，不再副屏x3840、不再inactive动画inset；background期限25min。

Mac真实Claude CLI+GLM baseline10已启动且确认live：CLI13292、Host13322；exec session45121，fixture12555、bg12566，oracle prefix `layered-mac-baseline-e2-*`；agent evidence `/private/tmp/computer-native-mac-20260929/evidence/20260930T042815Z-13217`（路径另写 `layered-mac-baseline-e2-agent-evidence-20260930.path`）。源码隔离profile/sha/meta保留。12:42当次oracle已有8个不同case文本精确一致；未final，不宣称10/10。

特别保留case01目标click没有HIT/WRONG事件的事实。首click参数[1277,473]在绿色圆内，coord实看post-click图确没有HIT，文本该case仍raw exact。其后singleclick均可HIT。因此不把工具dispatched当作target通过，也不按成功textcheck倒推点击成功。ShapesView未override acceptsFirstMouse，仅acceptsFirstResponder，现象符合自有NSView首次窗口激活click未传mouseDown的假设；未记录当时keyWindow状态，不称具体根因已证明。正在做新-f纯API回归与最小自有view click-through修复/自有focus日志，不触碰运行中-e二进制，不给当前agent重试首case，不修改Host输入行为。

Windows emoji先前只有fixture准备，没有启动GLM、没有输入，因Mac显示恢复可测，按统一串行调度暂将这份owned Win fixture身份核验停掉，oracle setup记录仍保留（run-63a7d8578f7a494ea6b54eccf2489e19）。Windows已有3suite最终证据不变，emoji6待Mac当前run结束后新run，不错记为已完成/已失败。用户Notepad未操作，Windows TLS Host仍在。

## Mac baseline 最终核实及接续（2026-09-30）

Mac同次真实GLM run正常exit0，最终结果事件存在；17项工具审计通过，8tools/67calls/69turns/1017168ms。10个不同普通文字case，首次及最终raw UTF16 exact10/10，应用HIT9/10、WRONG0，case01无HIT事件保留为未验证目标点击，不能把文本通过倒推HIT。unresolved0、没有重输文本。最终真实截图 SUITE COMPLETE10/10 / MATCHED，coord实看：header/nonce/样例/输入清晰、背景中性。证据prefix `layered-mac-baseline-e2-final-*20260930.*`。模型最终报告也诚实记首case无HIT，并报告case08预算耗尽后的未派发Check请求/关闭重开会话，应用只实际check一次，不改原始失败工具事件。

-f first-mouse真实API回归fail-first 1/50→50/50，coord独立重跑50项及独立review通过，SHA `5823306b714e00919157a70a8a53389dd57f2ef399d4a6c07398e45d63928099`。未宣称GUI已验收，未改正在跑的-e、未重试/覆盖-e首case；focus日志将用于下一真实run关联。审查报告 `.agents/reviews/layered-mac-firstmouse-review-20260930.md`。SDK签名以实际编译的NSEvent?为准，worker“旧文档NSWindow?”的表述不作为API历史变更事实。

-e2 CLI/Host正常自行退出；bg25min后已自行退出，fixture12555身份核验SIGTERM停止，无全局kill。Mac随后准备-f known-input，但预检Host再次no_display/rc4，**没有启动GLM、没有键鼠输入、没有case通过**；owned准备fixture21116/bg21125身份核验停止，setup oracle/日志仍保留。不能把pure50/50当Mac known输入10例已通过，也不自动唤醒/解锁。

因此继续串行Windows emoji：新fixture3372，oracle run-5a645b99de4f485c8f8f4642583d75d0；启动日志prefix `layered-win-emoji-*-start2-20260930.log`，runner/agent状态由后续实际进程和轨迹核对。Mac known/punctuation/emoji和Calculator3仍必须补，不缩小scope、不标goal完成。WindowsOS文件隔离缺项仍公开保留。


## Windows emoji 最终收取与阶段性清理（2026-09-30 13:33:23 CST）

上一轮已启动的同一真实GLM run权威核实为正常结束，未重启/重试以凑成功率：runner18012、CLI3148、client18216均已退出，run-summary exit_code=0/timed_out=false/stdout_drain_complete=true；终态result存在。响应模型实际标签仅glm-5.3-flash。策略审计17项通过，8tools/39calls/40turns/429055ms；这只是工具策略审计，不替代GUI正确性。

- Windows emoji6：6个不同case、6次text_input、6次应用text_check，按case/时间关联均proven，unresolved=0；首次raw exact **5/6**，final **5/6**；随机绿色圆HIT **6/6**，WRONG0；未对失败case补输。
- 唯一失败emoji-01：oracle expected `状态 ✅`（U+2705），模型payload及应用actual `状态 ☑`（U+2611）；模型参数在键盘派发前已经偏离，应用忠实输入。其余地球🌍、火箭🚀、微笑😀、铃铛🔔、书本📚五例都精确通过。
- 协调者实际看了与`req-t1-check`工具返回关联的失败截图：大字号样例仍是单色方框勾，输入框是另一方框勾，红字MISMATCH expected4/got4。图片清晰、未黑屏、未裁切，但仅凭该字形不能保证唯一反推出Unicode；不能据此宣称GLM所有emoji都不识别或原生键盘丢字符。最终截图SUITE COMPLETE6/6、最后case MATCHED，与oracle一致。
- 完整证据prefix `.agents/runs/layered-win-emoji-final-*20260930.*`；失败图 `layered-win-emoji-first-check-frame-20260930.png`；remote agent GUID75b60c66-a7cd-4387-9cf4-93f623187161，fixture run5a645b99de4f485c8f8f4642583d75d0。

Windows四套分层用例现已执行完毕，不等于全部通过：baseline10/10、known-input9/10（CRLF目标表示与LF预期）、punctuation5/6（模型参数漏4空格）、emoji5/6（模型参数勾选符号错误）。OS文件隔离欠缺、前两suite背景console露出、旧0/9失败均保留，不宣称完整黑盒门禁通过。

严格身份核验清理已完成：本次agent/backdrop任务、fixture任务、TLS Host29420均停止/注销；权威final check显示本轮owned_tasks=[]、listen8399=[]，被检查的PID中仅用户Notepad24332仍在Session1。证据和部署文件保留，未操作/关闭用户记事本。清理证据`layered-win-emoji-*-cleanup-20260930.log`、`layered-win-emoji-fixture-cleanup-correct-20260930.log`、`layered-win-host-cleanup-20260930.log`、`layered-win-final-identity-cleanup-check-correct-20260930.log`。第一次fixture参数名误用及第一次final-check命令引号错误均只失败、未产生误清理，失败日志保留。

## macOS 后续仍待活动桌面（2026-09-30 13:33:23 CST）

最新只读probe g/h确认两屏online=true、active=false、asleep=true，CGGetActiveDisplayList count0，screen capture与AX预检true；因此这次有明确休眠状态证据，但不臆断是否另有锁屏/TCC变化，也不自动唤醒/解锁。未启动新GLM、没有盲目键鼠输入；自有Mac fixture/backdrop/host进程均不在运行。

-f已stage的fixture SHA仍为5823306b714e00919157a70a8a53389dd57f2ef399d4a6c07398e45d63928099，本轮fresh self-test50/50通过（`layered-mac-fixture-f-recheck-20260930-g.log`），这不是-f GUI验收。known-input/punctuation/emoji及Calculator3次prompt已在source-free发布目录就绪；继续要求用户手动点亮并保持Mac屏幕、必要时自行解锁，恢复后新唯一run前缀执行，不覆盖旧-e2和-f准备失败证据。

完整范围不缩小：Mac baseline旧-e2已10/10 raw text、target9/10；剩余Mac三suite与Calculator完整3次尚未验证，不能标整个目标完成。

### 本轮证据完整性备注

只读probe第一次调用漏传新输出路径，暂时覆写了旧`native-mac-display-probe-20260929.json`。已从原始未修改worker transcript中的完整stdout恢复原始JSON（pid75872、timestamp_epoch1790684125），本次pid27478/epoch1790745788结果单独保存在`layered-mac-display-recheck-20260930-g.json`；后续h显式给独立路径。截图第一次按数组位置提取未对应Check操作，保留为`layered-win-emoji-first-check-extraction-before-input-20260930.png`；正式失败截图由req-t1-check关联提取、已实际看图。未用误取证据作为通过证明。


## Mac环境阻塞复核（2026-09-30 13:38:05 CST）

上一轮属于实质进展：Windows emoji最终证据收取、关联分析、实际看图及owned清理完成。此轮未假定显示环境恢复：新i只读probe两屏仍online/inactive/asleep、active_count0、screen/AX预检true。随后真实release Host以EOF且不请求任何MCP动作启动，backend初始化明确失败no_display/rc4，无stdout、无输入；Host已terminal并退出，当前无owned GUI/Host残留。--describe rc0只输出静态能力清单，不作为真实后端可用证据。

新证据`layered-mac-display-recheck-20260930-i.json`、`layered-mac-host-startup-recheck-20260930-i.json`。剩余Mac三suite和Calculator3仍未运行；需要用户点亮Mac并必要时手动解锁才能推进，不以重跑纯测试/配置说明替代GUI验收，不重启此前已完成agent，不自动解锁或改系统设置。完整目标仍未完成。


## 环境阻塞确认，等待用户恢复桌面（2026-09-30 13:39:45 CST）

连续三轮复核同一外部条件：g/h、i、j均为Mac两屏休眠/inactive，活动显示器0；i真实Host启动no_display/rc4，j最新只读复核仍未恢复。上一轮没有推进实际GUI用例，也没有活跃任务可以等待，属于环境条件复核，不冒充验收进展。当前无owned Mac测试进程；Windows四suite和owned清理已完成。剩余Mac known-input10、punctuation6、emoji6、Calculator完整3次不能在不可见桌面安全执行，必须由用户手动亮屏、必要时解锁后继续。目标标记为blocked而非complete；恢复后保留原完整范围、新run新证据，不重复重启已完成测试。
