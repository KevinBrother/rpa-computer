# Windows multiclick fixture StageA — author handoff, 2026-10-08

**状态：SOURCE_FROZEN（仅源码交接就绪）；CC Windows RED / review 尚未执行。**
没有生产修改；没有运行测试、API、SSH、GUI；没有 worktree、commit、push、reset、subagent 或删除。
使用 systematic-debugging + TDD，停在测试源已写/compile-only 阶段，不宣称完整 RED→GREEN。
实际测试、诊断输入、截图判读和 review 只能由 **CC+GLM** 执行。

## 1. 权威证据和来源边界

最新权威 oracle：`/private/tmp/windows-multiclick-diagnosis-20261008/oracle-evidence-remote-authoritative.jsonl`

| 文件 / 产物 | Bytes | SHA256 |
|---|---:|---|
| 权威 oracle（cleanup 后） | 42578 | `2fe4a01115cce6cdddda1aae133111b598516638638f123682f95835c5061add` |
| 原本地 oracle（cleanup 前快照） | 42292 | `eb30833e415959ad8cb1d62725f9351d78d5ddf7fa21578344bdd0248b139e3d` |
| 原始内层 transcript | 6025729 | `c2aa99fca0f3aa66f8a1ed53ed702966433cf2c98b5c17f610e21784f7746951` |
| 部署旧不可变 fixture PE（既有 CC 留证，作者未重新取 PE） | — | `3a5fa228477e176e42d05a614f20ad8e8187a7e6cd6217b84ee9411514f526ff` |

作者仅作文件读取/散列核对：权威 oracle 的前 42292 字节与原快照完全一致，
新增一行 `session_close`，时间 `2026-10-08T03:42:55.2937960Z`（北京时间 11:42:55），
十例 `gesture_check` 不变。旧快照和 transcript 与 `/private/tmp/windows-current-multiclick-cc-20261008/` 副本逐字节相同。
回归程序**只接受权威版 SHA**，传旧快照应 exit 2（前置条件错误），不能冒充 RED。

依据：原始 oracle/transcript 优先，`.agents/reports/windows-multiclick-evidence-cc-20261008.md` 为 CC 恢复勘误。
原报告称 04 没选词、08 没收到右键的归因错误；不采用。
截图位于 `.agents/runs/windows-current-multiclick-cc-20261008/screenshots/`，作者未查看/判读。
“report the”截断为 CC 报告事实，尚未由本轮 Windows 尺寸回归验证。
transport 失败与调度日志计数问题归 transport author，不在本写集合。

## 2. 有界根因

### 04：fixture 词选择判据过窄，不是缺少原生双击

- oracle nonce `EUBN9H`：WM `513,514,515,514` = DOWN,UP,DBLCLK,UP；
  `native_count` 按 down 为 `1,2`，两对释放齐全；位置 `(39,27)`，间隔约 94.20ms < 500ms。
- `selection` 原样为 **`alpha `**，UTF-16 `0061 006C 0070 0068 0061 0020`，6 个字符；
  `matched:false`、`released:true`。不是空字符串，不是视图推测。
- `GestureJudge.cs:47-50` 用 `obs.Selection=="alpha"`，直接解释失败。
- `GestureNative.cs:72-90` 的 `GestureSentence` 是 ReadOnly、single-line TextBox，
  原生 `base.WndProc` 处理选区；`GestureForm.cs:110-125` 把 `SelectedText` 原样送 judge/log。
- transcript 行 780 是 `count:2` 的 `case4-double-click-alpha`；899 才 Check。
- 下游 `scripts/analyze-gesture-gui.py:198` 同样精确拒绝非 `alpha`，是后续需独立授权的 analyzer 判据问题；本轮不改。

