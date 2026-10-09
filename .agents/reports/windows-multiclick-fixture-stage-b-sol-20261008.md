# Windows multiclick fixture StageB — 2026-10-08

**SOURCE_FROZEN：author handoff 就绪；Windows GREEN、review、GUI 均待 CC+GLM。**
仅授权代码修改和 compile-only。未执行测试/API/SSH/nativeprobe/GUI，未进行实际 review；
无 worktree/subagent/commit/push/reset/delete，无全局设置或权限变更。所有大产物在 Data 卷。

## 1. RED 依据（作者只读取原始留证）

- CC report：`.agents/reports/windows-multiclick-stage-a-cc-20261008.md`。
  `/private/tmp/windows-multiclick-stage-a-cc-20261008/replay-first-evidence/summary.json`：
  native exit **1**、no timeout、stdout 未截断、stderr 0；恰好5个指定合同 RED。
  stdout SHA `f2c1f2be6ac873d0633a86ba24961b91861b069a93f1ae43a5cebedc81602f2a`。
- CC layout report：`.agents/reports/windows-multiclick-layout-red-cc-20261008.md`。
  `/private/tmp/windows-multiclick-layout-red-cc-20261008/layout-first-evidence/summary.json`：
  native exit **1**、Session **1**、no timeout、stdout 未截断、stderr 0。
  原始 measurement：case05 Label **860×75**，preferred/measured **838×111**，96 DPI，Segoe UI 20pt。
  恰好 `multiclick-05-full-instruction-fits-real-label` / `multiclick-05-wrapped-height-fits` 两个 RED。
  stdout SHA `0fc2506a083ef4582ce084663a7cd36f18752c619f51b73e1acf831481564d70`。
- 原始 stdout 并非全文件有效 UTF-8（旧 console codepage 字节）；作者未改写它们。
  原始 SHA、ASCII失败名、numeric measurement 与 summary 核对，不把临时字符解码当选区证据。
- 回放继续固定 cleanup 后42578B权威 oracle：
  `2fe4a01115cce6cdddda1aae133111b598516638638f123682f95835c5061add`。
  旧 oracle、RED records、StageA archive/freeze/report 均未修改。

## 2. 有界修改与不变项

| 文件 | 修改 |
|---|---|
| `acceptance-fixture/windows/GestureJudge.cs` | Windows词选择仅允许 exact `alpha` / `alpha\u0020`；整行仅允许 exact `GestureCatalog.SelectWordSentence`；整行理由明确完整句子要求 |
| `acceptance-fixture/windows/GestureForm.cs` | 改为partial；保存footer按钮引用；在Trial/Check/Finish边界调用布局；保留完整 canonical指令及原caveat，追加明确whole-sentence要求；当前trial `platform_expected` 改为真实整句标准 |
| **新增** `acceptance-fixture/windows/GestureFormLayout.cs`（79行） | 实际Label preferred/wrapped measurement；实际可见内容逐行布局；footer动态测量；working-area边界保护 |
| **新增** `acceptance-fixture/windows/GestureStageB/GestureStageBLayoutContracts.cs`（91行） | 独立Windows-only补充布局合同，调用真实Form/Check/Finish，不注入输入；不进入root产品构建 |

### 语义

- 04的count、time/slop、left-button、area、DBLCLK、ordered release等原守卫**全部保留**。
  不trim、不normalize、不改写 `SelectedText` / observed / raw logs，不扩展至别的case或平台。
- 05必须原样整个 `alpha beta gamma delta epsilon`；partial/empty均不能成功。
  旧权威oracle的05空选区应继续 MISMATCH。没有合成选择、补点击、delivery-only成功或unsupported分类。
- 08没有修改。原left/right缺第三left的记录继续失败，不修kernel/模型遗漏。
- 原始canonical goal文本未改；完整旧Windows caveat亦保留，新增一句
  `Only the whole sentence satisfies this goal.`，不是缩短文案规避裁切。
- 同一 `WholeLineRequirement` 用于judge理由和当前trial的 `platform_expected` **字符串值**。
  字段名/类型/事件结构未改，未引入API schema或capability状态。

### 布局

- 按实际Font/Label宽度测量 `GetPreferredSize` 与 `TextRenderer.MeasureText`，取足够高度并加2px余量。
- 标题、nonce、完整instruction、live行、当前可见canvas/EDIT/scroll区、status、按钮依次安排。
  文本例不再预留隐藏的300px canvas空白；宽860×高300输入canvas、425×260scroll panel均不缩放。
