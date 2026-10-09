# Windows native-text known-09 semantic assertion repair — SOURCE_FROZEN

日期：2026-09-30，Asia/Shanghai。仅修CC首个真实RED；收到WRITE GO后才写源码。
本轮只实现/compile-only，未执行任何测试、自测、导出、分析器、程序集、GUI、SSH或其他agent；无commit/push/worktree。
**CC上一轮exec terminal0不等于测试通过：真实结果是212 PASS / 1 FAIL / 213，native_exit=1。**

## 1. 已确认的失败与RCA

原始证据只读保留：
`.agents/runs/windows-native-text-policy-cc-20260930/computer-native-text-cc-9eee4b27789e4a70b8ba8a167c5ecc72/ntout/selftest-evidence/`

- `stdout.log` 第205行唯一失败：`native-text-legacy63-has-canonical-known09`；末行为`SELF-TEST FAILED (1/213 checks failed)`。
- `summary.json`：native_exit_code=1、timed_out=false；Windows default csc0由CC原报告记录。
- 后续exports/analyzers按first failure未执行，不能宣称它们通过。
- CC报告`.agents/reports/windows-native-text-policy-cc-20260930.md`未修改；原32输入manifest及所有前轮附件未覆盖。

源码确认的原因：
1. `CaseExporter.Escape`显式将LF写成JSON短转义`\n`。
2. `GestureExporter.ManifestJson`直接保留CaseExporter生成的旧文本目录，再追加gesture目录；不重编码旧known-09字段。
3. `GestureExporter.JsonString`把所有控制字符写成`\uXXXX`，因此LF变成`\u000a`。
4. 原新增断言`oldManifest.Contains(GestureExporter.JsonString(canonical))`在JSON序列化字符层查找，
   把两种合法且等义的JSON写法误判为不同，且没有真正限定known-09对象。

这是新增测试断言缺陷；当前没有证据要求修改产品expected、exporter或rawactual。
解析JSON中的`\n`与`\u000a`为同一LF不是换行normalize；解析后的CRLF仍是两个码元，不能等同LF。

## 2. 修复范围：仅2个文件

### `acceptance-fixture/windows/NativeTextSelfTest.cs`

- 保留原失败测试名`native-text-legacy63-has-canonical-known09`，改为调用测试专用`LegacyKnown09Matches`。
- 使用Framework已有`System.Web.Script.Serialization.JavaScriptSerializer.DeserializeObject`，
  不手写JSON解析器、不用生产exporter生成断言expected。
- 通过解析后的对象树严格定位`suites → known-input → cases`；要求ID为`known-09`的case恰好一个。
- 对该对象的`id/task_payload/expected_text`要求string类型、Ordinal精确比较：
  - id：`known-09`
  - task_payload：独立字面量`第一行\r\n第二行\r\n第三行`
  - expected_text：独立字面量`第一行\n第二行\n第三行`
- malformed JSON/缺失结构/字段类型不符返回false。此helper只检查目标case；整份目录仍由原63parity验证，
  不冒称通用JSON/schema校验器。
- 附加正例覆盖手写的JSON短转义和Unicode转义，expected不依赖CaseContract/NativeTextPolicy/JsonString生成。
- 附加负例覆盖：expected改为native CRLF、混合/裸CR/多余CR/LF、payload变为LF/混合/多余换行，
  错ID、错suite、缺失case、两个相同ID、一个正确加一个错误的重复ID、只有别的case内容正确但known-09错误，
  缺expected、非字符串payload/expected、坏JSON。
- 新检查全部追加在原检查之后，原213条注册顺序及动态`export-strict-*`名称不因插入而漂移。
  原负例保留；没有删除或放松known-09语义检查。

### `acceptance-fixture/build-windows.ps1`

仅给原csc引用列表增加`System.Web.Extensions.dll`，供Framework parser使用；无NuGet、无额外构建分支。
普通默认helper仍包含NativeText*.cs，默认policy接线不变。
本仓库现有Windows test runner也使用同一Framework parser；未修改runner。

没有修改MainForm、NativeTextPolicy、Cases/CaseContract/两个exporter、旧63/parity、focus/B2/gesture，
也没有改任何Python分析器/测试、Mac、Rust/root或旧输入actual。

## 3. Compile-only

独占临时目录：`/tmp/windows-native-text-repair-sol-20260930-k9gmvjse/`

