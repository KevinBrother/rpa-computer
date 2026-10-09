# Windows native-text default build — CC independent verification — 2026-09-30

- 执行身份：CC（真实 Windows 机器 acer-win，经既有 SSH；**Session 0 纯 console**；无 GUI/Host/网络监听/capture/原生输入；未运行 Mac/Linux 程序，Swift 仅只读 parity 数据；root Rust 未触碰）。实际模型：glm-5.3-flash。沿用 focus 轮已验证 staging/runner 模式。
- 结论：**FIRST RED 于步骤1（--self-test），序列按约停止。** 默认（无 define）helper 构建本身成功（exit 0），但自测 213 项中 **1 项失败**：`native-text-legacy63-has-canonical-known09`（native exit 1）。既有 178 断言逐行比对全部保持 PASS（missing=0），新增 35 项中 34 项 PASS、1 项 FAIL。**未修改任何源码/断言/脚本**；未重跑求 green；后续步骤 2–4 全部未执行。交回实现 owner。

## 0. 静态审查（运行前，均通过，无阻断）

- 默认接线确认：frozen `build-windows.ps1` 含 `NativeText*.cs` 源列表、无任何 define；`MainForm.cs` 无 `#if/#else/#endif`、无 canonical 运行 fallback；`TextExpected()` 无条件调用 `NativeTextPolicy`；`NativeTextSelfTest.Run()` 在自测序列注册。
- 策略窄域确认：仅 `SuiteKind.KnownInput` + ID `known-09`；payload/canonical 漂移抛 `InvalidOperationException`（fail-closed）；比较复用原封 `CaseContract.Utf16ExactMatch`；无全局 CRLF/LF 容忍；extra CR/LF/裸 CR/尾部空白由 deviation 负例覆盖（本轮全 PASS）。
- 独立导出确认：`--export-native-text-policy` 在 WinForms 初始化前返回，`FileMode.CreateNew` 拒绝覆盖；manifest 含 `policy_version=windows-winforms-known09-crlf-v1`、canonical/native 各自 UTF-16 hex、`actual_normalized=false`、`legacy63_changed=false`、`gui_verified=false`。

## 1. 冻结源码与哈希

- Manifest：`.agents/reports/windows-native-text-policy-sol-20260930-default-artifacts-224052/frozen-source-sha256.txt` 全部 **32 文件**（6 写集 + 全部编译/分析/回归依赖 + parity 工具 + 两 Swift 只读数据）+ coordinator 冻结 runner 2 文件，共 **34 文件**经 tar-over-ssh 复制到**新唯一 TEMP**（保留相对路径）。
- 远端 staging（保留）：`C:\Users\Administrator\AppData\Local\Temp\computer-native-text-cc-9eee4b27789e4a70b8ba8a167c5ecc72`
- 本地 shasum 预核对 32/32 一致；远端 `Get-FileHash`（`remote-hash-verify.txt`）**32 OK / 0 MISMATCH**；RUNNER `ad2a607c…`/`aced18e6…` 与冻结表一致。
- 未复用任何旧 focus/B2/opt-in 原型 exe。

## 2. 构建（普通 helper，无特殊 define）

- frozen `acceptance-fixture/build-windows.ps1 -OutDir <staging>\ntout\buildout`（Windows in-box Framework64 csc v4.0.30319）：**build exit = 0**（`build-exit.txt`），build stderr 空。
- 新 exe SHA-256：`6e7f06718fba34b78f213ccf6c9630a9cff16ffa6ee9d508d26636952d410db8`（本地归档重算一致）。**实测的即为默认构建**——无 `WINDOWS_NATIVE_TEXT_POLICY_V1` 或其他 define。

## 3. 执行与 FIRST RED（bounded approved JSON runner，evidence 父目录预建、leaf 原子保留，每步 120s）

| 步骤 | 状态 |
|---|---|
| 1 `--self-test` | **native exit = 1**（harness exit 1，`outcome=native_failure`，`timed_out=false`，`cleanup_state=root_exited`；stdout 14115 bytes drain 完整，stderr 0） |
| 2 `--export-native-text-policy` / `--export-cases` / `--export-platform-cases` / `--export-focus-cases` | **未执行**（按 first-failure-STOP） |
| 3 `tests/windows_native_text_analyzer.py`、focus/B2 回归 | **未执行** |
| 4 legacy63 parity + canonical/native manifest 独立解读 | **未执行** |

### 自测实际计数与逐行比对（非猜测）

