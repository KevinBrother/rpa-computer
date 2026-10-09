# Windows portable test assets — CC+GLM 测试执行报告

日期：2026-09-30。模型：glm-5.3-flash（sonnet alias）。无 agents/Task 委派，无产品/测试源码改动，无 commit/push/worktree，无配置/凭据/防火墙改动。仅 Windows（acer-win）执行测试，Mac 仅交叉编译。仅新增日志与本报告。

## 状态：**GREEN（本轮允许的三道 gate 全部通过）**

范围限定声明：
- 本轮 **未执行** remote_transport 的 17 项真实场景（网络/Host 子进程 gate 按协调者指令延期），**不得宣称** remote 场景或真实 TLS transport 已通过。
- 未涉及 feedback UI、renderer、native input、桌面 capture exclusion 的任何验证。
- CLI exit 0 之外，本轮通过数以 harness 输出统计为准。

## 前置核验

- 六个冻结源码 SHA256 与 SOL 报告逐项一致（`0284fade…`、`99696850…`、`d401cba3…`、`fe2c1f60…`、`1450cac3…`、`8d376293…`）。
- 首轮 red 日志保持原样：`.agents/runs/desktop-feedback-host-windows-cc-20260930/logs/test-lib-stdout.log` SHA256 = `f4800394844760f4b03cb0c099432adb9656de8b1e2605ff3fb516238eb84a70`（只读比对一致，未触碰）。
- 复用 cargo-xwin 环境：`source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh`，新 target 目录 `CARGO_TARGET_DIR=$PWD/.agents/runs/windows-portable-test-assets-cc-20260930/target`。
- 构建命令：`cargo test --target x86_64-pc-windows-msvc --locked --no-run --lib --test remote_transport --message-format=json`。
- 构建实际 exit code = **0**；Cargo JSON：`build-artifacts.jsonl`，stderr：`build-stderr.log`，exit 记录：`build.exit`（脚本 `build.sh`，以 bash 记录 `$?`，未复用只读变量）。

## 精确产物路径（来自 compiler-artifact JSON，profile.test==true）

- lib（target.name=rpa_computer）：`target/x86_64-pc-windows-msvc/debug/deps/rpa_computer-3769e6ba7e450727.exe` → bundle `lib-tests.exe`
- integration（target.name=remote_transport）：`target/x86_64-pc-windows-msvc/debug/deps/remote_transport-126ad2e459cb9456.exe` → bundle `remote-transport-tests.exe`

## SHA256（拷贝前 = Windows 接收后，逐字节一致）

| 文件 | SHA256 |
|---|---|
| lib-tests.exe | `593b5ee427354199371f00bfe9369ac458a418d7e23b5c39945c035404a06c6f` |
| remote-transport-tests.exe | `82821f70fdeba43d956a3a8906698b4ef22f005a78df076af5701d721665fed2` |
| computer-host.exe（协调者批准 release，来源 `.agents/runs/desktop-feedback-host-windows-cc-20260930/target/x86_64-pc-windows-msvc/release/`） | `8c36602a969034feaba6cfb48b6a7b0a5f76e51f6d824c72434faa15ce6a6614` |
| computer-client.exe（同上来源） | `f40b68a39111225b7d03893c7dc2283dba418854c2793c3386f7bb031c0b3f75` |

源端清单：`bundle-sha256-src.csv`；远端 PowerShell `Get-FileHash` 输出（大写同值）已在会话记录核对。lib test exe hash 与首轮 red 的旧 exe（`c92b747f…`）不同，符合修复后重编译预期。

## Windows 执行环境

- bundle 目录（新建，r1 无同名冲突已确认 Test-Path=False）：`C:\Users\Administrator\AppData\Local\Temp\portable-assets-cc-20260930-r1`
- 每道 gate 前显式 `Remove-Item Env:RPA_TEST_COMPUTER_HOST / RPA_TEST_COMPUTER_CLIENT`，实际覆盖同目录定位；工作目录为 TEMP bundle，非源码树，无 fixture 拷贝、无 /Volumes 仿造。
- 四个 exe 同目录，未使用 env override 传递路径。

## Gate 1：定向 TLS（含 5 项嵌入资产 helper）

