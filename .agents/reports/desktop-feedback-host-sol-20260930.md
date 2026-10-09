# Desktop feedback 真实 Host 接线 — Codex / gpt-6.1-sol 实施交接

日期：2026-09-30；工作目录 `/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。

## 状态和源码冻结

**Host 接线与测试源码现已冻结，交给协调者的 CC+GLM 在 Windows 原生环境构建/执行。**
这是实现交接，不是通过验收声明。若 CC 发现问题，由 Codex 修改；CC 不兼任源码实施。

- 已完整阅读 host-integration task、v1-wire task、独立 feedback crate README/IMPLEMENTATION，
  并核对 `.agents/README.md` / `.agents/CONTRACT.md` 最新角色规则。
- 初始 root 写入门禁遵守；收到明确 `ROOT_WRITE_RELEASED` 后才修改 root 接线。
- 未执行测试、self-test、clippy、GUI、截图、Host/renderer 启动或 SSH；未启动其他 agent/CC。
- 未 reset、worktree、commit、push；保留共享脏树。下列既有文件原本也有其他任务改动，
  本报告不把整个共享 Git diff 归为本任务成果。
- **从未执行 `cargo build --release`；没有重写、删除或移动
  `target/x86_64-pc-windows-msvc/release` 内任何 bin。旧 18:09 release 不包含此接线。**
- 本任务开发编译原输出在 `.agents/build/desktop-feedback-host*`，已仅将本任务自己新建的
  两个 dev/check 目录归档到忽略目录 `.agents/runs/desktop-feedback-host-sol-20260930/build/`。
  没有移动其他任务产物。CC 应使用新的独立 target 目录，不将旧 release 作为 feedback 产物。

## 精确改动文件

修改既有接线文件：

1. `Cargo.toml`：path 依赖 `rpa-desktop-feedback`；Unix 管道使用 `libc`。
2. `Cargo.lock`：保留已有锁文件变化，仅由开发 cargo check 追加 feedback path package / root dependency。
3. `src/lib.rs`：导出 `feedback`。
4. `src/runtime/runtime.rs`：可选 facts handle / decorator；实际成功 open 授权，实际状态同步，
   terminal 输入/生命周期拒绝，实际 shutdown cleanup 汇报。
5. `src/mcp/worker.rs`：可选 startup / shutdown flag / admission 拒绝 / quarantine 接线；
   将既有 construction/join 原样模块化迁移，最终 906 行。
6. `src/mcp/worker/native.rs`：native gap 的 terminal gate；既有 dispatch helper 从 worker 迁入；
   新增 mock native-gap 测试源码。未修改既有 `worker/tests.rs`。
7. `src/bin/computer-host.rs`：CLI 参数/校验/帮助/describe、可选 worker startup、unknown 非零退出、CLI 测试源码。
8. `src/mcp/remote/host.rs`：`RemoteArgs.feedback`、完整私有 argv 透传；exit 7 / 被迫 kill 的 child
   quarantine；kill 后有限 `try_wait` 取代无界 wait；argv 和 exit classification 测试源码。
9. `README.md`：追加可选反馈的使用、状态/安全语义和未验收说明。

新增独立模块：

- `src/feedback/mod.rs`
- `src/feedback/config.rs`
- `src/feedback/authority.rs`
- `src/feedback/backend.rs`
- `src/feedback/host.rs`
- `src/feedback/process.rs`
- `src/feedback/pipe.rs`
- `src/feedback/pipe/unix.rs`
- `src/feedback/pipe/windows.rs`
- `src/feedback/pipe/windows_security.rs`
- `src/feedback/pipe/windows_desktop.rs`
- `src/feedback/worker.rs`
- `src/feedback/worker_lifecycle.rs`：按 path 编译为 worker 的 child module，私有状态不扩为公共 API。
- `src/feedback/tests.rs`
- `src/feedback/tests_runtime.rs`
- `src/feedback/tests_process.rs`：Windows-only，全部显式 ignored。
- `src/feedback/fixtures/fake_renderer.rs`：独立 rustc 编译的纯 std 私有协议 fixture，非 GUI renderer。

没有修改 `native-input`、独立 `desktop-feedback` crate、`desktop-feedback/**` 平台 renderer、
acceptance fixture 或新 `crates/macos-capture`。

## 公共 API / 接线语义

- `FeedbackConfig { executable, accent, label }`：默认 off；style 不能单独设置；无 shell 拼接。
  `append_args` 仅向 owned child 传反馈配置，不传 TLS key、token、日志参数。
- `FeedbackHost::start`：off 在解析/查文件/spawn/thread 之前返回 None；enabled 真实 private
  spawn 后严格等待 ready（5 秒），unsupported 拒绝；ready 边界即校验，避免随后的 error/EOF
  抹掉明确 capture 能力错误。`requested` **始终不等于截图排除已验证**。
- `FeedbackHandle`：一个有界 facts owner，单调 session generation / sequence，复用 core
  `ControlGate`。成功 Runtime open 才 grant；ordinary close 可新开，terminal Stop 不可重开。
- `FactBackend`：native thread 本地 decorator；只有实际 capture / inject 回调驱动
  observing/executing，成功 inject 才确认 move/click/drag；pointer 内部关联 session+generation
  和 surface id/version，旧 surface / terminal 后迟到记录丢弃。无文本/key/截图/标题内容进入 wire。
- `Runtime::new` 保持自然 off 路径；`new_with_feedback` 仅 enabled 安装 decorator。
  open/resume 被 Stop 竞态撤销后不能成功，也不能靠清 cancel 恢复控制；input partial 结果不伪造。
- `Worker::start_with_feedback` / `start_with_feedback_shutdown`：终止 hook 先撤销权利，再 bump
  worker epoch / latch cancel / 发 Quit；生产 Host 同时设置 transport shutdown flag。
  admission、Runtime、pre-native、每次实际 inject 都检查永久 terminal。旧 stop 不停新 session。
- Stop 的 session+generation 验证与撤销在同一 facts 临界区；临界区无 native / 管道 I/O。
  ready/heartbeat/EOF/坏帧/溢出失联走 sticky fail-stop，不依赖可丢通知队列。
- 实际 Runtime shutdown 的 Released/NotNeeded 才 closed；Failed 为 faulted；worker abandonment /
  quarantine 为 Unknown，迟到 cleanup 不得改成安全完成。进程 kill/reap 不作为 input release 证明。
  fault/cleanup snapshot 尽力短时发送，不能送达时 stderr 给出准确诊断，不无限挂 Host。

## IO 和回收边界

- 一个独立 polling IO 线程，无 blocked reader/writer helper；输入线程不等 child I/O。
- Core 16 KiB NDJSON、latest 单槽、单未完成 frame、16 个 raw stop 上限；每 tick 最多读
  4×4096 bytes；事实 owner 只保留最新完整 state，publisher contended 留最新重试，不忙等。
- ready 5 秒 / heartbeat 5 秒；有 live heartbeat 但 snapshot 单帧持续堵塞 5 秒也 fail-stop。
- Windows Host 端为 byte-mode `PIPE_NOWAIT` 私有管道；child 为普通 blocking stdio。
  随机管道名、logon-SID-only DACL、拒绝远程 pipe client、不继承其他私有 handle。
  启用时校验非 Session 0，输入 desktop 与当前 thread desktop 一致；off 不做这些调用。
- renderer 环境为限量 desktop/系统变量白名单，不继承 Host 认证/API 环境；stderr 为 null，
  不另设可能堵塞的未排空诊断管道。
- child terminate/reap deadline 2 秒；Host IO join 3 秒上界，失败明确 ProcessUnconfirmed /
  ThreadUnconfirmed 并使 worker Unknown，重复 shutdown 不翻成成功。
- OS 拒绝 kill/reap 或调度异常时，不能保证绝对无遗留：实现明确报告 unconfirmed、quarantine，
  没有用 Drop/发送成功冒充实际回收。常规实际回收仍需 CC process/handle 证据。

## 测试源码覆盖与副作用

**没有测试执行 / RED-GREEN / 测试通过证据。**

新增普通测试：off、style/argv/describe、unsupported/requested 区分、旧 session/generation Stop、
新 session 不被旧 Stop 取消、terminal 后新 open/resume/input 拒绝、实际 callback phases 与
pointer（含 Runtime 成功 click）、拒绝不派发、秘密文本不进 wire、旧 geometry / late pointer、
实际 release 成功/失败/unknown sticky、geometry 内 open/resume 竞态、长 key_hold 的 mock
取消+release、worker→native gap 的 fresh epoch 也不能恢复 terminal 权利、remote exit 7 quarantine。
这些只用内存 mock / CLI argv；**不产生真实桌面输入、截图、窗口或 renderer 子进程**。

Windows 专项 ignored tests：缺文件/无 ready/unsupported/超大/错误帧、EOF/heartbeat loss、
慢 reader 不阻塞 publication、live heartbeat 掩盖 writer 堵塞时 fail-stop、wire Stop 真正到
worker/cancel/Quit、重复 shutdown/reap。它们启动本任务纯 std fake exe，并做 enabled 的被动
session/desktop/logon 身份预检；**无 GUI / capture / input，不代表默认 renderer 真 ready**。
需 CC 在交互式用户 desktop 执行，不能在 Session 0 的 SSH 服务上下文运行。

既有 root 的 `backend::live_test::live_backend_capture_only` 是显式 ignored 的真实截图诊断；
不要全局 `--include-ignored` / 无过滤 `--ignored`。完整 root 其他历史测试由 CC 自行审查，
本报告的“无真实桌面副作用”保证只针对本次新增普通/专项 mock tests，不能替代全套审查。

## 开发编译证据（不是测试/验收）

1. 首次 root `cargo check --all-targets --offline` exit 101：native_main 新参数尚未透传；已修正。
2. 后续以及**最终源码**的以下命令 exit 0（最后输出 `Finished dev ... in 1.51s`）：
   ```sh
   CARGO_TARGET_DIR="$PWD/.agents/build/desktop-feedback-host" \
     cargo check --all-targets --offline --locked
   ```
   本机 target 为 aarch64-apple-darwin，只做类型/编译检查，没有执行 Mac 测试、Host 或 GUI。
   Windows-only tests 不会由这次本机 cfg 编译；还需 Windows 原生 all-targets check。
3. 完整 Windows 交叉检查尝试，**均非通过**：
   ```sh
   CARGO_TARGET_DIR="$PWD/.agents/build/desktop-feedback-host-win" \
     cargo check --target x86_64-pc-windows-msvc --all-targets --offline --locked
   CARGO_TARGET_DIR="$PWD/.agents/build/desktop-feedback-host-win" \
     cargo xwin check --target x86_64-pc-windows-msvc --all-targets --offline --locked
   ```
   第一条 ring C 构建缺 Windows `assert.h`；第二条 SDK/clang 能编译 C，但缺 `llvm-lib`，exit 101。
   没有修改 vendor/build script 或造假库绕过；**Windows root 构建和链接未验证**。
4. Windows-only 管道/security/desktop 原源码的独立 rustc metadata check exit 0：
   helper 仅按绝对 path 引用这三个真实源码，没有 stub。最初 helper 模块路径失败的两次
   exit 1 已纠正；最终 exit 0。输出 dead_code warning 是 helper 不调用这些入口所致。
   这只证明 Windows Rust 类型检查，不证明系统 ABI 链接、pipe 运行或 desktop 权限成功。
5. `rustc --edition=2021 --emit=metadata --target x86_64-pc-windows-msvc
   src/feedback/fixtures/fake_renderer.rs ...` exit 0，fixture 未执行、未链接成发布 exe。
6. `rustfmt ... --config skip_children=true` 仅整理本人写集，未遍历 frozen crate；
   所分配文件 `git diff --check` exit 0。这不是 CC fmt/clippy 验收。

开发日志/metadata/helper 保留在：
`.agents/runs/desktop-feedback-host-sol-20260930/build/{desktop-feedback-host,desktop-feedback-host-win}/`。

## CC + GLM 精确 Windows 命令

在 Windows 原生 MSVC Rust 工具链、同一交互式用户 desktop、已核对源码冻结 hash 的目录中执行。
这批命令**本任务未执行**；不给 CC 一个含源码的新“已验收”发布结论。

```powershell
# 在 Windows 的 rpa-computer 项目根目录。
# 独立输出，不覆盖旧 target/x86_64-pc-windows-msvc/release 两个 bin。
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target\desktop-feedback-host-cc-20260930'
cargo check --target x86_64-pc-windows-msvc --all-targets --locked
cargo test --target x86_64-pc-windows-msvc --all-targets --locked --no-run
cargo build --target x86_64-pc-windows-msvc --release --bins --locked

# 普通 mock/纯解析回归，ignored 真实 capture 不会运行。
cargo test --target x86_64-pc-windows-msvc --lib --locked -- --test-threads=1
cargo test --target x86_64-pc-windows-msvc --bin computer-host --locked -- feedback_cli_tests --test-threads=1

# CC 专项 fake 子进程，不启动 GUI。仅过滤本次 tests_process。
rustc --edition=2021 src\feedback\fixtures\fake_renderer.rs -o "$env:CARGO_TARGET_DIR\fake_renderer.exe"
$env:RPA_FEEDBACK_TEST_RENDERER = Join-Path $env:CARGO_TARGET_DIR 'fake_renderer.exe'
cargo test --target x86_64-pc-windows-msvc --lib --locked feedback::tests_process -- --ignored --test-threads=1

# 独立 lint/格式门禁；问题返给 Codex，不由 CC 修源码。
cargo clippy --target x86_64-pc-windows-msvc --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

分组定位，可在上面普通回归之前执行：

```powershell
cargo test --target x86_64-pc-windows-msvc --lib --locked feedback::tests_runtime -- --test-threads=1
cargo test --target x86_64-pc-windows-msvc --lib --locked mcp::worker::native::feedback_tests -- --test-threads=1
cargo test --target x86_64-pc-windows-msvc --lib --locked mcp::remote::host::feedback_tests -- --test-threads=1
```

## 待 Windows 真机门禁 / 非承诺

- CC 原生 Windows 完整 root all-targets 编译、release 两 bin 链接和所有测试执行。
- 默认 Windows renderer（不是 fake）真实 ready、所有反馈 HWND 的 API 设置/回读与
  实际模型 capture 排除；`requested` 不代替模型截图证据。
- 停止按钮/关窗口、长拖拽/持键中的取消和真实 held-input 释放；旧/排队 open/resume/step
  拒绝；真实 release 故障及 unknown/quarantine 不假安全。
- 真实点击穿透、停止可达、焦点不抢占、pointer 原生单位/geometry 变化、启用前后原动作回归。
- kill renderer / EOF / heartbeat / 背压故障、owned child/thread/handle 实际回收证据。
- remote supervisor 在正确 Windows desktop 的 owned child 完整透传；新连接授权与 exit 7 /
  forced kill quarantine；TLS/token 不在 renderer wire/argv/env/诊断中。
- 当前只测 Windows。macOS enabled 仍明确 unsupported；没有整合/修改 macos-capture crate，
  不等待其 API、不将它的存在冒充排除已可用。Linux 默认 renderer 与任何 Linux 测试不在本批。

以下 hash 冻结本任务实际接线文件（含其已有共享改动），用于 CC 对齐源码副本。
不冻结/覆盖其他 owner 的 renderer、native-input、fixture 或其他新 crate。

```text
f370986c8615805c2cdfdf1986387e95c24aacab6f6b24e7832be8ed70b0d4f1  Cargo.toml
36354cc795ca582e3bc47fe5bbba3283fadd3357f9146dd637ffabd8640869b7  Cargo.lock
cf27f4a05bc51dbb4840197f970a842f71b01d02da6b6dcc945dd3f1d8fa6bc3  src/lib.rs
00df5b58feab6395251f86028cddb6abe4450fb9b63fd53da0e809fbbea3b348  src/runtime/runtime.rs
df8aafd064a8cbacdc3b5abca27e19bd8fe980031d65891f5639675edb8b8786  src/mcp/worker.rs
37302212085f69f6bf1fa2f93d7c5de6ad05304e231af5d5f3285201dfad731a  src/mcp/worker/native.rs
0c4cc53a688f73fe0f0f1d1ba84be1bfb0ef640679349c543aa53414409ea592  src/bin/computer-host.rs
6bac18b30451d13a82dee42402b4c27e23c61645323501a6467aed92d97297f6  src/mcp/remote/host.rs
1266df0950668de303c2fd0f2c07d50e8a4de6af467a38d1a76ced2761f2f53f  README.md
acb3b500d0b41b58a69250c4098fc90892329ad83b8186202c168b183c96ff13  src/feedback/authority.rs
df5f8f4bdf0513a7277f5f1a509a7a57a0c489e177dbf9a40db94baf07f0a189  src/feedback/backend.rs
a82cfd908f530c63063a6ab157d9e9414dff64ff563dd8d14b0802b739a1b169  src/feedback/config.rs
62ba8a0c11886689b076dff3e774ab5e998faaa95075d19177118424b5256453  src/feedback/fixtures/fake_renderer.rs
e2946b8657f7ad297a8aa17250cecc34c0d90e317256775d731d166faa21953d  src/feedback/host.rs
7c9cff401c02dcd3ea6d1a006f52cfefc81d469b7fbbafe1ad34db5c2c411315  src/feedback/mod.rs
44c99b8f367cf8abef3b1e9abeead22b30373017f181d4aa66dea21401c949e8  src/feedback/pipe/unix.rs
6505f314c513eeedfab4f2398310451ac67281ea9dcff3a42aec2c2a711c97cc  src/feedback/pipe/windows.rs
9435e3c55416b44fbd5eb2e16c5a5a53c657938ba7d75d2832995b583a61a117  src/feedback/pipe/windows_desktop.rs
96aa721cfae68679b96120e4fea4a991a9edc7beb71de0564cf93251e3909c9b  src/feedback/pipe/windows_security.rs
fcd373fa0d1598f0a474023eeb03cdad5699d1bfb225a924b8f8584044259372  src/feedback/pipe.rs
0782d2a41283f6c7a11a6ce89d05e54617305f3deca8ef9e50b9f316a8fc1c38  src/feedback/process.rs
652dcfbef3659f92eac5333dfbf319b05895e8d06822ae9d0a4d358731948736  src/feedback/tests.rs
f78347d1a70ded7dce3ee15ec6b9185c339d686a3a8d3cc4c59280d1ce69b2b3  src/feedback/tests_process.rs
5552cc4687d1642123acfdaf07885ad56860528d55847f93a9aa124bacecfe43  src/feedback/tests_runtime.rs
f83d83e13b5b65b89417a227a97e642936311f562afd7ccb6438f549787febec  src/feedback/worker.rs
6f3ef2dc80f5c1bce18fb1839cb5e063595ac169d4ff1e122d1483851abc283e  src/feedback/worker_lifecycle.rs
```