- 总计 **213 checks：212 PASS / 1 FAIL**，末行 `SELF-TEST FAILED (1/213 checks failed)`。
- 原 **178 断言逐行比对全部存在且 PASS（missing=0）**（与上一轮 focus green 178 行归档逐行 diff）。
- 新增 native-text 35 项：**34 PASS / 1 FAIL**。FAIL 行（原样，stdout 行205）：

```text
FAIL: native-text-legacy63-has-canonical-known09 ? pure Windows expected policy; not GUI evidence
```

### 失败断言精确定位（供 owner；CC 未改源码）

`NativeTextSelfTest.cs:42-43`：

```csharp
string oldManifest=GestureExporter.ManifestJson();
add("legacy63-has-canonical-known09",oldManifest.Contains(GestureExporter.JsonString(canonical)));
```

即：**未改动的 `GestureExporter.ManifestJson()`（legacy63 导出路径）应包含 JSON 转义后的 canonical LF known-09 expected**。该断言在真实 Windows Framework csc 产物上失败。SOL 轮仅 Mono compile-only（从未运行），属首次真实运行发现。同组相邻断言 `legacy63-no-native-policy-overlay` PASS；`catalog-payload-unchanged`/`catalog-canonical-unchanged`/canonical LF 接受等均 PASS，失败疑点集中在 manifest 序列化内容/转义与 `Contains` 的匹配，**根因判定与修复归实现 owner，CC 不猜改**。

## 4. Wrapper 尝试失败记录（保留，与产品 RED 分开）

1. 内联远端文件清单命令再次被 SSH/cmd 层吞掉（0 字节）→ 改 on-disk 脚本（与 focus 轮相同模式）。
2. `tr` macOS 默认编码 Illegal byte sequence → `LC_ALL=C` 重做逐行比对。
3. 残留检查脚本第一次误用 focus 模板路径（`greenout\buildout`，路径不存在 → 假阴性）→ **发现后用正确 `ntout\buildout` 路径重做**（正确结果见 §6）；第一次结果一并保留为 wrapper 错误证据。
4. `master.ps1` 抛出 STOP 后 cmd `%ERRORLEVEL%` 显示 0（cmd `&` 链不传播 PowerShell 退出码）；真实停止证据为 throw 文本与 `selftest-harness-exit.txt`。
5. 无被测物重复运行；无断言放宽。

## 5. 哈希与测试后复查

- 测试后 `Get-FileHash` 复查（`remote-hash-recheck.txt`）：**32 OK / 0 MISMATCH**，源码未被测试过程改动。

## 6. 残留核对（read-only，无任何 kill）

按精确 owned 身份（本轮 `ntout\buildout\ComputerUseAcceptance.exe` 全路径）枚举 `Win32_Process`：**无残留**（`residual-check.txt`）。runner summary `root_exited`，无需 kill；Notepad24332 未触碰。

## 7. 归档

`.agents/runs/windows-native-text-policy-cc-20260930/`：`remote-staging.tgz`（RED 后立即快照）+ `remote-staging-final.tgz`（含 hash-recheck/residual，均解包保留）：34 源码副本、双份 hash-verify/recheck、`master.ps1`/`inspect1.ps1`/`residual*.ps1`、build 日志/exit、exe（SHA 可复核）、`selftest-config.json`/`selftest-evidence`（config/identity/summary/stdout/stderr）及各 wrapper 日志。远端 TEMP 全部保留，无删除/覆盖。

## 8. 边界与未验证项（不隐瞒）

1. **仅 known-09**；未扩展任意换行输入、geometry/focus 新场景。
2. 步骤 2–4（policy 导出、三个旧导出、native-text 分析器测试、focus/B2 回归、legacy63 parity、canonical LF vs native CRLF 独立解读）**因 first RED 未运行**，无任何 green 声明。
3. CLI exit 非 pass 判据：本轮红即以 stdout FAIL 行 + 精确源码定位为准。
4. 旧 Windows known-input GUI 9/10 历史失败**保持不变**，未追认；NEW GUI run（含 10/10 valid trials）仍 pending，未授权未执行。
5. `native_exact` 即使将来通过也非整套分数或 GUI 认证（`gui_verified=false` 固定）。

**STOP：FIRST RED 已保留并归档，序列未续跑。实现 owner 需先修复/裁决 `native-text-legacy63-has-canonical-known09` 后重新冻结，CC 再复验；本轮未做任何源码改动。**