- 字体保持原值（instruction Segoe UI 20pt Bold；EDIT 24pt）；不缩字号，不隐藏/ellipsis全文。
- live行预量最大int计数字符串，防止记录过程中变长导致重排。布局**不在 Record/WndProc 中执行**，
  只在 Trial/Check/Finish边界发生，避免多击/拖动中移动目标。
- footer按实际文本测量，按钮保留至少110×40，右侧排列；不让加高instruction挤占下层控件。
- 最小client仍900×760。仅所需内容超出时考虑增长；先比较实际带边框window尺寸与primary working area，
  放不下就明确报错，不能静默离屏或压缩输入几何。窗口位置约束在working area内。
  **96 DPI实际是否保持900×760、所有控件完整可见，待CC补充布局测试/GUI确认，不作作者运行声明。**

### 旧断言与遗留矛盾（修改前已明确报告）

- `GestureSelfTest.cs:58` 的 `word-selection-native-valid` 使用精确alpha，仍有效。
  没有旧自测要求partial/nonempty通过select_line；**没有修改任何旧自测断言**。
- 原4个StageA测试源逐字节不变。其pending `capability_diagnostic` 输出也是旧测试代码，
  不表示StageB产品新增分类；nativeprobe仍明确延期，命令不调用它。
- 冻结 `GestureCases.cs:107` 的旧Windows note仍写“pass = 3 downs + nonempty”。
  按授权不改catalog/canonical/pins；实际Form trial使用新的真实标准覆盖该运行时文案。
  **旧manifest/export note仍属已知遗留差异**，不把它当新产品判据。
- `scripts/analyze-gesture-gui.py:198` 仍仅接受alpha（未授权修改）。将来04原样alpha+space时，
  旧analyzer可能继续报告word selection absent；不能把该旧口径误称新fixture失败，也不能宣称整个analyzer pipeline已修。

## 3. 编译与冻结

本地Mono/csc编译三份最终PE，**全部exit 0，编译log均为空**。没有执行任何PE；
不构成Windows编译/测试/GUI通过。C#5、System/Core/Drawing/Windows.Forms/Web.Extensions references。

Data目录：`/private/tmp/windows-multiclick-fixture-stage-b-sol-20261008/`

| 编译产物 | bytes | SHA256 |
|---|---:|---|
| `ComputerUseAcceptance-stage-b.exe` | 198144 | `4290f76da5e8bb88e7ad9932be505cf5caed5da1aa8a30e660bda95885e03207` |
| `GestureStageA-linked-stage-b.exe` | 233984 | `58356268623f5d2ec4d454c36f93e1c1ec27bcf4e20748f17bd7240ccb5fc7d8` |
| `GestureStageBLayout.exe` | 209408 | `43e92c581f87940db0417f504fe27b623afdd7848845e044b233b73ddfeb6f30` |

- Bundle：`windows-multiclick-fixture-stage-b-sol-20261008.tar.gz`，354306B。
  SHA **`bf1618e86f467514b9eca6e9336aa94c0dcfb92c1e96675b11ac4c8517a1ea26`**。
- Source-freeze：48文件；manifest SHA **`722f2ce43ed2bc847cf8212c8257ee78a193cb7e0c41980cb0f8ecfc8923a2ed`**。
- repo run：`.agents/runs/windows-multiclick-fixture-stage-b-sol-20261008/`：
  `before.json`、`preservation.json`、`compile-inputs.json`、`compile-only.json`、`evidence-pointer.json`、
  `source-freeze.sha256`、`artifacts.sha256`、`cc-compile-only.ps1`、`readiness.json`及小plan。
- Data bundle包含冻结source、compile-only PE、权威oracle与两份CC RED stdout/summary的原字节副本。
- protected baseline中仅 `GestureJudge.cs` / `GestureForm.cs` 改动；新增两个代码文件。
  原4个StageA源、旧StageA run文件、其他root源、自测、build脚本、catalog/task/canonical/analyzer均不变。
- 旧StageA freeze仍指向旧production hashes：应在旧immutable bundle内复核，不可拿它在有意变化的StageB checkout
  上通过重写pin来“修复漂移”。新PE不能冒充旧 `3a5fa228…526ff` fixture或继承旧campaign通过记录。
- 以后若改GestureCases，canonical source drift仍须有意使旧campaign绑定失效；本轮不改它。

## 4. CC+GLM Windows精确交接命令（作者未执行）

