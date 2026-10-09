# Windows renderer strictness — 阶段 A

状态：**TESTS_READY_FOR_CC_RED**。终态暂停：等待协调者提供 CC 实际 red 证据和明确 GO；不进入阶段 B。

## 范围与交付

- 仅修改 `desktop-feedback/windows/SelfTests.cs` 和本报告。
- `JsonValue.cs`、`Protocol.cs` 保持不变；未改 Host、build 脚本、Mac 或 Linux。
- 所有旧断言保留；新增 `StrictProtocolCases(basic)` 放在旧测试之后、成功摘要之前。
- 新增命名负例 helper `RejectNamed`：仅 `WireFailure` 算预期拒绝，意外异常不吞；非法帧被接受时错误包含具体 case 名。
- 未执行测试或生成的 exe，未执行 GUI、SSH、截图、Host；未启动其他 agent，未 worktree/commit/push/reset。

## 新增 case（80 个参数化断言；此数为源码统计，不是执行结果）

以下名称为失败码 `self_test_` 后的 case 名；花括号表示笛卡尔展开。

1. `strict_{session_id,surface_id,surface_version}_reject_{empty,129_bytes,space,slash,unicode,escaped_newline,escaped_unicode}`（21）。
2. `strict_{session_id,surface_id}_reject_comma`（2）；版本逗号不得误拒绝。
3. `strict_{session_id,surface_id,surface_version}_accept_{1_byte,alphabet,128_bytes}`（9）；断言解析后的完整值相等。
4. `strict_pointer_requires_surface`（1）。
5. `strict_null_surface_null_pointer`、`strict_surface_without_pointer`（2）。
6. `strict_geometry_version_signed_origin_commas`（1）：`d1:o-1920,-200:i1920x1080:c1920x1080:r0`，保留原真实 Geometry.version 正例。
7. `strict_{sequence,coordinate}_json_{leading_zero,negative_leading_zero,double_zero_fraction,leading_zero_exponent,plus,missing_fraction,missing_integer,missing_exponent,missing_signed_exponent}`（18）：`01/-01/00.1/01e2/+1/1./.1/1e/1e+`。
8. `strict_{sequence,generation}_reject_{overflow,fraction,decimal_integer,exponent_integer,negative,negative_zero}`（12）：`18446744073709551616/1.5/1.0/1e0/-1/-0`。
9. `strict_{sequence,generation}_exact_{0,9007199254740993,18446744073709551615}`（6）：覆盖零、超过 double 精确整数范围、u64 MAX。
10. `strict_coordinate_valid_json_{0,1,2,3,4,5}`（6）：分别 `0/-0/-1800.5/-1.8e3/-1.8E+3/-18000e-1`，断言数值。
11. `strict_pointer_fraction_exponent`（1）：pointer 的 `-1.8005e3/-100.25` 精确解析。
12. `strict_pointer_nonfinite`（1）：pointer 坐标 `1e999` 拒绝。

## 静态观察（非 red 证据）

Windows `JsonValue.Parser` 现有零分支只消费一个 `0`，已有前导零拒绝结构；新增相关 case 是回归防线，不预先声称这些 case 会 red。`Protocol.Nonempty` 仅检查非空，pointer 分支没有同帧 surface 非空约束。按源码，新增测试的首个预期失败是 `self_test_strict_session_id_reject_129_bytes`。纯 self-test 为 fail-fast；首个失败后的 case 尚未执行，不能据首个 red 宣称所有负例已验证。

## 编译结果

仅执行本地 C#5/.NET4 **语法编译**（mcs），成功退出 0；没有运行生成 exe。这不是 Windows Framework64 构建或 CC 测试证据。

```sh
mcs -langversion:5 -sdk:4 -warnaserror+ -target:winexe -platform:x64 \
  -r:System.dll -r:System.Core.dll -r:System.Drawing.dll -r:System.Windows.Forms.dll \
  -out:/tmp/renderer-strictness-sol-CcBQki/desktop-feedback-windows-syntax-only.exe \
  desktop-feedback/windows/{JsonValue,Protocol,Model,IPC,Native,Windows,SelfTests,Program}.cs
```

编译产物仅在 `/tmp/renderer-strictness-sol-CcBQki/`，未覆盖共享项目 build 产物。

## CC 精确待执行命令（Windows PowerShell，在 rpa-computer 根目录）

下面命令仅交接给 CC+GLM；本实现者未执行。构建后必须等待 WinExe 退出并检查退出码，保存 stderr 作为 red 证据；不启动普通 GUI 模式。

```powershell
$ErrorActionPreference = 'Stop'
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\desktop-feedback\build-windows.ps1
if ($LASTEXITCODE -ne 0) { throw "renderer build failed: $LASTEXITCODE" }
$stdout = Join-Path $env:TEMP 'desktop-feedback-renderer-strictness-sol-20260930.stdout.log'
$stderr = Join-Path $env:TEMP 'desktop-feedback-renderer-strictness-sol-20260930.stderr.log'
$p = Start-Process -FilePath (Resolve-Path .\desktop-feedback\build\desktop-feedback-windows.exe).Path -ArgumentList '--self-test' -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
Get-Content -LiteralPath $stderr
Write-Output "self-test exit=$($p.ExitCode); stdout=$stdout; stderr=$stderr"
exit $p.ExitCode
```

