# Windows portable test assets — 实现交接与源码冻结

日期：2026-09-30。角色：Codex 实现；测试执行交 CC+GLM，仅 Windows。

## 状态与证据边界

**测试源码实现完成并冻结，Windows compile-check 已通过；尚未获得本次修复的 Windows 运行 green。**

本次没有执行测试、SSH、GUI、Host/renderer，没有启动 agent/CC，没有 commit/push/reset/worktree；没有修改或重写既有 `target/x86_64-pc-windows-msvc/release` 两个 bin，也未覆盖 CC 的固定 exe 或首轮 red 日志。

真实旧 red：`.agents/runs/desktop-feedback-host-windows-cc-20260930/logs/test-lib-stdout.log`，237 passed / 2 failed / 6 ignored。两项失败是 TLS 测试读取烘入的 Mac 构建路径，不是已证实的生产 TLS 故障。`remote_transport` 同类定位缺陷由源码确认，本轮旧 red 并未执行它，不能宣称已复现它的运行失败。

首轮 red 日志 SHA256（修改前后只读比对一致）：
`f4800394844760f4b03cb0c099432adb9656de8b1e2605ff3fb516238eb84a70`

## 精确写集

1. `src/mcp/remote/tls.rs`：仅 `#[cfg(test)]` 区；引入共用 helper，替换测试绝对 fixture 路径，加强 mismatch 测试。
2. `tests/remote_transport.rs`：TestGuard 自有资产及 bin 定位；显式传递上下文；坏 token 使用自有目录；fake TLS 线程捕获预先读取的 bytes；负向 Host 参数测试也附加 `--mock-backend`。
3. `tests/support/remote_assets.rs`：共用 test-only 嵌入式资产、唯一自有目录和 RAII。
4. `tests/support/remote_assets_tests.rs`：5 项资产 helper 回归源码。
5. `tests/support/remote_bundle.rs`：test-only binary resolver。
6. `tests/support/remote_bundle_tests.rs`：4 项 resolver 回归源码。
7. 本报告。

辅助编译/只读比对证据单独保存在忽略目录 `.agents/runs/windows-portable-test-assets-sol-20260930/`，不属于生产源码。

未修改生产 TLS、Cargo、Host、feedback、renderer、native-input、Windows-display crate、fixtures。生产 TLS 从文件开头至首个 `#[cfg(test)]` 的 bytes 与修改前完全相同，SHA256：
`b48f4b140985d5744a4e15637b01b2eeda6d3f7f9674fb6acba004fc4d63a8be`

`remote_transport.rs` 为 935 行；新增 helper/回归分四个文件，无新增千行文件。

## 实现细节

### 资产

- 编译期 `include_bytes!` 嵌入既有公开 throwaway fixtures：good 的 CA/server cert/server key/client token/host token，以及 wrong 的 CA/server cert/server key，共 8 项。不嵌入两套 CA signing key，不访问真实部署 credentials。
- library 只在 `#[cfg(test)]` 中引用该 helper；另一引用来自 integration test target。生产二进制不引入本 helper 的资产表。
- 每个 TLS 测试或 remote TestGuard 单独创建目录。目录名组合 PID、纳秒时间和进程内原子计数；以 `create_dir` 原子取得所有权，撞名只重试、不接管目录。文件使用 `create_new`，不覆盖已有文件。
- 目录创建成功即取得 RAII 所有权；中途写入失败或测试 panic 时也清理。Drop 仅删除该自有目录；清理失败输出明确诊断，避免 unwind 时二次 panic。进程被强杀/abort 时 RAII 不保证执行，不宣称此类退出无残留。
- fixture lookup 只接受嵌入表中的名字，读取并比较 bytes。缺失或内容改变立即报 `test harness failure`，不会流入“TLS 应拒绝”的断言假通过。
- 负向 token/garbage 文件走同一自有目录，名字禁止路径跳转、分隔符和 Windows ADS 冒号，避免固定 PID 临时文件覆盖。

### TLS mismatch 防假绿

先分别验证 good cert+good key、wrong cert+wrong key 均能构建配置，确认 cert bytes/key bytes 两两不同，再测试 good cert+wrong key。要求错误前缀 `invalid TLS cert/key:`，包含 rustls 的 `KeyMismatch` 且不包含 `cannot read`。错误标识依据当前锁定 rustls 源码，不依赖猜测的 OpenSSL 文案。

