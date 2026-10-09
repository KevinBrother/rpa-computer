# Windows known-09 native-text policy sidecar — SOURCE_FROZEN

日期：2026-09-30，Asia/Shanghai。此项仅修正**未来新run**的Windows控件expected声明。
源码与测试源码由实现者交付，执行/独立验收由CC；未运行测试/self-test/export/parity/分析器/程序集/GUI/截图/SSH/其他agent。
未commit/push/worktree，未修改Rust、root、renderer、Mac代码或任何旧run/部署bin。

## 1. RCA：实际证据与调用链

已只读核对：
- `.agents/reviews/layered-known-crlf-diagnosis-20260930.md`
- `.agents/reports/layered-gui-validation-20260930.md` 的 Windows known-input 最终核实/12:10诊断段
- `.agents/runs/layered-win-known-input-final-oracle-20260930.jsonl`
- 同prefix的真实 `transcript-20260930.jsonl`（仅提取指定tool_use字段；不解码图像）

实际 known-09：trial=9、nonce=`S4L86E`，trial时间`2026-09-30T03:48:43.8054472Z`；
`text_check`时间`2026-09-30T03:51:22.9495573Z`，`matched=false`、expected_len=11/got_len=13。
模型真实标签`glm-5.3-flash`；`call_cded8cf2a63f48069ea8d6b0`于`2026-09-30T03:51:16.379Z`
发送`text_input`，request=`k09-text`，raw text与原始task_payload完全相同：

```text
task_payload / tool text / actual_text: 第一行\r\n第二行\r\n第三行
old canonical expected_text:           第一行\n第二行\n第三行
actual UTF16:   7B2C 4E00 884C 000D 000A 7B2C 4E8C 884C 000D 000A 7B2C 4E09 884C
canonical:      7B2C 4E00 884C 000A 7B2C 4E8C 884C 000A 7B2C 4E09 884C
```

实际源码调用链：
`CaseCatalog.KnownInputCases[8]`原始CRLF payload → `FixtureCase`构造器
`ExpectedText = CaseContract.NormalizeCRLF(payload)` → `MainForm.StartTrial`记录canonical expected →
`MainForm.CheckText`读取`_textBox.Text`原样 → `CaseContract.Utf16ExactMatch(got, expected)`逐码元比较。
控件声明为WinForms multiline TextBox、AcceptsReturn/AcceptsTab=true。

证据支持本次窄结论：这份Windows actual是两个CRLF对，旧expected是两个LF，严格比较自然失败。
历史诊断对runtime每对CRLF一次Return的源码链已完成；本sidecar不重新测试或改写Rust链。
**旧9/10失败、旧analysis的input_divergence、旧截图/转录/oracle均不改、不追认通过。**

## 2. 已实施方案与兼容边界

版本：`windows-winforms-known09-crlf-v1`。

- 新`NativeTextPolicy.Expected`只覆盖`SuiteKind.KnownInput`且ID=`known-09`，并要求payload与canonical
  均仍是上面精确固定值；目录漂移直接抛错，须新版本审查。其余suite/ID返回原canonical expected。
- Windows native expected明确为13码元CRLF版本；实际比较仍调用**原封不动**的
  `CaseContract.Utf16ExactMatch`。不Trim、不Unicode normalize、不替换actual或tool payload。
- 没有建立“所有CR/LF都等价”的通用规则；混合CRLF/LF、裸CR、额外CR/LF、尾部换行/空格全部严格不相等。
- `Cases.cs`、两平台目录、CaseContract、GestureExporter及原63parity工具都不修改。
  `--export-cases`仍导出旧canonical expected。B2/focus独立manifest也不变。
- 新独立`--export-native-text-policy PATH`导出版本、平台/控件、作用域、原始payload、canonical/native expected及各自UTF16 hex；
  在GUI初始化之前返回，使用CreateNew拒绝覆盖旧文件，不将其混入legacy63 manifest。
