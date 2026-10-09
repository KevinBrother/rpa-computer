# Windows首次trial计划与记录统计实施计划

> 完整动作规范阶段B的续项，已批准至少10次首次操作的要求；不是另立更小的最终验收目标。源码/测试/CLI由gpt-6.1-sol实现，实际执行和独立review仅真实CC＋GLM。无worktree/commit/push。

**Goal:** 给实际Windows验收预先生成完整、不可随结果缩减的trial清单，区分首次、失败、缺失、重试和复用证据，统计七种基础动作；数量与输入记录自报不能直接证明GUI验收。

**Architecture:** 独立Python标准库模块，不改产品/原生夹具/旧分析器。两层职责：计划生成＋提交记录计数。统计器的记录来自监督者整理，属于待核验的输入数据；本批不伪装成新的截图/trace/oracle验证器。可以输出reported_first_matches/candidate_count_met；`gui_verified`及`action_trial_gate_satisfied`恒false，并显式列出原始证据关联、实际模型/工具政策、截图与窗口所有权的独立审核仍需完成。未来审核器接入是单独边界，不能通过一个自报true字段绕过。

**Tech Stack:** Python标准库，现有Windows fixture task/catalog只读。Windows Python3.12实测；本机仅py_compile。

## 计划合同

- Windows-only，模式`glm`和`deterministic`严格分开；默认GLM要求模型标签glm-5.3-flash，但输入声明不叫model_verified。
- 七动作种类：move、click、drag、scroll、text_input、key_chord、key_hold，不把管理点击/Check/Next/观察算目标动作。
- 固定6个suite：pointer、multiclick、drag、scroll、keyboard、known-input；每suite10个语义case，按七个基础动作的最低10次要求预排：pointer×4、multiclick×1、drag×1、scroll×2、keyboard×3、known-input×1，共12个run组/120个slot，语义case仍60。预排结果不能按成功结果追补轮次或删失败轮。实施优先明确可维护，不引入GUI调度框架；实际执行需串行监督与每run新fixture身份。
- 每run/slot有由campaign UUID确定的唯一计划ID，语义caseID不变，source fixture trial仍1..10。计划ID≠运行时run/session/nonce；未绑定为null/needs_preflight，不捏造运行数据。
- 正例目标分类以实际任务为准：pointer01..03 move、04..08 click；multiclick01..05 click（count1/2/3及原生选择），06..09复合/reset保留coverage_only，10拒绝；drag01..10 drag；scroll01..07 scroll，08边界饱和coverage_only、09零量、10拒绝；keyboard01/02/07/08 key_hold（02两次hold仍每slot最多计1），03..06 key_chord，09组合释放coverage_only，10拒绝；known01..10 text_input。pointer09blocked、10拒绝。不把支持的动作种类表当所有按钮×count×方向的穷尽覆盖；保留case/variant分项计数和未覆盖组合说明。
- 计划正例候选按七action分别为move12/click25/drag10/scroll14/text_input10/key_chord12/key_hold12，共95个slot；其余25个仍留在计划但不计正例。每个已批准语义case至少出现一次。这里只承诺每基础action种类≥10，并如实列各参数variant数量，不宣称每个按钮×count×方向组合都有10次。草案最初的每suite十轮/600slot属于不必要扩张，在任何源码/RED冻结前收敛到用户批准的最低要求，不是根据失败结果缩分母。
- 保存引用catalog/task的相对路径和SHA，禁止源漂移后悄悄继续原campaign。已绑定记录要包含campaign/plan digest以防跨清单拼接；调用summarize验证manifest仍是canonical生成物（拒绝改阈值、改eligible、删slots、重复ID、错schema/mode/platform）。
- 所有120slot留在分母，缺失为missing，环境阻碍不删除。expected quota≥10只是计划数，不是实际操作数。

## 提交记录与计数合同