**拟议局部合同（待 CC RED 后批准实现）**：仅此 Windows 原生 EDIT 词例允许精确
`alpha` 或 `alpha\u0020`，保留原始选区；不使用 Trim/Unicode normalization，不接受
前置空格、多空格、TAB、NBSP、错误词或跨词。原生跨度/当前具体系统行为须由已批准诊断补证，
不把这次尾空格观测宣称为所有 Windows 控件的永久通用规则。

### 05：输入交付达成；整行目标未达成；现有 fallback 本身不足

- oracle nonce `4EMV4Z`：WM `513,514,515,514,513,514`，3 down / 3 up，down count `1,2,1`，一个 DBLCLK；
  位置 `(260,27)`，相邻 down 约 92.90ms 和 92.98ms，释放完整；最终 **selection=""**。
- transcript 行 1199 派发 `count:3`，行 1284 Check；不能归因工具没送第三击。
- `GestureCases.cs:41` 原目标是 select **whole line**；`GestureCases.cs:105-107` Windows note
  却写 “pass = 3 downs + nonempty visible selection”；`GestureJudge.cs:48` 接受任何非空字符串。
  因而任意词乃至单个空格均可能被误当整行目标成功。
- `GestureForm.cs:93-94` 又追加 “whole-line selection is not guaranteed”，没有同步明确区分
  input-delivery 与 user-semantic-goal 的判定状态。
- 现有窗口只记录 Check 的最终 SelectedText，**没有每条消息后的原生选区范围**，
  无法仅凭最终空字符串证明第三 DOWN 在哪一步消除了选区。

**结论分层**：已证明整行语义未满足、输入三击已交付；native EDIT 与整行预期的能力不匹配
是有官方默认处理依据的候选分类，不是这台机器整个过程已重新观察的结论。
不要放宽成 “3 presses 即 pass”，更不要把 empty/partial selection 写成语义成功。

### 指令截断：固定高度 + 运行时追加 caveat

- `GestureForm.cs:30-36,44-45`：900×760、AutoScaleMode.None、instruction 860×75、Segoe UI 20pt Bold、AutoSize=false。
- `StartTrial:93-94` 追加 Windows-only 长句，75px 不随内容增长；下一个 live label 在 y=190，
  instruction 下边缘 y=185，仅5px间隔。因此单独加高也可能侵占 live/canvas，不能盲改数字。
- trial oracle 日志在 `:98-101` 仅写 `c.Instruction`，**不包含 UI 追加后完整字符串**，
  oracle 的短 instruction 不是完整渲染文本证据。
- CC 报告已指出 case05 在 “report the” 后被裁；本轮新增回归读取实际生产 Label.Text/Font/Bounds，
  做 Windows 原生 wrap measurement、preferred height、边界和可见 sibling overlap 检查。
- 后续方案：测量实际完整文本后分配布局/重排下方内容，或审批后把 caveat 拆成独立可读 capability 状态；
  不以偷偷删语义目标、减成不可读字体或测试专用替代 label 消除 RED。

### 08：agent omission，明确不补内核或 fixture

- transcript 行 2105 left → 2109 right → 2162 Check；**无第三个 LEFT**。
- oracle nonce `6JLL7Q` 的 event3/4 为真实 `WM_RBUTTONDOWN/UP`（516/517），不支持“右键没收到”。
- `GestureJudge.cs:54` 要求 left/right/left，拒绝当前两次按下是正确的。
- 新回归保留该拒绝和右键存在性；不执行新的输入、不添加补救动作。

## 3. Windows EDIT 原生合同依据及待批准问题

2026-10-08 只读查阅 Microsoft Learn 官方文档：

- **About Edit Controls**，`/windows/win32/controls/about-edit-controls`：
  默认 `WM_LBUTTONDBLCLK` 选择光标下的 word；文档没有三击整行保证，也没有给 `alpha` 的尾空格定死跨系统规则。
- **EM_GETSEL**，`/windows/win32/controls/em-getsel`：读取起始及首个未选字符位置，空选区 start=end；
  应使用 pointer 参数的完整32位位置，而不是只取 packed low/high word。
