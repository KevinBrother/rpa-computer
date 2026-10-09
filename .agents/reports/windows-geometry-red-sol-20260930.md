# Windows geometry StageA — RED SOURCE_FROZEN / STOP

日期：2026-09-30，Asia/Shanghai。仅交付StageA需求测试源码，不实施StageB。
共享工作区，无worktree/commit/push；未执行测试/self-test/export/parity/程序集/GUI/截图/SSH/CC/其他agent。
仅允许的C#5 compile-only已完成。Rust EOF owner的源码、测试、runner均未修改。

## 1. 已读计划与边界

已读：
- `docs/superpowers/plans/2026-09-30-windows-geometry-and-eof.md` 全文；
- `docs/superpowers/specs/2026-09-30-complete-computer-actions-design.md` 全文；
- `docs/superpowers/plans/2026-09-30-complete-computer-actions-plan.md`；
- 当前BasicArguments/MainForm/build helper与旧FocusSelfTest的StageA写法。

后续geometry边界已保留但**尚未实现**：07/08需人工环境/前后topology证据，09自有target移除不是系统topology变化，
06纯映射须not_gui，10必须检查实际PNG字节而非JSON尺寸自证；真实DPI、截图/输入、十次有效trial等门禁不能用纯测试替代。
不改其他suite、旧63/basic20/focus10、native text默认策略。不给Rust/EOF写集加代码。

## 2. 当前基线与真实缺口

已只读前轮CC报告`.agents/reports/windows-native-text-policy-retest-cc-20260930.md`及其协调纠正：
前轮Windows self-test实际237 PASS / 0 FAIL；237=178既有项+35首轮native-text项+24语义回归项。
这是**前轮CC结果**，不是本轮自行运行的结果；本轮保留这237项的源码、顺序和名称。

当前`BasicArguments.Parse`真实行为（静态源码核对）：
- suite白名单尚无`geometry`，会抛`ArgumentException: unknown suite: geometry`；
- 参数白名单尚无`--export-geometry-cases`，会抛`ArgumentException: unknown argument: --export-geometry-cases`。

本轮没有修改parser、没有添加GeometryExportPath或未来类型stub，未放宽既有参数规则。

## 3. 唯一源码写集：3文件

### 新增 `acceptance-fixture/windows/GeometrySelfTest.cs`

新增两个纯需求断言，都调用**现有BasicArguments.Parse**，捕获真实ArgumentException并将消息写入FAIL详情：

1. `geometry-args-accepts-geometry-suite`
   - 调用`Parse(new[]{"--suite","geometry"})`；
   - 断言返回的Suite必须Ordinal等于geometry。当前抛错，observedSuite没有得到值，需求断言自然失败。
2. `geometry-args-accepts-independent-geometry-export`
   - 调用`Parse(new[]{"--export-geometry-cases","geometry-stage-a-never-written.json"})`；
   - 断言确实返回解析结果，且SelfTest/ExportPath/PlatformExportPath/FocusExportPath这些旧console模式均未被借用；
   - 当前抛unknown argument，解析结果不存在，需求断言自然失败。

不是hardcode false，不依赖编译失败，也没有创建控件或文件。
`geometry-stage-a-never-written.json`只是字符串参数，从未打开/检查/写入。
StageA仅证明要求parser接受独立模式且不借用旧模式；不声称已验证未来导出路径保存或manifest内容，后者属于StageB。

### `acceptance-fixture/windows/MainForm.cs`

仅在原`NativeTextSelfTest.Run()`之后追加`GeometrySelfTest.Run()`注册。
原237项和native-text默认接线未修改，self-test仍在任何GUI初始化之前。

### `acceptance-fixture/build-windows.ps1`

仅新增`Geometry*.cs`源列表并加入现有Framework csc输入，仍保留System.Web.Extensions等既有引用。
没有parser/routing/export/GeometryForm或其他产品行为接线。

## 4. 预期Windows RED（不是已执行结果）

在前轮237项无回归的前提下，源码预期：

```text
FAIL: geometry-args-accepts-geometry-suite ... unknown suite: geometry
FAIL: geometry-args-accepts-independent-geometry-export ... unknown argument: --export-geometry-cases
SELF-TEST FAILED (2/239 checks failed)
```

预期native exit=1、两项新增需求失败、原237 PASS保留；必须由CC实际stdout/summary确认。
如果出现编译失败、超时、其他断言失败或数量不符，不能当作本阶段预期RED，应保留原始证据并报告。
本轮未运行任何测试，因此没有自行声称上述运行结果已发生。

## 5. Compile-only与冻结清单

临时独占目录：`/tmp/windows-geometry-red-sol-20260930-weh0_jqi/`。

使用当前本机csc（Mono包装，非Windows Framework csc）仅编译：

```text
csc /nologo /langversion:5 /target:winexe /out:<tmp>/ComputerUseAcceptance.geometry-red.compile-only.exe /reference:System.dll,System.Drawing.dll,System.Windows.Forms.dll,System.Web.Extensions.dll acceptance-fixture/windows/MainForm.cs acceptance-fixture/windows/Cases.cs acceptance-fixture/windows/Gesture*.cs acceptance-fixture/windows/Basic*.cs acceptance-fixture/windows/Focus*.cs acceptance-fixture/windows/NativeText*.cs acceptance-fixture/windows/Geometry*.cs
```