### Portable bundle 定位

优先级：
1. 测试环境变量 `RPA_TEST_COMPUTER_HOST` / `RPA_TEST_COMPUTER_CLIENT`；显式值为空、缺失文件或目录时立即 harness failure，不回退。
2. 当前 **测试 exe 所在目录** 下 `computer-host.exe` / `computer-client.exe`（Windows）。
3. 同机 Cargo 的 `CARGO_BIN_EXE_*` 路径，仅在其是目标平台 absolute path 且 regular file 时使用。Windows 上 `/Volumes/...` 不满足此条件。

返回 canonical absolute path，再交给 `Command`，不会通过系统 PATH 查找同名程序。无法定位时明确报错，不 skip。不增加任何生产 CLI/env。

remote 原有 timeout、owned Child kill/wait、ALPN/token/CA/name/oversize/concurrency 等断言和协议逻辑未弱化。真实 Host 子进程入口始终附加 `--mock-backend`，包括参数拒绝用例。

### 新回归源码

资产 5 项（在 lib 和 remote test binary 内各编译一份）：bytes 与嵌入内容一致、两个实例隔离且只清自己的目录、panic unwind 清理、缺失 fixture 明确 harness failure、拒绝 overwrite/path escape。

resolver 4 项（仅 remote test binary）：missing bin 明确 harness failure、override 优先且无效不回退、同目录优先于现存 Cargo fallback、拒绝 directory 候选。测试使用注入的路径，不改全局 env/cwd，不执行候选文件。

原 TLS mismatch 另加强断言。remote 保留原有 17 项场景，新增 helper 9 项，源码合计 26 项；这是源码计数，不是运行通过数。

## 已做的编译检查（非测试）

复用 CC 已修好的 cargo-xwin/rust-lld 环境，只覆盖输出目录到本任务独有目录：

```bash
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR="$PWD/.agents/runs/windows-portable-test-assets-sol-20260930/target"
cargo check --target x86_64-pc-windows-msvc --tests --locked
```

- 首次编译日志：`windows-check.log`，显示 Finished，14.04s；其后 shell 记录退出码时误用了 zsh 只读变量 `status`，包装命令 exit 1。未将这个包装结果冒充成功。
- 随后以 bash 重做相同 cargo check 并用独立 `check_rc` 记录，确认 **exit 0**，0.24s；日志 `windows-check-confirmed.log`，退出码 `windows-check.exit`。
- 以上日志均在本任务 runs 目录；无测试执行、未进行 root 测试 exe 的链接构建或 release 构建；不声明这些构建成功。原工具链问题未做任何生产源码修补。

## 交 CC：Windows portable bundle 与重跑

### 构建/打包（以下仅提供命令，本实现者未执行）

在构建机复用 CC 工具链、使用新的 CC run 目录，避免覆盖首轮 red/fixed exe：

```bash
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR="$PWD/.agents/runs/windows-portable-test-assets-cc-20260930/target"
mkdir -p .agents/runs/windows-portable-test-assets-cc-20260930
cargo test --target x86_64-pc-windows-msvc --locked --no-run \
  --lib --test remote_transport --message-format=json \
  > .agents/runs/windows-portable-test-assets-cc-20260930/build-artifacts.jsonl \
  2> .agents/runs/windows-portable-test-assets-cc-20260930/build-stderr.log
```

检查构建 exit code。由 `compiler-artifact` JSON 中 `profile.test == true` 的 `.executable` 精确选择 lib（`target.name == rpa_computer`）及 integration（`target.name == remote_transport`），不要按模糊 glob 误选旧 exe。复制到新的 Windows bundle，可分别命名 `lib-tests.exe` / `remote-transport-tests.exe`。

bundle 必须含：

| 文件 | 来源/用途 |
|---|---|
| `lib-tests.exe` | 本次冻结源码的 Windows lib test executable |
| `remote-transport-tests.exe` | 本次冻结源码的 Windows integration test executable |
| `computer-host.exe` | 协调者批准并已固定的 Windows release Host；由测试强制 mock |
| `computer-client.exe` | 与该 Host 配套的 Windows release client |
| `bundle-sha256.csv` | CC 对实际四个 exe 计算的 SHA256 清单 |

