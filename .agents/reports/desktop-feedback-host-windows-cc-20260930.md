# Desktop feedback Host 接线 — Windows 跨构建 + 非 GUI 门禁（CC 报告）

日期：2026-09-30。执行者：CC（config sonnet alias，实际 glm-5.3-flash）。
工作目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`（源码冻结，未改动任何
product/test/fixture/build 文件；无 commit/push/worktree）。

**环境修正**：Windows 原生无完整 Rust/MSVC 工具链。全部构建/编译在 Mac 上以
`cargo xwin` 交叉构建完成（仅构建，不执行）；测试 exe 通过 SSH 拷贝到 acer-win
新独立 TEMP 目录执行。未安装任何工具链，未触碰旧 target/、旧 18:09 release、
用户 Notepad 或防火墙。未启动 Host/renderer/GUI；GUI 套件因网络权限弹窗继续阻塞。

## 1. 源码冻结 hash 核对

对 SOL 报告列出的 26 个文件逐一 `shasum -a 256`，**26/26 与冻结 hash 完全一致**。
清单：`logs/frozen-hash.txt`。

## 2. 普通测试副作用审查（决定可执行集）

只读审查结论（覆盖 src/ 全部 #[cfg(test)]、tests/ 目录、crates）：

- lib/binary 内普通测试全部为内存 mock / argv 解析，无真实截图、注入、窗口枚举。
  `src/backend/live_test.rs:12` 的真实截图诊断 **已 #[ignore]**。
- `src/mcp/remote/host.rs:528` 的 `Command::new("owned-child.exe")` 仅构造用于
  检查 argv，从未 spawn；`#[cfg(windows)]` 的 exit-decode 测试无副作用。
- `crates/native-input/src/windows.rs` 测试不调用 SendInput（仅构造 INPUT 记录）。
- **无 include!/include_str!/include_bytes!**；无 Mac-only fixture 路径导致交叉编译失败。

排除/不可执行测试（精确清单）：

| 排除项 | 原因 |
|---|---|
| `tests/remote_transport.rs` 全部（~15 测试） | 编译期 `env!("CARGO_BIN_EXE_computer-host"/"computer-client")` 与 `env!("CARGO_MANIFEST_DIR")`（TLS fixture, `src/mcp/remote/tls.rs:105`）把 **Mac 路径** 烧入交叉构建产物，Windows 上不可执行；且会 spawn 真实子进程 + 回环 TLS。本批未运行。 |
| `mcp::remote::tls` 2 个 fixture 测试 | 同上 env! Mac 路径；实际运行失败（见 §4）。 |
| `src/feedback/tests_process.rs` 全部 5 个（IGNORED） | 本任务未授权；需独立协调的 active-console 进程生命周期运行 + 回收证据。**Pending。** |
| `backend::live_test::live_backend_capture_only`（IGNORED） | 真实截图诊断，未授权。 |
| `tests/contract/registry.rs:53` 1000-request bound（IGNORED） | 慢测试，未运行。 |

未使用 `--ignored` / `--include-ignored`；每个 exe 均以 `--test-threads=1` 运行。

## 3. 跨构建（Mac → x86_64-pc-windows-msvc）

环境（`logs/build-env.sh`）：

```sh
eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"
export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export AR_x86_64_pc_windows_msvc="$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/rust-lld"
export ARFLAGS_x86_64_pc_windows_msvc='-flavor link /lib'
export CARGO_TARGET_DIR="$PWD/.agents/runs/desktop-feedback-host-windows-cc-20260930/target"
```

| 命令 | exit | 结果 |
|---|---|---|
| `cargo test --target x86_64-pc-windows-msvc --all-targets --no-run --locked --offline --message-format=json` | **0** | Finished test profile 30.18s；0 warnings；7 个测试 exe（JSON：`logs/cargo-test-norun.jsonl`） |
| `cargo build --target x86_64-pc-windows-msvc --release --bins --locked --offline --message-format=json` | **0** | Finished release profile 51.13s；0 warnings |

编译一次通过，**无阻塞，未返 Codex**。完整 stdout/stderr：
`logs/cargo-test-norun.{jsonl,stderr}`、`logs/cargo-build-release.{jsonl,stderr}`。

测试 exe（取自本次 JSON artifact）：