- 只接受有界JSON对象/列表（严格整数，bool不能冒充attempt1；限制字符串、记录数/输入bytes）。公开API先在API.md固化精确字段/大小界限，CLI保留JSON解析失败/duplicate-key/nonfinite/截断拒绝，不靠json最后值吞重复key。
- 每条记录带计划slot/run标识、attempt_index、模式/platform/suite/case、运行时完整身份(run/session/nonce/trial)，目标动作、结果state及tool_call/request/observation等关联标识。记录和evidence refs都只是supplied claims，不能因有hash/ID就称审核过文件。
- attempt1是唯一首次；attempt2+始终在retry栏，不能在首条缺失时提升为首次，不能覆盖first failed。相同记录重复提交最多计一次且报告duplicate；相同ID但内容不同fail closed/conflict。跨不同计划slot复用同一真实fixture身份/工具call/request等必须报告冲突，不因改计划UUID而凑数。
- reported_first_matches仅eligible slot＋匹配目标动作＋首次matched＋完整不冲突绑定，可按action/variant汇总；失败、拒绝、零量、blocked、unknown、missing和重试不贡献成功候选数。首次失败仍保留，后来retry matched不改first。
- 同一slot一组组合输入最多计一次指定action；一组相同工具证据不得拆成多条成功。不得用点击Check的click填补该slot真实key_hold/text_input目标。
- 混合GLM/deterministic或错误平台/型号、wrong case/trial/schema/非法内容需拒绝或明确unknown/conflict且不计候选。计数元数据不声称检验实际LLM转录或文件真实性。
- 空记录必须120 missing、所有候选0、全部GUI/验收门禁false。完整合成matched记录最多只能证明统计函数工作，`candidate_count_met`可以true，但两个验收flag仍false且explanation明确独立审核缺失。

## 文件与接口

- 新 `acceptance-fixture/trials/`：catalog、plan、records/summary模块、API.md、README.md；按职责拆分。
- 新 `tests/windows_first_trial_accounting.py`：独立unittest源；不修改任何旧测试断言。
- 新薄CLI `scripts/windows-first-trials.py`：plan/summary命令，stdout JSON，错误非0；只读现有输入/引用，不起应用/监听/注入/删除/覆盖。可选输出若实现必须exclusive create，禁止自动覆盖；不必提供。
- 新 `acceptance-fixture/tasks/windows-known-input.md`：从现有Cases.cs得到10个已知payload，提供安全source-free任务。不得复制旧模板错误：当前没有key_press工具动作；known09的Windows native expected是CRLF版本政策，不能声称Host一律把CRLF改为LF。明确JSON escapes须解码输入，保留NBSP/NFD/非BMP/Tab与CRLF，不附oracle。原Windows其它5份task原样只读引用。
- 两个核心API：`build_manifest(campaign_id, mode="glm")`、`summarize(manifest, records)`；StageA stub＋测试/API先冻结，StageB后实现。辅助可拆模块，但API与原测试不弱化。

## 执行

- [x] sol交付StageA接口stub/API及真实需求测试，py_compile-only，冻结source+只读依赖+runner。
- [x] CC Windows运行首次RED，确认缺能力而不是import/权限/环境错误，保留原始native code/输出，随后停。
- [x] sol实现、补README/新文本任务/CLI并冻结；原StageA测试不改断言，必要新增独立测试文件。
- [x] CC独立spec→quality review后Windows纯回归、plan实际CLI和空记录summary；必须仍missing/GUI false，不操作桌面。
- [x] 协调核对原始结果和冻结，更新总矩阵。真实120slot未执行，不称十次操作验收通过；独立原始证据审核集成仍待，不缩小完整目标。

## 本批交付结果（非完整 computer-use 验收完成）

- StageA Windows首轮RED：36个方法、120个subTest失败，均缺少实现；原始archive/log不改。
- StageB CC + 实际message.model `glm-5.3-flash` 完成Windows执行（2026-09-30T20:14:48–20:14:57Z）：原36/36、新25/25；plan CLI生成12组/120slot/60语义case/95候选；empty summary120missing、0候选、验收flags false。
- 独立负例实际是malformed JSON拒绝native2，不是重复键钩子证明；wrapper最终误判harness1的错误保留。重复键独立由StageB测试覆盖。见CC报告末尾协调更正。
- 原始证据核对：`.agents/runs/windows-first-trial-stage-b-cc-20260930/coordinator-evidence-readback.json`。源码与原测试没有修改。
- **冻结说明**：执行与协调核对时35/35一致；本计划现在仅更新完成勾选/结果，属于验证后的文档修改，故当前工作区不再宣称整份35文件清单全等。原冻结清单和archive保持原样；可执行源码仍对应被测版本。
- 120真实GUI slot未执行。网络授权、原生释放、多屏、跨应用、反馈Stop/穿透/排除等独立门禁均未解除；不开始macOS/Linux测试。
