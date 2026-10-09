# Windows focus StageA — RED SOURCE_FROZEN / STOP

日期：2026-09-30，Asia/Shanghai。
冻结源码指纹生成时间：20:59:13 +0800。
状态：**仅StageA RED测试源码已冻结；未执行RED，等待CC Windows真实RED与StageB GO。**

## 1. 本轮边界

按 `.agents/tasks/windows-focus-fixture-sol-20260930.md` 只新增一个pure parser回归测试、必要注册/sourceinclude。
没有实现focus suite、窗口、路由、catalog、judge、analyzer或生产stub。
没有修改BasicArguments、BasicCases/Judge/Form/Native/SelfTest、旧Cases/Gesture、start白名单、README、其他analyzers/tests、Mac/Linux、Rust/root/renderer。
没有运行测试/self-test/export/parity/程序集、GUI/截图/SSH/CC/其他agent；没有commit/push/worktree/reset。
没有覆盖B2或旧gesture部署bin、snapshot、oracle、旧报告或旧compile-only目录。

用户提供的B2 CC结果为143项自测native0、30项analyzer native0、旧63 parity补只读依赖后0、Windows20 manifest正确。
这些是前轮CC证据，本轮没有重新执行；GUI仍授权弹窗暂停，未调整系统权限。

## 2. 精确源码改动（只有3文件）

1. **新增 `acceptance-fixture/windows/FocusSelfTest.cs`**（33行）
   - 调用现有 `BasicArguments.Parse(new[] { "--suite", "focus" })`。
   - 新契约要求返回 `Suite == "focus"`。
   - 捕获真实 `ArgumentException`，记录原异常message；判定是实际解析所得suite与focus的ordinal比较。
   - 没有硬编码false、缺失未来类型、生产stub、控件构造或输入。
   - 单一测试名：`focus-args-accepts-focus-suite`。
2. **`acceptance-fixture/windows/MainForm.cs`**
   - 仅在现有三组自测后新增一行：`results.AddRange(FocusSelfTest.Run());`。
   - 原143个断言/逻辑及其顺序不改；self-test仍在GUI初始化前返回。
   - 没有新增focus路由，其他入口与export不变。
3. **`acceptance-fixture/build-windows.ps1`**
   - 显式增加 `windows\FocusSelfTest.cs` 路径、存在性检查及csc编译输入。
   - 没有泛化为未来Focus生产模块，无构建参数/发布策略变更。

另外仅写本报告及其独占编译/指纹附件目录：
`.agents/reports/windows-focus-fixture-red-sol-20260930-artifacts-205840/`

该目录内保存本轮修改前MainForm/build的只读副本、before-sha256、仅StageA的精确diff；不会把整个共享脏工作区diff当作本轮改动。

## 3. 预期首次RED（静态推导，不是执行结果）

当前未修改的BasicArguments最后检查BasicCatalog/GestureCatalog/SuiteNames均不认识focus，实际代码路径抛出：

```text
ArgumentException: unknown suite: focus
```

因此CC执行**仅 `--self-test`**后，预期新增输出：

```text
FAIL: focus-args-accepts-focus-suite — required --suite focus; parser rejected with ArgumentException: unknown suite: focus
SELF-TEST FAILED (1/144 checks failed)
```

预期数值native exit：**1**；已有143项应保持原结果，新加1项得到预期144项总数。
**上述数量和FAIL行是待CC确认的预期，不是本轮已运行的结果。**
若出现其他失败/数量变化/异常或构建失败，应保留真实first red，不能将其当作指定缺focus能力的RED。
不要使用 `--suite focus --self-test`：那会先在入口parser失败，尚未执行注册的回归测试，不能替代这个RED。

## 4. 仅compile-only证据

实际执行的命令：

```sh
out=.agents/reports/windows-focus-fixture-red-sol-20260930-artifacts-205840
/Library/Frameworks/Mono.framework/Versions/Current/Commands/mcs \
  -langversion:5 -target:winexe \
  -out:$out/ComputerUseAcceptance.focus-red.compile-only.exe \
  -r:System.dll -r:System.Drawing.dll -r:System.Windows.Forms.dll \
  acceptance-fixture/windows/MainForm.cs acceptance-fixture/windows/Cases.cs \
  acceptance-fixture/windows/Gesture*.cs acceptance-fixture/windows/Basic*.cs \
  acceptance-fixture/windows/FocusSelfTest.cs
```