```
rpa_computer-3769e6ba7e450727.exe   c92b747fec8d400cc3174deed967ddfbd84d712a18a19f05881e4c36c5452a5b
computer_host-f7adec8f390e6073.exe  7a535da3f855d98ca5ceb43b3ebedaa03911cd3c20a7b157aa202d1f71a30e53
protocol-3992069d7459467f.exe       193b1d6b5adfbae4ecfe3c06f49bb787f9ed05ddda357919f6ed4d2aa73b22cf
transport_lifecycle-4dd438721023ae64.exe 7c8bccff524c7328ac3f503c4b444b29c8be4d524f7ce41df9bc78b365ace81b
runtime_contract-e5ec4fa11352812c.exe   4b68758e44b8db566f502d76f1e3b9ceb31dc86b1cd49828a40f765802b2a9c2
```

Release bins（本次新 target，未覆盖旧产物）：

```
release/computer-host.exe   8c36602a969034feaba6cfb48b6a7b0a5f76e51f6d824c72434faa15ce6a6614
release/computer-client.exe f40b68a39111225b7d03893c7dc2283dba418854c2793c3386f7bb031c0b3f75
```

**注意：以上 release bin 不构成可执行验证** —— 未在 Windows 上启动过（GUI 门禁阻塞）。

## 4. Windows 测试执行（acer-win）

新 TEMP：`C:\Users\Administrator\AppData\Local\Temp\dfb-host-cc-20260930-r1`（新建，
唯一）。5 个 exe 拷贝后 certutil SHA256 **5/5 与 Mac 端一致**。执行命令（每个 exe）：

```
powershell -NoProfile -Command "Set-Location <TEMP>; & .\<exe>.exe --test-threads=1 2>&1 | Out-String; exit $LASTEXITCODE"
```

| 二进制 | 结果 | exit |
|---|---|---|
| `rpa_computer` (lib) | **237 passed; 2 failed; 6 ignored**；238.52s | 101 |
| computer_host bin | **3 passed; 0 failed** | 0 |
| protocol | **12 passed; 0 failed**；9.59s | 0 |
| transport_lifecycle | **3 passed; 0 failed**；0.75s | 0 |
| runtime_contract | **35 passed; 0 failed; 1 ignored**（即 §2 所列 1000-request 慢测试） | 0 |

**lib 的 2 个失败（不隐藏，未重跑变绿）**：

- `mcp::remote::tls::server_config_loads_fixture` (tls.rs:113)
- `mcp::remote::tls::client_config_loads_fixture_ca` (tls.rs:124)

原因：`src/mcp/remote/tls.rs:105` 使用编译期 `env!("CARGO_MANIFEST_DIR")`，交叉编译
时烧入 Mac 路径 `/Volumes/doc/...`，Windows 上不存在。**这是环境边界而非产品缺陷**；
同模块的 reject 测试（mismatched key、garbage）均通过。判定为 unexecutable-in-
cross-build，不计为通过也不计为产品失败。Windows 原生构建（将来若有工具链）预期通过。
若希望交叉构建可测，需 Codex 把 fixture 路径改为运行期解析 —— 仅记录，未改代码。

（其余 4 个 exe 结果见上表与 `logs/{bin-computer-host,test-protocol,test-transport-
lifecycle,test-runtime-contract}-stdout.log`。）

## 5. tests_process 专项（pending，未授权）

`src/feedback/tests_process.rs` 5 个 IGNORED 测试未运行。它们需要：交互式用户 desktop
（非 Session 0 SSH 上下文）、独立 rustc 编译 `fake_renderer.exe`、
`RPA_FEEDBACK_TEST_RENDERER` 环境变量、协调的进程生命周期与回收证据。即使 fake 不画
GUI，进程启动/回收仍需 active-console 独立运行批次。**Pending，未授权。**

真实 stop/overlay/WM affinity/截图排除仍 pending（需默认 renderer + GUI 门禁解锁）。

## 6. 独立 spec/质量审查（与上面实测分开；绿色 mock ≠ 原生 release 证明）

逐项审查结论（基于冻结源码阅读）：

