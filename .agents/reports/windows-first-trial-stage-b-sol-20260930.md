# Windows first-trial accounting — StageB sol 交付

**SOURCE_FROZEN / STOP，等待 CC＋GLM 独立 review 和 Windows GREEN。**
冻结时间：2026-09-30T20:06:42.729464+00:00（UTC）。作者角色仅代码实现者。

## 授权与 RED 输入

用户明确授权 StageB；亲读 StageA CC 报告、原始 `summary.json` 与 stderr 终态。
原始 RED 的 native_exit_code=1、outcome=native_failure、timed_out=false，
`Ran 36 tests / FAILED (failures=120)`。用户/CC确认 36 unique 方法、0 ERROR，
build_manifest19 / summarize101 均为 NotImplementedError 缺能力。
外层 exec62898 terminal0 不被当作被测通过。本作者没有重跑 RED，也没有承担独立审查角色。

## 已交付源码（尚未取得运行验证）

- `__init__.py` facade 改为真实 API 入口；`_core/` 六模块按职责划分：
  package exports、bounds、catalog、plan、records、summary。
- 从已批准的 Cases/BasicCases/GestureCases/NativeTextPolicy 和5份任务独立编写
  catalog；生产实现不读取 canonical-fixture.json，不导入测试构造答案。
  固定9个可信路径＋已审 source SHA pins；每次构造/验证都检查原始字节，源漂移
  fail closed；用户给的 sources/path/hash 不决定读取目标，也不能靠重算digest授权改计划。
- 固定12run /120slot /60semantic /95eligible；七动作数量和variant与冻结契约一致的
  实现已写入。所有缺失和非计数slot保留；first失败/重试分栏，所有冲突方排除，
  不采用先到先得。数字候选不是GUI证据，两个验收flag保持false。
- 有界 JSON/严格整数/类型/字段、循环/非有限数/重复JSON key/路径及数量/字节限制。
  evidence refs 只做形状校验，不读取/执行/检查真实证据文件，不声明model/source已核验。
- 薄CLI `plan` / `summary`，只读所选有界regular文件，stdout单JSON；无输出文件选项。
  `ensure_ascii=True` 使Windows stdout不依赖console codepage；按同一ASCII canonical JSON
  计字节预算，不规范化 Unicode。
- facade 的私有包先注册于位置隔离的sys.modules名称再进行relative imports；
  不要求旧exec_module调用者注册facade，不修改全局sys.path。
- 新Windows known-input任务有精确10 payload（含LegacySample），明确JSON解码、
  NBSP/NFD/non-BMP/Tab/CRLF，Ctrl+A用key_chord，无key_press；仅八工具/source-free/
  截图定位/不重试/安全弹窗STOP。known09不是Host全局LF归一。
- 原v1的9个manifest源引用不变；新known-input任务独立冻结，监督者须核对发布hash，
  API不声称自动核验该任务的staging。README明确此边界。
- 新StageB测试25个方法，包含旧exec_module加载、源码漂移、原fixture不可作为生产答案、
  未信任路径不读取、额外类型/冲突/重试、任务payload、CLI cp1252严格编码roundtrip、
  duplicate-key/nonfinite/truncation/oversize与空记录summary。CLI测试只使用owned临时文件，
  每个subprocess timeout20秒；仅CC将执行。

## 保护范围核对

编辑前19/19 StageA冻结文件与原hash一致。唯一授权修改的原文件是stub `__init__.py`；
其原版本仍保留在StageA归档。

- 287个受保护源文件：写前/写后SHA全部不变，包含产品src/crates/Cargo、旧fixture、旧测试、
  旧分析器及runner；没有清理/覆盖既有大量未提交代码。
- 另31个归档/报告/原始RED证据文件全部不变；StageA tar、SHA清单和19份历史快照仍匹配。
- 原36/API/canonicalfixture/runner如下（完整基线与复核见runs目录）：

| 保护文件 | SHA256（未变） |
|---|---|
| `acceptance-fixture/trials/API.md` | `dce62b6d631ae50bf78b14ef40ac055b59fab081d32c3edf7d5e39c4715ea408` |
| `acceptance-fixture/trials/canonical-fixture.json` | `6834b40f30486c2bfc70e181bbbefc4f401ec9fdc03f321ddbc0d89490fa1344` |
| `scripts/windows-test-runner.ps1` | `ad2a607ca0af59983b306ba5f6c32611005669a6b7d385834ccceb027a9572c4` |
| `scripts/windows-test-runner/Runner.cs` | `aced18e616afe881845b2ac233e32a190b7186b69492aaa7be416ac41e972214` |
| `tests/windows_first_trial_accounting.py` | `86a6b5f0d4ce0fdc214a338fd33332a81e700fda297e7931e4a25e2a4714dce0` |

