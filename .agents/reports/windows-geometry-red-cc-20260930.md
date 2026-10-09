# Windows geometry StageA — RED CC verification — 2026-09-30

- 执行身份：CC（真实 Windows acer-win，Session 0 纯 console；`--self-test` 在任何 WinForms 初始化前返回，未创建窗口/输入/截图；无 Mac/Linux 执行）。实际模型 glm-5.3-flash。
- 结论：**预期 RED 精确确认。** 默认 helper 构建 exit 0；`--self-test` native exit **1**、无超时；**既有 237 PASS 逐行保留（missing=0）**，仅 2 条命名新增 geometry 需求 FAIL，总计 `SELF-TEST FAILED (2/239 checks failed)`。两条 FAIL 均为真实 parser `ArgumentException` 消息，非 hardcoded false/无关异常。零源码修改、无重试、后续 export/GUI 全部未执行。

## 1. 冻结源码与哈希

- Manifest：`.agents/reports/windows-geometry-red-sol-20260930-artifacts-230944/frozen-source-sha256.txt` 全部 **35 文件**（含 runner ps1/Runner.cs）tar-over-ssh 到**新唯一 TEMP**（保留相对路径）。本地 SHA 清单=`3c71b589…`。
- 远端 staging（保留）：`C:\Users\Administrator\AppData\Local\Temp\computer-geometry-red-cc-4f9151277b6d4fe4817d11814c59c42c`
- 远端 `Get-FileHash`：**35 OK / 0 MISMATCH**（`remote-hash-verify.txt`）；RUNNER `ad2a607c…`/`aced18e6…` 与冻结表一致。
- 静态审查：`GeometrySelfTest.cs` 为真实需求断言（调用现有 `BasicArguments.Parse`，捕获真异常、消息入详情；`geometry-stage-a-never-written.json` 仅为字符串，从未打开）；`MainForm.cs:545` 在 `NativeTextSelfTest.Run()` 后追加注册，原 237 项接线未动；helper 仅新增 `Geometry*.cs` 源列表，无 define、无产品接线。

## 2. 构建与执行

| 项 | 值 |
|---|---|
| 构建 | 普通 `build-windows.ps1`（Framework64 csc v4.0.30319，无 define），**exit 0**，build stderr 空 |
| exe | `geoout\buildout\ComputerUseAcceptance.exe` SHA-256 `9dccd472a37126f586c2196169a5b76bc609e8cacb5c59b9394d1ac4fe320749`（归档重算一致） |
| runner | 冻结 bounded JSON runner，`--self-test` 仅此一步，timeoutSeconds=120，evidence 父目录 `geoout` 预建、leaf 由 runner 原子保留 |
| native exit | **1**（`outcome=native_failure`、`timed_out=false`、`cleanup_state=root_exited`、stdout 16898B drain 完整、stderr 0B） |

## 3. 实际计数（真实 stdout，非猜测）

| 组 | 计数 |
|---|---|
| PASS | 237（前轮 237 基线逐行比对 **missing=0**） |
| FAIL | 2（均为本轮新增命名需求） |
| 总计 | **239**，末行 `SELF-TEST FAILED (2/239 checks failed)` |

两条 FAIL 原样（stdout）：

```text
FAIL: geometry-args-accepts-geometry-suite ? required --suite geometry; parser rejected with ArgumentException: unknown suite: geometry
FAIL: geometry-args-accepts-independent-geometry-export ? required independent --export-geometry-cases PATH; parser rejected with ArgumentException: unknown argument: --export-geometry-cases
```

### 缺失根因（静态代码核对，与运行输出一致）

- `BasicArguments.Parse` suite 白名单无 `geometry` → `--suite geometry` 抛 `ArgumentException: unknown suite: geometry`，`observedSuite` 未赋值，断言 1 失败。
- 参数白名单无 `--export-geometry-cases` → 抛 `ArgumentException: unknown argument: --export-geometry-cases`，`exportParsed` 为 null，断言 2 失败（其断言同时要求 SelfTest/ExportPath/PlatformExportPath/FocusExportPath 均未被借用）。
- 与 SOL 预测完全一致；无其他失败、无编译失败、无超时。

## 4. Wrapper 记录

本轮 wrapper（master/collect/hashverify/hashrecheck/residual 全 on-disk）一次成功；真实状态取自 `summary.json`/exit 文件，未用 cmd `%ERRORLEVEL%` 作权威。无复用旧 exe/snapshot。

## 5. 测试后完整性与残留

- 测试后 `Get-FileHash`：**35 OK / 0 MISMATCH**（`remote-hash-recheck.txt`）。
- 残留核对（read-only，无 kill）：按本轮精确 owned 路径 `…\computer-geometry-red-cc-4f9151277b6d4fe4817d11814c59c42c\geoout\buildout\ComputerUseAcceptance.exe` 枚举 `Win32_Process`：**无残留**（`residual-check.txt`）。Notepad24332 未触碰。

## 6. 归档

`.agents/runs/windows-geometry-red-cc-20260930/`：`remote-staging.tgz` + 解包全量（35 冻结源码副本、hash-verify/recheck、master/collect/residual 脚本、build 日志/exit、exe（SHA 可复核）、`selftest-config.json`/`selftest-evidence`（config/identity/summary/stdout/stderr）、runner stdout/stderr、`collect.txt`、`residual-check.txt`）。远端 TEMP 保留；旧 native-text R2/focus/known-input snapshot 与报告未触碰。

## 7. 边界（不隐瞒）

1. 本轮仅证明 **parser 需求缺口**（geometry suite 与独立 export 参数缺失）；geometry catalog/export 实现/GUI/判定/analyzer/视觉任务全部未实施（StageB）。
2. CLI/构建成功不代表功能成功；RED 即本阶段交付物，未做任何使测试通过的改动。
3. `native exit 1` 是预期 RED 的真实原生退出码，不是 wrapper 错误（harness 与 native 分开记录）。
4. geometry 10 例、真实 PNG 字节校验、人工环境/DPI/topology、安全停止等门禁属 StageB，未被纯测试替代或声称支持。
5. Root Rust/EOF、Mac/Linux、Host/renderer 未触碰；未 commit/push/worktree。

**STOP：StageA RED 已确认并归档。源码实现 author 等待本报告后启动；本轮未做任何源码改动。**