- **EM_SETWORDBREAKPROC**，`/windows/win32/controls/em-setwordbreakproc`：word-break 可由控件/应用定义，
  默认在空格断开。这不是对本例精确尾空格跨度的额外观察证明。
- **TextRenderer.MeasureText**，`System.Windows.Forms.TextRenderer`：测量依赖 proposed size/flags 并包含 glyph padding；
  新回归同时记录 Label preferred size/renderer/actual DPI，不声称纯尺寸即可替代 screenshot oracle。

**精确审批问题（StageB 前必须答复）**：

1. 是否批准将 05 在保留原始 whole-line goal 的同时标成 `unsupported_expectation` /
   `needs_capability`（命名尚未定），且与 delivery success 分开，不计为 semantic pass、不用于凑成功配额？
   建议采用此诚实双层合同，但必须先取得有界 native observation。
2. 若必须仍达成三击整行，是否批准改为明确声明的自定义/其他控件行为（非“native EDIT convention”）？
   这会改变 fixture 功能及 canonical identity，应另开 StageB 设计，不能现在加 selection-manufacturing helper。
3. 是否批准 **一次有界 Windows diagnostic**：用实际 GestureSentence，在 04 双击后和 05 三击结束、
   Check 失焦前后记录 native class/style、ReadOnly/Multiline、DPI/font、EM_GETSEL start/end 和原样 UTF16？
   不重复整套 campaign；不更改设置；不接受空选区为成功。

现有 probe 只观察生产控件，不注入、不设置选区、不替代 WndProc。
pre-native snapshot 为同步；`queued-after-native` **可能已含后续消息**，不能把它假称精确逐消息 postcondition。
observer 开销也可能影响 timing，结果属于诊断而非原 immutable PE campaign。
如需精确每条消息后跨度，须另行批准最小诊断 instrumentation；不能绕过 StageA 修改生产接收器。

## 4. 新增回归源（独立于 shared root build）

均位于 `acceptance-fixture/windows/GestureStageA/`，根构建的非递归 `Gesture*.cs` glob 不会收入它们。

| 文件 | 职责 |
|---|---|
| `GestureStageARunner.cs` | 单独 csc `/main`，Windows guard，exit 0/1/2，JSONL结果 |
| `GestureStageAContracts.cs` | 固定权威 oracle SHA，复用真实 GestureJudge；04 U+0020、负例、05整行负例、08漏击 |
| `GestureStageALayout.cs` | 实际 GestureForm 构造和 StartTrial；读取真实 label，不复制布局实现或改尺寸 |
| `GestureStageANativeProbe.cs` | 可选批准后诊断实际 native EDIT；不注入/制造选区，不给 GUI PASS |

Native probe 的 class 识别包括 EDIT 与 WinForms EDIT superclass wrapper 名称；未知 class 停止下结论。
真实产品 MainForm 的已有入口和其他类只作为编译依赖，没有写入。
31个显式编译输入（27个已有 root C# + 4个新 StageA），无需 NuGet 或 root build 修改。
新的回归不接入产品 `--self-test`，不改变既有 assert / exports / manifests / task SHA pins。

**预期 Windows replay RED（源码推导，未执行）**：

- `04-recorded-native-alpha-U0020`：期望成功，现有 judge 失败。
- `05-partial-is-not-whole-line-"alpha"`、`"alpha "`、`"beta"`、`" "`：期望拒绝，现有 judge 错接受。
- `05-recorded-empty-not-success` 必须维持拒绝；whole sentence 正例和缺少 release 负例保留。
- 04 missing-up / no-DBLCLK / 超时 / 超slop / release-outside 仍须拒绝；所有 selection 输入逐项检查未被改写。
- 08 记录 replay 必须拒绝；“native right present”应满足。
- `05-delivery-only-three-presses` **仅把 area remap 为 target 交给既有通用 triple judge**，
  用于显示 delivery 与 goal 的差异；明确不是 whole-line 成功或原始 GUI 证据。