## 仅 compile-only：实际执行记录

```sh
PYTHONPYCACHEPREFIX=/tmp/windows-first-trial-stage-b-pycache.zB05MZ python3 -m py_compile acceptance-fixture/trials/__init__.py acceptance-fixture/trials/_core/__init__.py acceptance-fixture/trials/_core/bounds.py acceptance-fixture/trials/_core/catalog.py acceptance-fixture/trials/_core/plan.py acceptance-fixture/trials/_core/records.py acceptance-fixture/trials/_core/summary.py scripts/windows-first-trials.py tests/windows_first_trial_accounting.py tests/windows_first_trial_accounting_stage_b.py
```

仅此一次编译，exit0；stdout/stderr为空，未遇编译错误。缓存只在新建/tmp目录。
日志保留为 `compile-01-command.txt` / `compile-01-stdout.txt` /
`compile-01-stderr.txt` / `compile-01-result.txt`。**这不是测试通过证明。**
没有运行unittest、API、CLI、Windows、GUI、SSH、socket、CC/subagent；没有worktree、
commit/push/reset，没有review别人的任务。哈希/快照/打包操作不加载被测模块。

## 写集及源码hash

| 新增/授权修改路径 | SHA256 |
|---|---|
| `acceptance-fixture/tasks/windows-known-input.md` | `c9cbeccc1627b22dec0ea5dbc0964466dc9d76ddd76a8544fc2453b20e56236b` |
| `acceptance-fixture/trials/README.md` | `fab58891fac3cfb734ab3753e064cff5d29e6ea648eef6bc752ee9842b785440` |
| `acceptance-fixture/trials/__init__.py` | `c88459e05c4a3df0096e18adf4bed9a6a401fa097a02e99b57ba9b0d2bee9437` |
| `acceptance-fixture/trials/_core/__init__.py` | `c56f898c8a105b28e3c12dc77ac54613979fa224205658e55e0e9dc91e674acc` |
| `acceptance-fixture/trials/_core/bounds.py` | `6ddcf7bfa39933a4215ade9740c9d0f428ea5b3651b739e51b01daff88732cb0` |
| `acceptance-fixture/trials/_core/catalog.py` | `c4abb5a21463d97aeab5c66969e383cc962c5127eb168ee8ff0594102bc044a7` |
| `acceptance-fixture/trials/_core/plan.py` | `16908918b57786ff65f8a6758b9f0919c25b1bd79cc13da474ef525256bab2b5` |
| `acceptance-fixture/trials/_core/records.py` | `48bf903fda951840ec1ad0f5a12d25eae6b2668ea2ea668952b26e9bfc464a1e` |
| `acceptance-fixture/trials/_core/summary.py` | `4b265e83f075c6acdde2ba735acf4a408db51fcd0f4f45ca89e9a7f4e87ea642` |
| `scripts/windows-first-trials.py` | `735d748005714af54b2c4f04ff8cc150b4399616bb58023a8a935d6485f4c3c7` |
| `tests/windows_first_trial_accounting_stage_b.py` | `6ae6f4854933bdd14ffc13118c20af4ded6e63a321478af3895a14d2b51387a6` |

另新增本报告、`.agents/runs/windows-first-trial-stage-b-sol-20260930/` 中的编译日志、
保护hash基线/结果、Windows命令、SOURCE_SHA256SUMS、source-snapshot、tar、FREEZE.json。
本报告不纳入源码tar（避免自引用）；被测源已冻结，不再编辑。

## 冻结

- **35**文件tar：11个新增/授权修改实现交付文件，18个StageA只读输入、StageB任务、
  StageA sol/CC两报告、RED原始summary/stderr、Windows命令文档。
- Archive：`.agents/runs/windows-first-trial-stage-b-sol-20260930/stage-b-source.tar.gz`
- Archive SHA256：`f3021e24b71b1db4aa996b2c2f680309aca94691b31f3435d6c39acca6616a27`
- SOURCE_SHA256SUMS SHA256：`aa7c68ed84ddf362ae64b2aac314f886a0a63205493799f5c6d740b10ee53a68`
- 逐文件路径、hash、角色、字节数：`FREEZE.json`；tar与快照按原始字节封存。
- 新known-input任务hash在上述源表；监督者须在source-free staging前单独核对。

## 精确 Windows 执行交接（未执行）

以下内容同时保存于已冻结 `WINDOWS-COMMANDS.md`：

# CC + GLM planned Windows commands — author has NOT executed these