- 新run的session记录policy版本/控件；trial和text_check显式记录`platform`、`native_text_policy_version`、
  `native_control`、`native_override_applied`、`canonical_expected_text/utf16_hex`、
  `native_expected_text/utf16_hex`、`actual_normalized=false`。原有`expected_text`在sidecar中是本次实际native比较值。
  `actual_text/actual_utf16_hex`仍从控件原样输出；新增check_index/check_kind保留first/final。
- 标题可见`Windows native text policy v1`；未改布局尺寸或其他交互。

### 显式编译标记：避免超出写集改构建脚本

用户写集不包含`build-windows.ps1`/BasicArguments，因此本轮没有修改它们。
新policy接线、self-test注册、新导出分支仅在 **`/define:WINDOWS_NATIVE_TEXT_POLICY_V1`** 下启用。
CC须显式包含`NativeText*.cs`与上述标记构建sidecar。**旧build-windows.ps1不启用新policy**：
它仍可编译原source-list，保留canonical行为，不是修复后的GUI候选exe。

这不是运行时模糊降级：sidecar新run必须有版本字段及独立policy导出，缺标记的run不能被新诊断升级。
如未来希望默认build helper启用该策略，需另批该helper的最小接线；本轮不越界。

## 3. 精确写集：仅5份源码

1. `acceptance-fixture/windows/NativeTextPolicy.cs`：新增纯policy、严格单独export参数、独立manifest。
2. `acceptance-fixture/windows/NativeTextSelfTest.cs`：新增纯测试源码。
3. `acceptance-fixture/windows/MainForm.cs`：条件编译下最小expected/log/export/self-test接线；原178注册保留。
4. `scripts/analyze-windows-native-text.py`：新增known-09专属诊断。
5. `tests/windows_native_text_analyzer.py`：新增诊断测试源码。

测试源码先于对应实现写入，**未执行，不声称RED/GREEN**。
C#覆盖旧canonical拒绝CRLF/native严格接受CRLF、LF拒绝、混合及多余换行、目录漂移、其他case保持不变、
旧manifest无overlay、strict export参数。Python覆盖未版本化旧run不升级、nonce/run/session/时间/hex、
raw tool payload、版本/平台、缺结果/错model/request/based_on/cleanup、重试first/final分离及换行负例。

### 新诊断范围

只读复用 `scripts/analyze-basic-input-gui.py` 的JSON/time/tool metadata primitives（须一起复制）。
**不修改或运行**旧layered/focus/B2分析器。
新诊断必须有新session/trial/check的同一run/nonce身份及完整policy元数据；以真实工具call/result时间包络，
唯一text_input请求、request_id、based_on观察metadata、实际glm-5.3-flash及干净dispatched结果关联，不按Nth配对。
原始task/tool payload保持CRLF精确；同时输出`canonical_match`与`native_match`，不合并语义。

`native_exact`只是known-09新策略诊断，不是全套分数或GUI认证；`overall_suite_result=null`、`gui_verified=false`。
缺证据unknown；原始未版本化run只保留历史matched flags，不重新分类。新诊断不读取/导出图像像素，
也不宣称审阅过截图、证明完整GUI政策或所有键盘事件。输出新文件必须不存在。
旧分层分析器仍按canonical-LF工作，不能用它给native-policy新run生成“自动修正”的成功率。

## 4. Compile-only与受保护文件指纹

临时独占目录（不占/Volumes/doc编译空间）：
`/tmp/windows-native-text-sol-20260930-ju_rv2mr/`

本机现有`csc`是Mono包装的C#编译器，不是远端Windows Framework csc。执行的仅为：
- C#5、完整源文件、`/define:WINDOWS_NATIVE_TEXT_POLICY_V1`：exit **0**，编译日志空。
- C#5、原source-list、无标记且不含NativeText*.cs：exit **0**，编译日志空。
- 新Python脚本/测试 `py_compile`：exit **0**，编译日志空。