**compile-only exit 0，日志空。** 未执行生成exe，也没有执行Windows helper。
本机编译产物不是CC待验收的Windows exe，不部署/复用；仅源码构建可交CC。
编译exe SHA256：`ddf9e61397af50d1893ca4903763738ede25398410d2d38c2428f73653799d54`。

新报告附件（不覆盖任何旧目录）：
`.agents/reports/windows-geometry-red-sol-20260930-artifacts-230944/`

- `compile-commands.txt`、`compile.log`、`compile-exit.txt`、`compile-exe-sha256.txt`：实际命令/编译证据。
- `changed-source-sha256.txt`：本轮3文件。
- `compile-source-sha256.txt`：完整23个C#编译输入。
- `fixture-source-sha256.txt`：33个fixture/既有诊断与parity依赖文件。
- `frozen-source-sha256.txt`：完整35个CC快照输入（上述33个加只读runner ps1/Runner.cs）。
- `MainForm.cs.stage-a.diff`、`build-windows.ps1.stage-a.diff`：相对237基线的最小diff。
- `before-sha256.json`、`protected-fingerprints.json`：除获准MainForm/helper外30个原文件指纹全部不变，含BasicArguments、
  原所有自测/产品模块、旧parity与Mac目录源码、已冻结Python分析器与测试。
- `frozen-at.txt`、`compile-location.txt`：精确时间与临时产物位置。

本轮源码SHA256：

```text
ef3e484291eccf45ba1a9df58397cd869af9a291f4bc977dace3737e26e0b3a1  acceptance-fixture/windows/GeometrySelfTest.cs
0b2a1980d828d6cfd82b126a787fdc5c91444c2b2defe5234503a0e13a8b65e9  acceptance-fixture/windows/MainForm.cs
837f61ea553dc8fa9064c1b231719549dde1d80e3b136a89efd64c85f31bab84  acceptance-fixture/build-windows.ps1
```

## 6. CC Windows StageA验收指令（仅待CC执行）

使用35文件manifest的新冻结snapshot，保留相对路径；不要覆盖旧native-text R2/geometry其他快照/产物。
本阶段仅普通helper构建和`--self-test`，不启动GUI、不运行新export、不开始StageB。

```powershell
# 新Windows源码snapshot根目录，短ps1串行执行。
$ErrorActionPreference = 'Stop'
$root = (Get-Location).Path
$out = Join-Path $env:TEMP ('computer-geometry-red-cc-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $out | Out-Null
powershell -NoProfile -ExecutionPolicy Bypass -File .\acceptance-fixture\build-windows.ps1 -OutDir (Join-Path $out 'buildout') 1> (Join-Path $out 'build.log') 2> (Join-Path $out 'build.err')
$buildCode = $LASTEXITCODE
$buildCode | Set-Content (Join-Path $out 'build-exit.txt')
if ($null -eq $buildCode -or $buildCode -ne 0) { throw "csc exit=$buildCode; unexpected build failure" }
$exe = Join-Path $out 'buildout\ComputerUseAcceptance.exe'
Get-FileHash -Algorithm SHA256 $exe | Format-List | Out-File (Join-Path $out 'exe-hash.txt')
$evidence = Join-Path $out 'selftest-evidence' # leave nonexistent for runner reservation
$config = Join-Path $out 'selftest-config.json'
$cfg = @{ executablePath=$exe; arguments=@('--self-test'); workingDirectory=$root;
          evidenceDirectory=$evidence; timeoutSeconds=120 }
[IO.File]::WriteAllText($config, ($cfg | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows-test-runner.ps1 -ConfigPath $config
$harnessCode = $LASTEXITCODE
$harnessCode | Set-Content (Join-Path $out 'selftest-harness-exit.txt')
$s = Get-Content -Raw (Join-Path $evidence 'summary.json') | ConvertFrom-Json
if ($null -eq $s.native_exit_code -or $s.timed_out) { throw 'native exit missing or timeout; NOT expected RED' }
# Preserve actual native exit; do not turn expected RED into a fake native0.
exit ([int]$s.native_exit_code)
```

CC独立核对：build0，self-test native1且无timeout，双输出完整；只有上述两条FAIL，原237条名称/PASS不缺失。
先保留RED再停，不把任务CLI terminal0与native1混淆；不要在这个阶段改parser使测试通过。

## 7. STOP / 未实现项

**StageA RED SOURCE_FROZEN。** 无剩余StageA源码阻断，编译完成；真实Windows RED pending CC。
`BasicArguments.cs`仍拒绝geometry，GeometryForm/catalog/export实现/判定/analyzer/视觉任务全部未实施。
geometry10、真实PNG字节校验、人工环境/DPI/topology、安全停止等保持StageB计划，不因本阶段而声称支持。
等待协调者读CC真实RED并明确StageB GO；不继续写源码，不自行运行测试，不触碰EOF owner。
