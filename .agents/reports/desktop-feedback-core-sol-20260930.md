# Desktop feedback Rust core 交付报告

日期：2026-09-30。工作目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。
本次交付是**基础库源码、纯测试源码及编译检查**，不是测试通过或 GUI 验收结论。

## 冻结交付状态

**已冻结，供后续 Host 接线任务使用。** 当前交付的编译检查已完成，源码交付不等待 CC 测试。
本任务到此结束，不扩展至 Host/root 接线，不继续执行测试或验收。CC 后续可按本文命令
执行 33 个已写纯测试用例与 clippy；若发现问题，由协调者另行反馈/分配。

用户告知 native Win 构建/目标机 57 tests 已由 CC 通过，root 正由 CC 复测；这些是
其它模块的外部状态，不计入本 crate 的测试结果，也不作为本任务已验收的证据。

## 分工及写集

- 按最新指令：Codex 实现代码，测试/验收交协调者调度真实 Claude Code + GLM。
- 本任务未启动其它 agent 或 agent CLI、GUI、SSH，未执行真实输入，没有 worktree、commit/push。
- 只写新 `crates/desktop-feedback/**` 与本报告。未修改 root Cargo/src/设计文档，
  未恢复或清理已有脏工作区，未改其它 agent 的 native-input / fixture 写集。
- 独立 `[workspace]`，package `rpa-desktop-feedback`，直接依赖仅 `serde`、`serde_json`。
  构建产物限定 `crates/desktop-feedback/target`；它在 crate 自身 `.gitignore`中忽略。

## 实现内容及公共 API

| 入口 | 已实现的源码行为 |
|---|---|
| `protocol::{HostMessage,RendererMessage,Snapshot,Session,V1,...}` | 严格 v1 snake_case 类型、未知/重复字段拒绝、必需 nullable、锁定枚举和无敏感扩展字段 |
| `codec::{NdjsonDecoder,encode_*,decode_*}` | 16KiB 含 LF 帧上限，固定读缓冲，坏帧终止，无换行 EOF 拒绝，无内容回显诊断 |
| `channel::{Publisher,Subscriber,SnapshotCursor}` | 单 latest 完整 snapshot、1..64 订阅上限、非阻塞 try_lock、严格递增发布序号、旧展示帧忽略与新订阅重同步 |
| `transport::{SnapshotWriter,WriteProgress}` | 每流最多一个部分写帧，一次最多一个 write，先完成旧帧再取最新；Sent 不代表 UI 或 cleanup 成功 |
| `control::{StopInbox,ControlGate,StopToken}` | 1..64 有界 stop 队列、重复合并；消费时按当前 session+generation 授权；立即撤销；清理待确认期间禁止新 grant |
| `state::{ProtocolMachine,Failure,Timeouts}` | ready/heartbeat 默认 5s deadline，duplicate ready / renderer error / 倒退时钟失败，sticky stop-required，不可晚到消息复活 |
| `process::{Supervisor,PrivateSpawner,ProcessControl,RendererCommand}` | enabled 延迟命令解析和握手门禁，off 不解析/读取 renderer 路径、不 spawn/读流/poll child；shell-free 私有管道 command 构造及可注入 child supervision |
| `process::{begin_shutdown,poll_shutdown,ShutdownStatus}` | 实际 terminate/reap 状态驱动，准确区分失败与仍等待；Drop 仅 best-effort，不能冒充已回收 |

完整公共 API 摘要、Host 接线前置条件与待执行命令：
`crates/desktop-feedback/IMPLEMENTATION.md`。
完整行为、容量、字段长度、所有权与 race 规则：`crates/desktop-feedback/README.md`。

## snapshot/control 顺序与 race

1. 单 Host lifecycle owner 决定事实并分配流 sequence；只发布完整当前 snapshot。
2. pointer 必须是确认派发后的记录，owner 匹配 session/内部 epoch/surface version 后
   合并到当前事实；不从旧 snapshot 克隆后赋新序号，不发送计划输入，权威切换清空 pointer。
3. 最新 snapshot 可替换待展示帧，但不能截断已经部分写入的 JSON。旧 UI 状态暂时可滞后，
   其 stop 仍在消费时按当前 Host 控制权重新验证。
4. 接受 stop 的同一次 gate 调用立即撤销 current，产生 pending token；Host 随即使真实
   控制权失效、执行取消/清理，不等消息发送或 UI ACK。pending 未清理确认前拒绝新 grant。
5. 排队旧 A stop 与新 B grant 不交叉授权；cleanup token 关联 epoch，不能延迟取消 B。
6. enabled renderer loss / 协议损坏 / 队列溢出均为 sticky stop-required，绕过已满队列的
   可信 Host 失联路径可 `revoke_current()`；Host 必须拒绝未来 grant 直到新 renderer 真实 ready。
7. released / failed / unknown 来自真实 input cleanup；进程关闭、EOF、发送成功都不是清理证明。

## Geometry.version 集成纠正及 renderer owner 同步

已只读核对 `src/backend/capture.rs::geometry_version`：格式为
`d{display_id}:o{x},{y}:i{w}x{h}:c{w}x{h}:r{rotation}`。

修正为专属 `Surface.version` validator，允许 **1..128 bytes ASCII `[A-Za-z0-9_.:,-]`**，
因此真实 `d1:o0,0:i1512x982:c3024x1964:r0`和负 origin 可原样通过基础库边界。
`Session.id`仍只允许 `[A-Za-z0-9_.:-]`，未放宽到逗号或用户文本；`surface.id`仍由 Host 按
`os:id`构造。新增真实 geometry version roundtrip 与身份不放宽的测试源码，未执行。