没有运行任何生成的exe或Python测试/诊断；两个exe/cache留在/tmp，不复制为部署bin。
只把小型compile日志、数值exit、精确命令和hash清单复制到：
`.agents/reports/windows-native-text-policy-sol-20260930-artifacts-2237/`

- `compile-commands.txt`与三个`*-compile-exit.txt`、`*-compile.log`：实际编译命令/结果。
- `MainForm.sidecar.diff`：相对本轮开始时已冻结focus MainForm的精确差异。
- `before-sha256.json`、`protected-fingerprints.json`：41个受保护文件前后SHA相同，包括旧Cases/Gesture/Basic/Focus、
  Mac源码、构建helper、旧analyzers/tests/parity和历史known-input完整raw记录/分析/图像文件。
  这里的hash核对不是运行测试；未打开或解码图像。
- `changed-source-sha256.txt`：5个变更文件；`frozen-source-sha256.txt`：CC构建/诊断/回归需要的完整源文件指纹。
- `frozen-at.txt`：冻结时间；报告自身不纳入源码manifest，避免自引用。

本轮源码指纹：

```text
3d511f01989b672f43719a5c81a90a1f8ca41f94a1ec47b4c6bcc12369785e17  acceptance-fixture/windows/MainForm.cs
533054682960a8611a214c953a9e302fa479ca4feb5d89df33284aac32e37c73  acceptance-fixture/windows/NativeTextPolicy.cs
6b9f13a25756982165477b7c7c026bef601f40e0186c639c338dfdaa323d75c8  acceptance-fixture/windows/NativeTextSelfTest.cs
1e2d6d5dc83774ffcc3dd930cbe846d93208c0f95b6c3a7774c7fa860bca3517  scripts/analyze-windows-native-text.py
edb2760d6269752e272028cbba01f675c1712185c5ed5b19d1df58ef9780dd3e  tests/windows_native_text_analyzer.py
```

## 5. CC Windows独立复验命令（待执行）

在新的冻结snapshot根目录操作，不能复用/覆盖旧focus/B2/known-input run。
源文件按manifest核对；只读旧parity所需`macos/Cases.swift`和`macos/GestureCases.swift`一并带上，不运行Mac。
runner使用CC已验证的`scripts/windows-test-runner.ps1`及同目录`windows-test-runner/Runner.cs`，独立冻结其hash。