所有运行与review由CC+GLM，使用新目录。先审阅源码及冻结，再执行。
以下运行示例要求在**真实用户的interactive PowerShell console**中；GUI/layout不能在Session0。
远端编排可复用已审阅的Interactive ScheduledTask launcher，但不能SSH Start-Process/WMI。
不改执行策略；若脚本策略阻止执行，应报告blocked而不是改全局设置。

### 4.1 解包、原生csc编译与产物身份

```powershell
$ErrorActionPreference='Stop'
$package='C:\Temp\windows-multiclick-fixture-stage-b-sol-20261008.tar.gz'
if ((Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant() -ne
    'bf1618e86f467514b9eca6e9336aa94c0dcfb92c1e96675b11ac4c8517a1ea26') { throw 'BUNDLE DRIFT' }
$stage=Join-Path 'C:\Temp' ('windows-multiclick-fixture-stage-b-CC-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
& tar.exe -xzf $package -C $stage
if ($LASTEXITCODE -ne 0) {throw 'extract failed'}
$source=Join-Path $stage 'source'
$run=Join-Path $source '.agents\runs\windows-multiclick-fixture-stage-b-sol-20261008'
$out=Join-Path $stage 'native-compiled'
& powershell.exe -NoProfile -File (Join-Path $run 'cc-compile-only.ps1') -SourceRoot $source -OutDir $out
if ($LASTEXITCODE -ne 0) {throw 'source verification/csc failed; not test result'}
$artifacts=Get-Content -LiteralPath (Join-Path $out 'native-artifacts.json') -Raw | ConvertFrom-Json
foreach ($a in $artifacts) {
    if ((Get-FileHash -LiteralPath $a.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $a.sha256) {throw 'PE DRIFT'}
}
$oracle=Join-Path $stage 'evidence\oracle-evidence-remote-authoritative.jsonl'
if ((Get-FileHash -LiteralPath $oracle -Algorithm SHA256).Hash.ToLowerInvariant() -ne
    '2fe4a01115cce6cdddda1aae133111b598516638638f123682f95835c5061add') {throw 'ORACLE DRIFT'}
```

脚本使用 `C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe`，显式28/32/29编译输入，
`/main`分别 Program / 原StageARunner / 新StageBLayoutContracts，product为winexe，tests为exe。
Windows编译PE哈希另行留证，不要求等于Mono PE。原StageA测试类main未修改。

### 4.2 bounded replay、full self-test、layout和补充布局

以下只包装已有冻结 `scripts/windows-test-runner.ps1`，保留first stdout/stderr/summary原字节；
首次失败即停，不自动修复重跑。fixture self-test是全部既有suite，不是仅gesture子集。

```powershell
$session=(Get-Process -Id $PID).SessionId
if ($session -eq 0 -or -not (Get-Process explorer | Where-Object {$_.SessionId -eq $session})) {
    throw 'Run from the active user desktop console; no Session0 layout'
}
$runner=Join-Path $source 'scripts\windows-test-runner.ps1'
function Invoke-FrozenFixtureCheck([string]$Name,[string]$Mode,[string[]]$ArgsForExe,[int]$Seconds) {
    $artifact=@($artifacts | Where-Object {$_.mode -eq $Mode})
    if ($artifact.Count -ne 1) {throw 'artifact mode mismatch'}
    if ((Get-FileHash -LiteralPath $artifact[0].path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $artifact[0].sha256) {throw 'PE DRIFT'}
    $cfgPath=Join-Path $stage ($Name+'-config.json')
    $evidence=Join-Path $stage ($Name+'-first-evidence')
    if ((Test-Path -LiteralPath $cfgPath) -or (Test-Path -LiteralPath $evidence)) {throw 'refuse reuse/overwrite first evidence'}
    $cfg=[ordered]@{executablePath=$artifact[0].path;workingDirectory=$stage;evidenceDirectory=$evidence;arguments=@($ArgsForExe);timeoutSeconds=$Seconds}
    [IO.File]::WriteAllText($cfgPath,($cfg | ConvertTo-Json -Depth 4),(New-Object Text.UTF8Encoding($false)))
    & powershell.exe -NoProfile -File $runner -ConfigPath $cfgPath
    $code=$LASTEXITCODE
    Get-Content -LiteralPath (Join-Path $evidence 'summary.json')
    if ($code -ne 0) {throw "$Name exit $code; STOP and preserve first evidence"}
}
Invoke-FrozenFixtureCheck -Name 'replay' -Mode 'stage_a' -ArgsForExe @('--replay',$oracle) -Seconds 30
Invoke-FrozenFixtureCheck -Name 'full-self-test' -Mode 'product' -ArgsForExe @('--self-test') -Seconds 60
Invoke-FrozenFixtureCheck -Name 'layout' -Mode 'stage_a' -ArgsForExe @('--layout') -Seconds 30
Invoke-FrozenFixtureCheck -Name 'layout-controls' -Mode 'layout_controls' -ArgsForExe @() -Seconds 60
```