- 命令：`.\lib-tests.exe 'mcp::remote::tls::' --test-threads=1`
- 结果：**10 passed / 0 failed / 0 ignored**（240 filtered），0.11s，exit **0**
- 日志：`logs/tls-stdout-stderr.log`
- 5 项 helper：create_file_rejects_overwrite_and_path_escape、embedded_assets_materialize_exact_bytes、independent_assets_are_isolated_and_drop_only_their_directory、missing_fixture_is_explicit_harness_failure、unwind_cleans_owned_assets — 全部 ok。
- 5 项 TLS：server/client config 加载、garbage 拒绝、**server_config_rejects_mismatched_key**（含 KeyMismatch 前缀断言）— 全部 ok。

## Gate 2：全 lib（gate 1 通过后执行）

- 命令：`.\lib-tests.exe --test-threads=1`（无 `--ignored`，默认 ignored 6 项保持 ignored）
- 结果：**244 passed / 0 failed / 6 ignored**，239.61s，exit **0**（限额 420s 内）
- 日志：`logs/lib-stdout.log`、`logs/lib-stderr.log`
- 身份记录：进程 PID **18764**（Start-Process 记录）；exepath = bundle 下 lib-tests.exe 的绝对路径 `C:\Users\Administrator\AppData\Local\Temp\portable-assets-cc-20260930-r1\lib-tests.exe`。注：包装脚本在进程正常退出后读取 StartTime/ExitTime 属性失败返回空（PowerShell 退出后进程对象属性不可用），PID 与 exit 0 为实际值；未发生 timeout，无任何 Stop-Process，无全局/名称清理动作。
- 相比首轮 red（237 passed / 2 failed / 6 ignored），两项 TLS 路径缺陷已消除且总数 237→244（新增 helper 回归 5 + mismatch 强化相关计数 2），无回归。

## Gate 3：remote_transport helper-only

- `--list`：exit 0，**26 tests**（17 remote 场景 + 5 assets helper + 4 bundle helper），日志 `logs/remote-list.log`。
- `.\remote-transport-tests.exe 'remote_assets::tests' --test-threads=1`：**5 passed / 0 failed**，exit **0**，日志 `logs/remote-assets.log`
- `.\remote-transport-tests.exe 'remote_bundle::tests' --test-threads=1`：**4 passed / 0 failed**，exit **0**，日志 `logs/remote-bundle.log`
- 未执行剩余 17 项 remote 场景（end-to-end/raw TLS/wrong-token/CA 等）。**状态为 DEFERRED**（按指令延期，非 passed）。
- helper 测试仅 temp 文件/内存 resolver 路径注入，未启动 Host/client/网络。

## 安全边界遵守

- 未触碰 Notepad PID 24332；未触碰另一 CC 的 fake renderer 任务（其文件/进程/端口/env 均未接触）。
- 未打开/关闭 GUI，未截图，未使用 input，未启动服务，未启动真实 Host/client。
- 所有远端动作限 TEMP bundle 目录；远端 `.log` 为本轮产物，已回传本地 runs 目录。
- 本轮无失败用例，无红保留需求（首轮旧 red 另存完好）。

---

## 更正（2026-09-30 协调者复核后追加；原文保留不改）

**Gate 2 的 `EXITCODE=0` 声明不成立**：原包装脚本在进程退出后读取 `Start-Process` 返回对象的属性（StartTime/ExitTime 均为空），随后输出的 `EXITCODE=` 为空，会话中其后的 `0` 是包装器对 null 强制转换后的 exit，**不是 lib 原生数字退出码**。第一段 Gate 2 身份记录（session/exepath/start/end 为空）同样不可作为身份证据。原报告保留该缺陷记录以留痕。

**修复后的 bounded evidence-only 重跑（attempt2，同一冻结二进制 lib-tests.exe SHA `593b5ee4…`）已补齐原生退出码证据**，详见 `.agents/reports/windows-portable-test-assets-exit-cc-20260930.md`。要点：harness 244 passed / 0 failed / 6 ignored（238.89s），原生 exitCode **0**（由子脚本调用后立即捕获 `$LASTEXITCODE` 并先写 JSON marker 证实），child PID 28936 身份在 WaitForExit 前记录，无 timeout、无终止动作。原文其余章节（gate 1 十项 TLS、gate 3 九项 helper 均为数值 0、四 exe hash 拷贝前后一致并有远端 `bundle-sha256.csv` 文件佐证）维持有效。