结果：**exit0**，编译日志为空（无错误/警告）。
附件：`csharp-compile.log`、`csharp-exit.txt`。
这是本机Mono的C#5编译检查，**不是Windows Framework csc结果，也不是任何平台测试**；生成exe从未运行，不用于部署。

## 5. 冻结哈希与CC复制源列表

完整15个编译/构建输入指纹：
`windows-focus-fixture-red-sol-20260930-artifacts-205840/frozen-source-sha256.txt`（相对此报告目录）。

本轮3文件SHA256：

| 文件 | SHA256 |
|---|---|
| windows/FocusSelfTest.cs | `39ad3fc966a721b214ce3ed8d33268481bc41060f9f08519e4a23a94799de653` |
| windows/MainForm.cs | `7c0a166b33ddbeb63772520d65db56bbdb3fdb2684eea9706ef4513727e39d4d` |
| build-windows.ps1 | `3cd12ebfda925313ad6904683e563d100378a8521d6ef43c24e66d0b2ae5cf57` |

未修改parser的SHA256：
`BasicArguments.cs = 07f0a535bae688a9b10c2e3349a984ffc4163d81838fa0e5fcb2b23f4fcd51a5`。

CC新Windows源码快照必须带齐以下文件，保留相对路径：

```text
acceptance-fixture/build-windows.ps1
acceptance-fixture/windows/MainForm.cs
acceptance-fixture/windows/Cases.cs
acceptance-fixture/windows/GestureCases.cs
acceptance-fixture/windows/GestureForm.cs
acceptance-fixture/windows/GestureJudge.cs
acceptance-fixture/windows/GestureNative.cs
acceptance-fixture/windows/GestureSelfTest.cs
acceptance-fixture/windows/BasicArguments.cs
acceptance-fixture/windows/BasicCases.cs
acceptance-fixture/windows/BasicForm.cs
acceptance-fixture/windows/BasicJudge.cs
acceptance-fixture/windows/BasicNative.cs
acceptance-fixture/windows/BasicSelfTest.cs
acceptance-fixture/windows/FocusSelfTest.cs
```

## 6. 待CC Windows执行（这里只列命令）

在新的固定源码快照中，用独立短 `.ps1`；保留stdout/stderr及真实数值退出码，不复用任何旧部署或本机compile-only exe。

```powershell
$out = Join-Path $env:TEMP ('computer-focus-red-cc-' + [Guid]::NewGuid().ToString('N'))
powershell -NoProfile -ExecutionPolicy Bypass -File .\acceptance-fixture\build-windows.ps1 -OutDir $out
$buildCode = $LASTEXITCODE
if ($null -eq $buildCode -or $buildCode -ne 0) { throw "csc failed/exit missing: $buildCode" }
$exe = Join-Path $out 'ComputerUseAcceptance.exe'
& $exe --self-test 1> (Join-Path $out 'self-test.log') 2> (Join-Path $out 'self-test.err')
$testCode = $LASTEXITCODE
if ($null -eq $testCode) { throw 'native exit missing' }
$testCode | Set-Content (Join-Path $out 'self-test-exit.txt')
# 此StageA预期native1；CC须核对失败确为focus parser断言，且其余143结果未回归。
# 如需通过调用脚本回传native exit，保存日志后 exit $testCode，不能转成假green。
```

## 7. STOP / 未实现项

StageA无剩余源码阻断，当前已冻结。真实CC Windows RED尚待执行。
**StageB未开始**：focus01..10的原生UI/catalog/judge/export/analyzer/视觉任务均没有编写或伪造为可运行。
原parser继续拒绝focus是本阶段有意保留的产品缺口，不以修改白名单制造green。
等协调者提供CC真实RED及明确GO后才进入StageB；不扩展cancel/geometry/multi/crossapp，不等待时自行继续实现。