```powershell
$ErrorActionPreference = 'Stop'
$root = (Get-Location).Path
$out = Join-Path $env:TEMP ('computer-native-text-cc-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $out | Out-Null
$exe = Join-Path $out 'ComputerUseAcceptance.NativeTextV1.exe'
$csc = 'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$w = Join-Path $root 'acceptance-fixture\windows'
# 此新snapshot的顶层.cs必须与源码manifest一致，含NativeText*.cs。
$src = @(Get-ChildItem -LiteralPath $w -Filter '*.cs' | ForEach-Object { $_.FullName })
& $csc /nologo /langversion:5 /codepage:65001 /target:winexe /platform:anycpu /utf8output /optimize+ /define:WINDOWS_NATIVE_TEXT_POLICY_V1 /out:"$exe" $src /reference:System.dll,System.Drawing.dll,System.Windows.Forms.dll 1> (Join-Path $out 'csc.log') 2> (Join-Path $out 'csc.err')
$code = $LASTEXITCODE
$code | Set-Content (Join-Path $out 'csc-exit.txt')
if ($null -eq $code -or $code -ne 0) { throw "csc exit=$code; STOP" }
Get-FileHash -Algorithm SHA256 $exe | Format-List | Out-File (Join-Path $out 'sidecar-exe-hash.txt')

function Invoke-NativeTextCheck([string]$Label, [string]$Program, [string[]]$Argv) {
    $evidence = Join-Path $out ($Label + '-evidence') # runner requires a new directory
    $config = Join-Path $out ($Label + '-config.json')
    $cfg = @{ executablePath=$Program; arguments=@($Argv); workingDirectory=$root;
              evidenceDirectory=$evidence; timeoutSeconds=120 }
    [IO.File]::WriteAllText($config, ($cfg | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))
    powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows-test-runner.ps1 -ConfigPath $config
    $rc = $LASTEXITCODE
    $rc | Set-Content (Join-Path $out ($Label + '-harness-exit.txt'))
    if ($null -eq $rc -or $rc -ne 0) { throw "$Label runner exit=$rc; preserve first RED and STOP" }
    $s = Get-Content -Raw (Join-Path $evidence 'summary.json') | ConvertFrom-Json
    if ($null -eq $s.native_exit_code -or $s.native_exit_code -ne 0 -or $s.timed_out) { throw "$Label native missing/failure/timeout" }
}

Invoke-NativeTextCheck 'selftest' $exe @('--self-test')
# 原178断言必须逐条保留；新增检查总数以真实stdout为准，不套旧计数。
Invoke-NativeTextCheck 'native-policy-manifest' $exe @('--export-native-text-policy', (Join-Path $out 'native-policy.json'))
Invoke-NativeTextCheck 'legacy63' $exe @('--export-cases', (Join-Path $out 'legacy63.json'))
Invoke-NativeTextCheck 'platform20' $exe @('--export-platform-cases', (Join-Path $out 'platform20.json'))
Invoke-NativeTextCheck 'focus10' $exe @('--export-focus-cases', (Join-Path $out 'focus10.json'))
$python = (Get-Command python -CommandType Application -ErrorAction Stop).Source # 已确认实际python.exe，非Store占位符
Invoke-NativeTextCheck 'native-text-analyzer-tests' $python @((Join-Path $root 'tests\windows_native_text_analyzer.py'))
Invoke-NativeTextCheck 'focus-regression' $python @((Join-Path $root 'tests\focus_gui_analyzer.py'))
Invoke-NativeTextCheck 'B2-regression' $python @((Join-Path $root 'tests\basic_input_gui_analyzer.py'))
Invoke-NativeTextCheck 'legacy63-parity' $python @((Join-Path $root 'acceptance-fixture\tools\check-cases-parity.py'), (Join-Path $out 'legacy63.json'))
```

CC须独立核对：旧63仍known-09 payload CRLF/canonical expected LF；native-policy.json则同payload、
canonical LF、native CRLF、明确policy版本、actual_normalized=false。B220/focus10不掺native policy目录。
可以另用未改的build-windows.ps1在新的outdir构建canonical对照，分别导出63并逐字节比较；
不必GUI运行canonical对照。原生sidecar自测及新分析器测试均须真实native exit0后才称green。

新GUI尚未授权，保持暂停。未来协调者另行批准新known-input run后，明确选**带标记sidecar exe**、新oracle，
保留原始CRLF任务payload和raw actual，不重用历史run；按实际进程hash/session身份启动。
真实新证据离线诊断命令（输出新路径）：

```powershell
python .\scripts\analyze-windows-native-text.py --oracle $NewOraclePath --transcript $NewFullTranscriptPath --output $FreshSidecarAnalysisPath
# exit0仅说明生成诊断；检查first/final/unknown、raw payload及canonical/native分列，不是整套或GUI通过。
```

## 6. Pending / 终态交接

- 源码与显式版本策略已实现，无已知剩余实现阻断；没有放宽原CaseContract或改变legacy63契约。
- Windows Framework csc、所有新增/回归测试与exports/parity **pending CC**；本机compile0不是Windows验收。
- 新run实际控件原始读回/工具参数/视觉结果仍pending；旧9/10失败不能靠代码或pure测试改成10/10。
- 当前默认build helper仍canonical（有意保持写集）；必须使用上面明确标记的sidecar构建命令，否则未启用修正。
- 本次仅known-09，不扩展任意换行输入、geometry/focus新场景，不改root，不等待root/cancel。

**SOURCE_FROZEN：实现工作终结，交CC独立复验；不执行测试或继续追加源码。**