- 当前源预计 replay 有 **5个合同 RED**。前置条件/解析/源漂移是 blocked（exit2），不是 RED。
- layout预计 05 完整 instruction 的高度回归 RED；实际字体/DPI可能暴露其他例裁切，
  不预填 Windows测量值，也不保证 RED 的准确数量。屏幕不足或反射成员变化是 blocked，而不是 layout RED。

## 5. 只编译结果和冻结产物

本地 Mono C#5 **compile-only exit 0，输出为空，无编译诊断**。没有执行产物，
不是 Mac/Linux 测试，更不是 Windows 编译/运行通过。

- final assembly：`/private/tmp/windows-multiclick-fixture-stage-a-sol-20261008/GestureStageA-final.exe`
  SHA `eeec113e4b6a4580ed3c012550c71a9035b765b0d5f4e68f2119e099b676531e`。
- log：同目录 `compile-only-final.log`；完整 command / compiler SHA / bytes 见 repo run `compile-only.json`。
- source bundle：同目录 `windows-multiclick-fixture-stage-a-sol-20261008.tar.gz`（92200B）
  SHA `5dc142023587981012cce59f49a8ed068d89d4ed6b59ade7213525692fa50210`。
  内含 `source/` 冻结依赖/新增测试/上下文及 `evidence/` 权威 oracle。
- 42文件 source freeze：repo run `source-freeze.sha256`
  文件 SHA `3d914a34eb211f9b32bcfed1282e6f17fdc883105030eb6720c294dad4597cdc`。
- 原root源/构建/task/canonical的30文件before/after hashes **无变化**：`production-preservation.json`。
- 编译 temp、原始 oracle 副本和归档全部在 Data卷 `/private/tmp`，repo只放小源码/报告/指针。
  没有清理/删除任何缓存或旧产物。

冻结关键生产上下文：

| 文件 | SHA256 |
|---|---|
| GestureCases.cs（也等于旧 canonical pin） | `456108a7ab12640ca1abbddc7a27fa2f8923af185e926d1ccb5208050bddbebe` |
| GestureJudge.cs | `63920ae7d9667ac8696220c7e61211be89a2d16e24911b13e63adead6ac299b4` |
| GestureForm.cs | `6772609f0869615ce9ca400f0089a7a22727f5a4b2bc7fc11f39f1b852434082` |
| GestureNative.cs | `686212d5d0d6622504de5e3d6eaadd40a7990209ce1d4ae374abb6a1c8d256d2` |
| tasks/multiclick.md（也等于旧 canonical pin） | `8c2dc2bf472bc2ea1382214c04747e4673c3d155f41542fe33f5f67d737eca88` |

## 6. CC+GLM 精确 Windows 交接命令（本作者未执行）

先由协调者批准 CC RED。bundle通过既有安全传输交给Windows；下面不是本轮执行授权。
全部使用新路径；不覆盖 immutable PE、不更改策略/权限/全局设置、不需要改 root build。