CC须检查native_exit_code=0、timed_out=false、无截断、layout两项root_session_id等于当前active console。
运行进程存在/runner exit不替代实际contract输出；exception/exit2是blocked，不冒充合同通过。

**预期（未执行）**：

- 原StageA replay：5个RED转绿；raw-preserved和所有negative controls继续通过。
  `05-recorded-empty-not-success` / `08-recorded-final-left-omission-rejected`通过表示**拒绝正确**，不是05/08任务成功。
- 原StageA layout：case05完整文本符合新实际高度、无越界/overlap；保存实际measurement而非填预期值。
- full self-test：无旧断言改写，保存实际总数与逐条结果。
- 补充布局：真实三suite ×10 trial的ready / Check-with-no-input / Finish状态；
  所有可见controls在client内、pairwise无重叠、Label preferred fits；输入geometry/字体不变；
  multiclick 96 DPI client必须仍900×760。此测试没有注入input，不是GUI成功证明。
- 无nativeprobe执行，也不从测试诊断输出提升为unsupported事实。

### 4.3 新fixture GUI验证（上面通过且协调者GO之后）

```powershell
$product=@($artifacts | Where-Object {$_.mode -eq 'product'})[0]
$gui=Join-Path $stage 'gui-verification'
if (Test-Path -LiteralPath $gui) {throw 'GUI evidence directory must be new'}
New-Item -ItemType Directory -Path $gui | Out-Null
& powershell.exe -NoProfile -File (Join-Path $source 'acceptance-fixture\start-windows.ps1') `
    -ExePath $product.path -ExpectedHash $product.sha256 -EvidenceDir $gui -Suite multiclick
if ($LASTEXITCODE -ne 0) {throw 'fixture launch failed; stop'}
# start-windows uses the actual interactive console + new identity-bound ScheduledTask.
# No old fixture PE is replaced. Do not expose source/oracle to the inner visual agent.
```

上述start/stop为bundle中当前repo原字节，hash受本轮freeze保护；它们不是旧campaign的9C44…pin，
CC应按当前冻结源码review，不能冒称沿用旧helper二进制身份。
实际CC+GLM沿既有source-free runner/8个computer工具执行**未修改的**
`acceptance-fixture/tasks/multiclick.md`；不更改task pin、不用旧坐标、模型实际标签必须留证。
本报告不猜测/替换现有host、token、MCP config或inner-runner路径。

GUI必须另留截图/first oracle，尤其：

1. 04重新Observe当前坐标，原子双击一次；`alpha`或`alpha `原样选区与MATCHED应一致，原始末尾U0020可追溯。
2. 05保留原始整行要求；完整说明及新增whole-sentence criterion均可见，live/输入区/status/buttons均无遮挡。
   三击一次再Check一次；若仍为空/partial必须MISMATCH，禁止补动作/制造选择或标unsupported。
3. case05前后各一次Observe确认布局切换不裁切；Check后的完整理由/Finish完整状态必须可读。
4. 08若模型再次漏最后left，原样失败；真实right到达不可错误归因为内核故障。
5. API count0 negative不算输入成功；suite visited不等于10/10。新fixture的GUI结果不继承旧campaign。

GUI结束后只用自己的新run-record清理；下列stop保留全部文件，不删除证据：

```powershell
$records=@(Get-ChildItem -LiteralPath $gui -Recurse -Filter 'run-record.json' -File)
if ($records.Count -ne 1) {throw 'ambiguous owned fixture record; stop cleanup'}
& powershell.exe -NoProfile -File (Join-Path $source 'acceptance-fixture\stop-windows.ps1') -RecordPath $records[0].FullName
```

## 5. 当前结论与下一门禁

已交付受限StageB实现、三份compile-only PE、原样StageA回归和新增布局coverage、冻结及精确CC命令。
**没有Windows GREEN/GUI完成声明。** CC+GLM review须重点检查runtime note与旧catalog差异、
无输入期间重排、实际96DPI minimum viewport、所有可见footer状态与原样选区。

下一步仅CC+GLM审核/执行；失败保留first evidence并回报。nativeprobe与unsupported分类继续延期。