1. **off 零副作用**：成立。`FeedbackHost::start` 在 `enabled()` 为 false 时于
   validate/command/spawn/thread 之前返回 Ok(None)（host.rs:45-47）；
   `Worker::start_config` off 分支不分配 adapter/child/pipe（worker_lifecycle.rs:19-34）。
   off 时无 desktop/security 调用。
2. **enable 明确失败**：成立。ready 校验在精确帧边界：`read_tick` 内
   `validate_ready`（host.rs:288-290）使 unsupported 的后续 error/EOF 不会掩盖能力
   错误；`can_accept_control` 分支二次校验（host.rs:201-216）。ready 超时 8s（略大于
   文档 5s heartbeat 语义，为 STARTUP_BOUND，可接受）。
3. **Stop 撤销原子性 / 不可重开竞态**：成立。`FeedbackHandle::stop` 在单一
   owner 临界区内同时 `terminated.store(true)` + `authorize_stop` + snapshot
   stopping（authority.rs:131-147）；grant/resume/begin_dispatch 均先查
   `is_terminated()`；terminal 后无 open/resume 路径重开（retire≠stop：
   `complete_terminal` 仅清理，不 re-grant）。worker stop hook 先 bump epoch、latch
   cancel、再发 Quit（worker_lifecycle.rs:22-29）。
   - 微观察（非缺陷）：`stop()` 中 `edit` 返回 `Ok(false)`（未授权）时也会因
     `matches!` 不命中而不 latch，正确；`terminate()` 用 swap 保证只 notify 一次，
     正确。
4. **有界 child/pipe IO**：成立。每 tick 最多 4×4096 字节读（MAX_READS_PER_TICK，
   host.rs:274）；写侧 SnapshotWriter pump + 5s WRITE_BOUND 触发
   transport_failed + terminate（host.rs:240-246）；单 IO 线程轮询，输入线程不阻塞
   在 child I/O；facts owner `try_lock` 非阻塞（authority.rs:280-286）。停机 2s
   reap 界 + join 上界，超时报告 ProcessUnconfirmed/ThreadUnconfirmed 且 sticky
   （host.rs:92-115, Drop 不洗白）。
5. **cleanup failed/unknown 不当安全**：成立。`complete_terminal`：cleanup Unknown
   → `quarantined=true` 永久（authority.rs:202-223），Failed → Phase::Faulted；
   `shutdown_native_worker` 对 quarantined/faulted 状态强制 Unknown 且记录 sticky
   （worker_lifecycle.rs:148-151, mark_unknown）；重复 shutdown 返回记录值而非 Clean。
   Runtime shutdown 映射 unknown/其他 → Cleanup::Unknown（runtime.rs:512-527）。
6. **remote argv 密钥隔离**：成立。`append_args` 只传
   `--desktop-feedback/--feedback-accent/--feedback-label`（config.rs:35-45）；remote
   child spawn 用 `current_exe()` + token/TLS 材料不进 child（host.rs spawn 段注释与
   实现一致）；argv-only 有专门测试 `feedback_forwarding_is_argv_only_and_contains_no_
   tls_or_token_fields`（host.rs:522，本批 lib 内通过）。

局限声明：以上为静态源码审查，不能替代 Windows 原生运行时验证（pipe DACL、
desktop 一致性、logon-SID 预检等均未运行时证明）。

## 7. 产物索引

- 日志/hash：`.agents/runs/desktop-feedback-host-windows-cc-20260930/logs/`
- 新构建 target：`.agents/runs/desktop-feedback-host-windows-cc-20260930/target/`
- Windows TEMP：`C:\Users\Administrator\AppData\Local\Temp\dfb-host-cc-20260930-r1`
- 旧 target/x86_64-pc-windows-msvc/release（18:09）与已部署 baseline：**未触碰**。

## 结论

- 跨构建（test all-targets + release bins）：通过（exit 0，0 warnings）。
- Windows 执行：合计 **290 通过 / 2 TLS-fixture 环境性失败 / 7 IGNORED**（lib 237+2+6、
  host bin 3、protocol 12、transport_lifecycle 3、runtime_contract 35+1）。无重跑变绿。
  **无 fake 成功**：remote_transport 整体、tests_process 5 个、live capture、GUI 门禁均
  明确 pending。
- 一次性说明：本任务的测试副作用审查使用了 1 个只读 Explore subagent（无写入）；
  其余全部本地/SSH 直接执行。