```powershell
# Windows CC console; package already staged by coordinator.
$ErrorActionPreference = 'Stop'
$package = 'C:\Temp\windows-multiclick-fixture-stage-a-sol-20261008.tar.gz'
$expected = '5dc142023587981012cce59f49a8ed068d89d4ed6b59ade7213525692fa50210'
if ((Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
    throw 'BUNDLE DRIFT; stop, not RED'
}
$stage = Join-Path 'C:\Temp' ('windows-multiclick-fixture-stage-a-sol-20261008-CC-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
& tar.exe -xzf $package -C $stage
if ($LASTEXITCODE -ne 0) { throw 'extract failed; stop, not RED' }
$source = Join-Path $stage 'source'
$out = Join-Path $stage 'compiled'
$run = Join-Path $source '.agents\runs\windows-multiclick-fixture-stage-a-sol-20261008'
& powershell.exe -NoProfile -File (Join-Path $run 'cc-compile-only.ps1') -SourceRoot $source -OutDir $out
if ($LASTEXITCODE -ne 0) { throw 'source verification/csc failed; stop, not RED' }
$exe = Join-Path $out 'GestureStageA.exe'
$oracle = Join-Path $stage 'evidence\oracle-evidence-remote-authoritative.jsonl'
if ((Get-FileHash -LiteralPath $oracle -Algorithm SHA256).Hash.ToLowerInvariant() -ne
    '2fe4a01115cce6cdddda1aae133111b598516638638f123682f95835c5061add') {
    throw 'ORACLE DRIFT; stop, not RED'
}
& $exe --replay $oracle | Tee-Object -FilePath (Join-Path $stage 'cc-replay-red.jsonl')
$replayExit = $LASTEXITCODE
Write-Host "REPLAY EXIT=$replayExit (expected 1; inspect exact five named REDs; 2 is blocked)"
Get-FileHash -LiteralPath $exe -Algorithm SHA256 | Format-List
```

`cc-compile-only.ps1` 内置明确 csc：
`C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /langversion:5 /target:exe /platform:anycpu`
`/main:AcceptanceFixture.GestureStageARunner`，references System/Core/Drawing/Forms/Web.Extensions；
按 `compile-inputs.json` 显式输入，并校验完整42文件freeze。
Windows编译的PE SHA必须CC另记，不要求等于Mono产物。

**另行批准 layout/native diagnostic 才执行下面命令**。应在真实用户交互控制台session，
不能SSH非交互桌面或Session0，也不使用 WMI / SSH Start-Process。若通过远端编排启动，
复用已批准 Interactive ScheduledTask 方式，不由作者新增服务/权限/helper。

```powershell
# Existing $stage/$exe from above, only after coordinator diagnostic GO.
& $exe --layout | Tee-Object -FilePath (Join-Path $stage 'cc-layout-red.jsonl')
Write-Host "LAYOUT EXIT=$LASTEXITCODE (1 contract RED; 2 blocked; save exact DPI/font measurements)"

# Separate optional GO. CC+GLM own GUI decisions and screenshot review.
& $exe --native-probe | Tee-Object -FilePath (Join-Path $stage 'cc-native-probe.jsonl')
# diagnostic_only summary is NOT acceptance PASS or native contract validation.
```

有界probe建议：由CC+GLM用GUI逐次Next到04，双击alpha一次，先保存原样选区和截图、再Check一次；
到05三击一次，Check前后分别留选区/截图，然后关闭；不补救、不整套重跑。
要求留 native_class/style、readOnly/multiline、DPI/font、EM_GETSEL、消息序列及选区原样UTF16。
如CC不能取得准确对应快照，应标 unknown/诊断不足，不从异步snapshot推造因果。

CC回传：source freeze核验、Windows csc hash/log、replay exit和失败名、layout实际测量（若批准）、
native诊断（若批准）、GLM截图判断、review意见；协调者收到RED前不授权产品补丁。

## 7. Canonical drift 和后续约束

当前 `GestureCases.cs` 与 `tasks/multiclick.md` 的 hashes 仍等于旧 `canonical-fixture.json` pins。
本轮没有改变 canonical/assert/task identity。StageA子目录未被根构建发现，未重建旧fixture。

**之后若改 GestureCases 的 spec/instruction/platform notes，canonical source drift 必须有意使旧campaign绑定失效。**
不得静默更新pin把新语义塞进旧 PE/campaign；应新版本catalog/manifest/task binding、来源/PE冻结、重新批准新campaign，
旧证据不可继承成新语义的pass。仅改judge/layout也会改变PE/验收语义，须新PE provenance，不能宣称本轮旧PE已修好。

待批准推荐设计是“原始goal + input-delivery + truthful capability + full readable instruction”分开，
不是全局normalize或把缺失selection当完成。