已通过当前会话向协调者报告 renderer owner 的同步要求：**版本串 opaque 保留逗号，
不得套用 session token 校验、不得清洗/改写；v1 wire 字段不变。**未调用其它 agent。

## 实际执行及精确结果

工具版本（只是工具检查）：

```text
cargo 1.98.1 (797e8a9bc 2026-08-05)
rustc 1.98.1 (48a229cea 2026-09-01)
```

执行了 `cargo fmt`格式化本 crate，以及必要编译检查。末次编译命令：

```sh
CARGO_TARGET_DIR="$PWD/crates/desktop-feedback/target" \
  cargo check --manifest-path crates/desktop-feedback/Cargo.toml --all-targets --locked
```

末次输出：

```text
Checking rpa-desktop-feedback v0.1.0 (.../crates/desktop-feedback)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s
```

**退出码 0**。library、2 个源码 unit-test 用例所在 lib test target，以及 3 个 integration-test
源码 targets 可通过编译检查。`cargo metadata --no-deps --locked`确认 workspace root 与 target
路径是此独立 crate，直接依赖只有 serde/serde_json。

**实际执行测试数：0；cargo test 未运行；clippy 未运行；格式检查验收未运行；GUI 验收未运行。**
用户调整分工前也没有运行测试，不存在需要标记为历史开发测试自检的测试通过结果。
测试源码先于相应实现编写，但没有执行 TDD RED/GREEN，不声称它们已经通过。

静态 `#[test]`计数（不是测试执行结果）：

| 文件 | 已写用例数 |
|---|---:|
| `src/channel.rs` | 2 |
| `tests/wire_codec.rs` | 9 |
| `tests/channel_control_transport.rs` | 8 |
| `tests/state_process.rs` | 14 |
| 合计 | **33** |

覆盖源码包括超大/无换行有界、版本攻击/未知敏感字段/重复字段、真实 geometry version、
缺 session/过期代次 stop、序号严格单调、快写慢读/单槽重同步/部分写边界、锁竞争不等待、
ready 失败/重复 ready/heartbeat 失联、control overflow、fake child terminate/reap 失败与 off 无副作用。
以上仅描述已写测试的意图，结果全部待 CC。

## 待协调者交给 Claude Code + GLM 的命令

```sh
cd /Volumes/doc/workspace/datagrand/rpa/rpa-computer
export CARGO_TARGET_DIR="$PWD/crates/desktop-feedback/target"
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked
cargo clippy --manifest-path crates/desktop-feedback/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path crates/desktop-feedback/Cargo.toml --all -- --check
```

失败定位可用（仍未执行）：

```sh
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked --test wire_codec
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked --test channel_control_transport
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked --test state_process
```

重点 review：严格 serde nullable/duplicate 字段、Host owner 的 stop 取消排序、single-slot
Contended 单槽重试、旧 partial frame 不跨新流、失败 sticky 通知不可丢失、真实 geometry version。
未启动独立 reviewer agent；独立 review/测试同样交协调者。

## 文件清单

```text
crates/desktop-feedback/.gitignore
crates/desktop-feedback/Cargo.toml
crates/desktop-feedback/Cargo.lock
crates/desktop-feedback/README.md
crates/desktop-feedback/IMPLEMENTATION.md
crates/desktop-feedback/src/lib.rs
crates/desktop-feedback/src/protocol.rs
crates/desktop-feedback/src/codec.rs
crates/desktop-feedback/src/channel.rs
crates/desktop-feedback/src/control.rs
crates/desktop-feedback/src/transport.rs
crates/desktop-feedback/src/state.rs
crates/desktop-feedback/src/process.rs
crates/desktop-feedback/tests/wire_codec.rs
crates/desktop-feedback/tests/channel_control_transport.rs
crates/desktop-feedback/tests/state_process.rs
.agents/reports/desktop-feedback-core-sol-20260930.md
```

编译产物：`crates/desktop-feedback/target/**`，不作为源码交付。源码各模块最长 320 行；
没有单个 1000+ 行的实现文件。

## 未集成/未验证限制

- root Host、Runtime、native-input、capture、renderer 本次均未接线；没有实际子进程适配器。
  PrivateSpawner 必须保证私有管道与被控端交互桌面；command 构造本身不证明这一点。
- Host 必须按 README 周期 poll、使用单调时钟、在 ready 前拒绝 startup/grant，并把 sticky
  failure 接到真实 revoke/cancel/cleanup。库本身不能改变 Host session/auth 状态或释放输入。
- 对 full snapshot 提供事实、跨 session 的 id/代次发行、不接受旧 pointer/geometry，均属于
  Host lifecycle owner 的明确责任；不会建立无界 session 历史表或通用插件系统。
- 非阻塞 pipe adapter 或独立 I/O worker、有限 reader chunk、进程 teardown deadline/线程回收
  需 Host 实现；不得从输入/UI线程调用阻塞 Write，Drop 不保证 reap。
- Requested capture exclusion 不是实机验证；Mac/Win 用户可见性、停止区域点击、焦点/装饰穿透、
  实际模型 screenshot 排除、远程 Windows 交互 session 与真实释放失败准确显示都待 CC + GLM。
- 由于测试/验收未执行，本报告仅可用于源码/API 交接，不可用于宣布安全门禁通过。