Preconditions: independently review StageB; verify tar and SOURCE_SHA256SUMS hashes;
extract into a NEW owned source tree; verify every extracted source hash (raw bytes,
no CRLF conversion). Start Windows PowerShell 5.1 in that extracted root. These are
pure-test/CLI commands, not GUI inputs. A successful outer CLI is not test success.

```powershell
$ErrorActionPreference = 'Stop'
$source = (Get-Location).Path
$python = 'C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe'
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$runner = Join-Path $source 'scripts\windows-test-runner.ps1'
foreach ($required in @($python, $powershell, $runner, (Join-Path $source 'scripts\windows-test-runner\Runner.cs'))) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) { throw "Missing: $required" }
}
$batch = Join-Path $env:TEMP ('windows-first-trial-stage-b-cc-' + [Guid]::NewGuid().ToString('N'))
if (Test-Path -LiteralPath $batch) { throw 'NEW batch path required' }
[void][IO.Directory]::CreateDirectory($batch)
foreach ($dir in @('config','inputs','evidence')) {
    [void][IO.Directory]::CreateDirectory((Join-Path $batch $dir))
}
# Never precreate runner-owned evidence leaves; runner reserves each atomically.
function New-OwnedText([string]$Path, [string]$Text) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Dispose() }
}
function Invoke-FrozenRun([string]$Label, [string[]]$Argv) {
    $cfgPath = Join-Path $batch ('config\' + $Label + '.json')
    $evidence = Join-Path $batch ('evidence\' + $Label)
    $cfg = @{
        executablePath = $python
        workingDirectory = $source
        evidenceDirectory = $evidence
        arguments = $Argv
        timeoutSeconds = 180
    }
    New-OwnedText $cfgPath ($cfg | ConvertTo-Json -Depth 4 -Compress)
    & $powershell -NoProfile -NonInteractive -File $runner -ConfigPath $cfgPath
    $wrapperExit = $LASTEXITCODE
    $summaryPath = Join-Path $evidence 'summary.json'
    if (-not (Test-Path -LiteralPath $summaryPath -PathType Leaf)) {
        throw "No authoritative native summary for $Label (outer exit $wrapperExit)"
    }
    $s = [IO.File]::ReadAllText($summaryPath) | ConvertFrom-Json
    if ($wrapperExit -ne 0 -or $null -eq $s.native_exit_code -or $s.native_exit_code -ne 0 -or
        $s.outcome -ne 'success' -or $s.timed_out -or
        -not $s.stdout_drain_complete -or -not $s.stderr_drain_complete -or
        $s.stdout_truncated -or $s.stderr_truncated -or
        $null -ne $s.stdout_drain_error -or $null -ne $s.stderr_drain_error) {
        throw "STOP: inspect raw native summary/stdout/stderr for $Label; do not retry silently"
    }
}
Invoke-FrozenRun 'original36' @('-B','tests\windows_first_trial_accounting.py','-v')
Invoke-FrozenRun 'stage-b25' @('-B','tests\windows_first_trial_accounting_stage_b.py','-v')
Invoke-FrozenRun 'cli-plan' @('-B','scripts\windows-first-trials.py','plan','--campaign-id','c4105120-573e-4a35-8dd5-602d3fa12000','--mode','glm')
# Feed exact captured bytes to summary; do not reencode or overwrite the plan.
$planPath = Join-Path $batch 'evidence\cli-plan\stdout.log'
$recordsPath = Join-Path $batch 'inputs\empty-records.json'
New-OwnedText $recordsPath '[]'
Invoke-FrozenRun 'cli-empty-summary' @('-B','scripts\windows-first-trials.py','summary','--manifest',$planPath,'--records',$recordsPath)
Write-Host "Evidence retained at: $batch"
```

CC must read the original native logs, not just wrapper return values. Expected GREEN
method totals are original36 and new25; unexpected skips/missing tests are not a pass.
The actual plan should contain 12 groups/120 slots/60 semantic/95 eligible; empty summary
must have120 missing, candidate0, candidate_count_met=false, gui_verified=false and
action_trial_gate_satisfied=false. Those are expectations, not results from this author.
Retain native failures, timeouts, truncations and bootstrap errors separately; do not
change the original assertions or shrink the plan. No CLI/test result certifies GUI.


## 仍未完成的门禁

等待CC独立spec→quality review，以及Windows原36＋新25真实GREEN和CLI原始结果。
源码与编译完成不等于计数器运行正确；真实120slot未执行，GUI/trace/模型工具政策/
截图和窗口所有权审核没有完成，不声称action trial门禁满足。

**SOURCE_FROZEN — STOP。**