本次仅 test-only 改动，不要求修改或重建生产 release；优先复用协调者批准的固定两 bin。若生产 owner 已换版，必须先确认对应源码/产物身份并记录新的 hash。不要把构建生成的 test-harness 版 computer-host 当作正常 release Host。不需要 repo、fixture 文件或仿造 `/Volumes`。

实现者没有生成新的测试 exe，因此不会提供虚构的产物 hash。CC 在拷贝前和 Windows 接收后对四个 exe 核对 hash，记录源码冻结清单、实际 bin 来源、命令/exit/stdout/stderr。

### Windows 执行（仅 CC）

在新 bundle 目录，用 PowerShell；先记录并清除可能指向旧 bin 的测试 override，让本轮验证真正覆盖同目录定位：

```powershell
$ErrorActionPreference = 'Stop'
Remove-Item Env:RPA_TEST_COMPUTER_HOST -ErrorAction SilentlyContinue
Remove-Item Env:RPA_TEST_COMPUTER_CLIENT -ErrorAction SilentlyContinue
Get-FileHash .\lib-tests.exe, .\remote-transport-tests.exe, `
  .\computer-host.exe, .\computer-client.exe -Algorithm SHA256 |
  Select-Object Path, Algorithm, Hash | Export-Csv .\bundle-sha256.csv -NoTypeInformation

# 先定向 TLS（包括嵌入资产 helper 的五项），再全 lib，最后 remote。
& .\lib-tests.exe 'mcp::remote::tls::' --test-threads=1 *> .\tls-stdout-stderr.log
if ($LASTEXITCODE -ne 0) { throw "TLS failed: $LASTEXITCODE" }
& .\lib-tests.exe --test-threads=1 *> .\lib-stdout-stderr.log
if ($LASTEXITCODE -ne 0) { throw "lib failed: $LASTEXITCODE" }
& .\remote-transport-tests.exe --test-threads=1 *> .\remote-stdout-stderr.log
if ($LASTEXITCODE -ne 0) { throw "remote failed: $LASTEXITCODE" }
```

如 bin 不同目录，仅测试 harness 使用以下显式绝对路径；变量不传递任何生产新行为：

```powershell
$env:RPA_TEST_COMPUTER_HOST = 'C:\approved-bundle\computer-host.exe'
$env:RPA_TEST_COMPUTER_CLIENT = 'C:\approved-bundle\computer-client.exe'
```

普通新增 TLS/helper 测试只做 temp 文件与内存配置操作，无真实桌面副作用。remote 场景会创建 loopback TCP/TLS 和真实 **mock-backend** 子进程，不应触发 native input、桌面截图或 GUI，不属于真实桌面反馈/Stop/capture exclusion 验收。全 lib 保留既有 ignored 用例，本轮命令不使用 `--ignored`；本任务不扩张审计或修改其他 native 测试。

## 尚未验证

- Windows native 执行后的 TLS / helper / lib / remote green，含新 helper 的异常路径与 Drop 清理。
- 新 portable bundle 在无源码、与构建机不同 cwd 环境中的实际运行。
- remote 原有成功/拒绝/超时断言在新产物上全部保持通过。
- 无 Mac/Linux 测试，无真实 GUI、feedback Stop/capture exclusion 验收。编译通过不替代上述任何结果。

## 源码冻结 SHA256

以下六个源码文件已冻结，等待 CC Windows red/green；后续如需修改，须重新同步冻结清单。

```text
0284fade17ebc8a5060124c7115ba7a280f7b59f2315b4112e2779bc268b09e1  src/mcp/remote/tls.rs
99696850f641541f6ab4ccaa8fee7009ff21b3f8d2cd27950c1c1fc342b3ff35  tests/remote_transport.rs
d401cba330f8df9908e0c7ebb0eea47d7d01eb7c9a933d664697c6bc7fa72097  tests/support/remote_assets.rs
fe2c1f60a24fa243ae1de6c518778872a4e13f4042b4e9ca78f5195793c25da6  tests/support/remote_assets_tests.rs
1450cac329e2de1e14440221708b04429949b146a841f43fb9a7a658be1d29ba  tests/support/remote_bundle.rs
8d3762932db48c336bb21f0231efc2cb94b5436219fd23b19660fd347b2dc5c6  tests/support/remote_bundle_tests.rs

```