协调者须确认失败来自新增预期断言而非构建/环境异常，再提供 red 证据及 GO。未经 GO 不修实现、不自行跳过 red、不报告 green 或 GUI 通过。

---

# 阶段 B — 2026-09-30

当前终态：**READY_FOR_CC_RETEST**。上文阶段 A 状态及证据作为历史保留；本阶段已收到协调者明确 GO。

## red 门禁证据

实现前直接读取如下 CC raw 文件：

- `.agents/runs/renderer-strictness-windows-cc-red-20260930/self-test-windows.output`：`exitcode=1`，stderr 为 `self_test_strict_session_id_reject_129_bytes`。
- `.agents/runs/renderer-strictness-windows-cc-red-20260930/build-windows.stdout`：`build_exit=0`。
- 对应 CC Windows 目录：`C:\Users\Administrator\AppData\Local\Temp\renderer-strictness-cc-red-20260930-190311-265262435`。

该证据确认构建成功、首个新增 129 字节 session ID 负例失败；fail-fast 后续新增 case **不称已验**。未等待 final report，也未自行重跑 red。

## 最小实现

本阶段仅修改 `desktop-feedback/windows/Protocol.cs` 与本报告：

- 用 `Token(JsonValue, bool allowComma)` 替代仅检查非空的 `Nonempty`。
- session.id、surface.id：长度 1..128，只允许 ASCII `[A-Za-z0-9_.:-]`。
- surface.version：长度 1..128，单独启用逗号 `[A-Za-z0-9_.:,-]`，不放宽 ID。所有接受字符均为 ASCII，因此接受值的字符长度即 UTF-8 字节长度。
- 非 null pointer 在解析字段前要求本帧有效 surface 非 null；仍保留既有 surface 尺寸/finite 校验以及 pointer finite/kind 校验。
- 对照 canonical Rust `Snapshot.validate`，没有新增 pointer 必须落在 surface 矩形内的限制。
- `JsonValue.cs` **未改**：零分支只消费一个 `0`，额外数字不能成为合法分隔符/结束；小数与指数均要求数字；`UInt()` 保持逐字符十进制数字校验及精确 UInt64 范围。没有无故重写 JSON 词法。
- `SelfTests.cs` 本阶段 **未改**，所有旧断言及阶段 A 新增 80 个参数化断言均保留，未弱化。
- 没有修改 Host、构建脚本、Mac 或 Linux，没有新增依赖或输入注入行为。

## 编译结果与验证边界

仅本地 mcs C#5/.NET4 语法编译，成功退出 0；生成文件未运行，不代表 Windows Framework64 构建或 self-test green。

```sh
mcs -langversion:5 -sdk:4 -warnaserror+ -target:winexe -platform:x64 \
  -r:System.dll -r:System.Core.dll -r:System.Drawing.dll -r:System.Windows.Forms.dll \
  -out:/tmp/renderer-strictness-sol-B-HkddhE/desktop-feedback-windows-syntax-only.exe \
  desktop-feedback/windows/{JsonValue,Protocol,Model,IPC,Native,Windows,SelfTests,Program}.cs
```

产物位于 `/tmp/renderer-strictness-sol-B-HkddhE/`，不覆盖共享项目 build 产物。没有执行测试、GUI、SSH、截图或 Host，没有启动其他 agent，没有 worktree/commit/push/reset。

## CC 精确复测命令

CC 应将本次最新 Windows 源码交接到 Windows，使用下列命令重新构建，不能复用阶段 A 的旧 exe。命令在 Windows `rpa-computer` 根目录执行；仅 `--self-test`，不运行普通 GUI。

```powershell
$ErrorActionPreference = 'Stop'
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\desktop-feedback\build-windows.ps1
if ($LASTEXITCODE -ne 0) { throw "renderer build failed: $LASTEXITCODE" }
$stdout = Join-Path $env:TEMP 'desktop-feedback-renderer-strictness-sol-B-20260930.stdout.log'
$stderr = Join-Path $env:TEMP 'desktop-feedback-renderer-strictness-sol-B-20260930.stderr.log'
$p = Start-Process -FilePath (Resolve-Path .\desktop-feedback\build\desktop-feedback-windows.exe).Path -ArgumentList '--self-test' -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
Get-Content -LiteralPath $stderr
Write-Output "self-test exit=$($p.ExitCode); stdout=$stdout; stderr=$stderr"
exit $p.ExitCode
```

实际 green/全部 case 通过须由 CC+GLM 的最新构建及测试 raw 证据确认；本实现者仅交付待复测，不报告 green 或 GUI 通过。