使用本机现有csc（Mono包装，非远端Windows Framework csc），C#5、默认无define路径、原source组加NativeText*，
加`System.Web.Extensions.dll`引用：**compile-only exit 0，编译日志空**。
生成exe没有执行，未执行PowerShell helper；没有重跑任何测试/导出/parity/分析器。
本轮未修改Python，未重复Python编译或执行。

最终附件目录：
`.agents/reports/windows-native-text-policy-sol-repair-20260930-artifacts-225208/`

- `compile-commands.txt`、`compile.log`、`compile-exit.txt`：精确编译命令/日志/数值退出码。
- `NativeTextSelfTest.cs.repair.diff`、`build-windows.ps1.repair.diff`：仅本轮修改差异。
- `repair-source-sha256.txt`：2份修订文件。
- `frozen-source-sha256.txt`：新的完整32份输入manifest；不覆盖上一轮32份manifest。
- `before-sha256.json`、`protected-fingerprints.json`：除两份授权修订外33份源文件/原RED stdout与summary/CC报告指纹相同。
- `frozen-at.txt`：源码冻结时间；`compile-location.txt`：临时编译目录。

源码指纹：

```text
33fd2d3c60d15ac087f9d9024f0b63fd635d1735d1c4f921fb21ade86fc7724f  acceptance-fixture/windows/NativeTextSelfTest.cs
851974f9a7a9ec91d9992ef6358562948914f4aabeccd7a989d03fbc40af6e3b  acceptance-fixture/build-windows.ps1
```

## 4. CC新一轮独立复验

1. 新建Windows冻结副本，核对本轮`frozen-source-sha256.txt`。不得覆盖首轮212/1的源快照、exe、ntout或报告。
2. **普通helper构建**，无需define：

```powershell
# 在新snapshot根目录；$out必须是新的独占目录。
$out = Join-Path $env:TEMP ('computer-native-text-repair-cc-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $out | Out-Null
powershell -NoProfile -ExecutionPolicy Bypass -File .\acceptance-fixture\build-windows.ps1 -OutDir (Join-Path $out 'buildout') 1> (Join-Path $out 'build.log') 2> (Join-Path $out 'build.err')
$code = $LASTEXITCODE
$code | Set-Content (Join-Path $out 'build-exit.txt')
if ($null -eq $code -or $code -ne 0) { throw "build exit=$code; STOP" }
$exe = Join-Path $out 'buildout\ComputerUseAcceptance.exe'
```

3. 沿用CC已验证的bounded native runner（`scripts/windows-test-runner.ps1` + `windows-test-runner/Runner.cs`），
   每步独立新config/evidence，立即检查真实native_exit_code及timed_out；first failure停并保留原始输出。
   封装函数和完整参数可沿用`.agents/reports/windows-native-text-policy-sol-20260930.md`第5节，按顺序执行：

| 程序 | 参数 |
|---|---|
| 新`$exe` | `--self-test` |
| 新`$exe` | `--export-native-text-policy <新policy.json>` |
| 新`$exe` | `--export-cases <新legacy63.json>` |
| 新`$exe` | `--export-platform-cases <新platform20.json>` |
| 新`$exe` | `--export-focus-cases <新focus10.json>` |
| 实际python.exe | `tests/windows_native_text_analyzer.py` |
| 实际python.exe | `tests/focus_gui_analyzer.py` |
| 实际python.exe | `tests/basic_input_gui_analyzer.py` |
| 实际python.exe | `acceptance-fixture/tools/check-cases-parity.py <新legacy63.json>` |

保留原213检查名称/原失败条目在新stdout中的实际结果，新增检查的实际总数由CC报告，不能拿compile0当green。
native manifest仍应为明确Windows CRLF policy，旧63 known-09 expected仍是canonical LF，raw payload不变。
只读依赖/旧parity所需两份Swift目录源按前报告带齐；不启动Mac程序或GUI。

## 5. 冻结结论

**REPAIR SOURCE_FROZEN。** 无已知剩余源码实现阻断；下一步是CC默认Windows csc、自测及完整后续复验。
本轮尚无任何新测试通过证据，不把CLI terminal0、compile0或源代码审阅当验收通过。
原212 PASS / 1 FAIL / native1保留；旧GUI known-input9/10失败也不因本修复改变。无GUI授权或新增范围。
