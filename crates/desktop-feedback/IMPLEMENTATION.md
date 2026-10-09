# Desktop feedback core — implementation and Host handoff

日期：2026-09-30。交付为独立 `rpa-desktop-feedback` workspace，尚未接入 root Host。
约束：只写此 crate 和指定 report；无 worktree、commit/push、GUI、SSH 或其他 agent。

## 已实现 / 未执行

- [x] 精确 v1 wire 类型、严格版本/字段/枚举/nullable/数值与 token 校验。
- [x] 16KiB 含 LF 的有界增量 NDJSON；坏帧终止、无换行 EOF 拒绝、无敏感回显。
- [x] 单 latest 完整 snapshot pub/sub、订阅上限、单帧部分写入 transport、序号单调。
- [x] 独立有界 stop inbox；消费时验证 session+generation，撤销与清理 gate。
- [x] ready / heartbeat / duplicate ready / 失联失败状态机，sticky stop-required。
- [x] off 延迟命令 resolver；私有子进程 trait / 无 shell command builder /
      真实 terminate/reap 结果接口；纯 fake process/stream 测试代码。
- [x] `cargo check --all-targets --locked` 编译了 library 与测试 targets（没有执行测试）。
- [x] 公共 API、完整 snapshot/control/pointer race 顺序与集成限制见 `README.md`。
- [ ] 所有测试、clippy、格式检查验收，由协调者的 Claude Code + GLM 执行。
- [ ] Host 与 Mac/Win renderer 实际接线、平台 UI 与 input cleanup 验收。

用户中途调整了分工：代码实现由 Codex；其余测试/验收由 CC + GLM。本任务此前也未执行测试。
测试源码先写、实现随后写；未声称观察到 TDD RED/GREEN 或测试通过。

## 最小公共 API 摘要

```text
protocol::{Snapshot, HostMessage, RendererMessage, Session, Ready, V1,
           Phase, Cleanup, Surface, Pointer, PointerKind, CaptureExclusion}
codec::{MAX_FRAME_BYTES=16384, MAX_LINE_BYTES=16383, NdjsonDecoder,
       encode_host, encode_renderer, decode_host_line, decode_renderer_line}
channel::{Publisher, Subscriber, PublishError, SnapshotCursor}
transport::{SnapshotWriter, WriteProgress}
control::{StopInbox, ControlGate, StopToken, StopRejection}
state::{ProtocolMachine, ProtocolPhase, Timeouts, Failure, ReceiveOutcome}
process::{FeedbackMode, SupervisorConfig, RendererCommand, PrivateSpawner,
         ProcessControl, Supervisor, ReceiveSummary, ShutdownStatus}
Diagnostic
```

`Supervisor::start(Enabled)`返回成功只表示已 spawn 且等待 ready；Host startup 必须继续等待
`machine().can_accept_control()`。`start(Disabled)`在任何 renderer 路径解析前返回。

`Publisher::try_publish`接受完整权威 snapshot；Contended/错误携带原 snapshot。
Host 用单槽保存最新待重试状态，不阻塞输入、不忙等；单 `SnapshotWriter`最多一个未完成 frame。

`Supervisor::feed` / `receive`把 ready 后 stop 入有界邮箱；Host owner 消费 inbox 后调用
`ControlGate::authorize_stop`，它立即撤销 current。取消/清理是 Host 的职责；只有实际
Released/NotNeeded 对应 token 能 `finish_stop`，此前禁止再 grant。

`Supervisor::poll`需要单调时钟和周期调度。任何 enabled renderer 故障都持续保留
`Failure.stop_required`；Host 可信失联路径调用 `ControlGate::revoke_current`，不依赖展示序号。

`PrivateSpawner` / `ProcessControl`只负责私有 child/pipe 的平台 Host adapter；本库没有
实际 spawn adapter、不建线程、不监听网络、不含 GUI。`to_std_command`只构造命令。

## snapshot / control / race 的稳定规则

1. 单 Host lifecycle owner 分配全流递增 sequence；所有发布都是完整状态，不是增量事件。
2. pointer 必须来自已派发输入，owner 校验内部 session/epoch/surface version 后合并进
   当前 state；迟到或旧 geometry 的 pointer 丢弃，权威状态切换清空 pointer。
3. 部分写入的旧帧先写完，再取最新；新流不能复用旧帧。UI 暂时落后不授予任何控制权限。
4. stop 在消费时验证当前 id+generation；sequence、管道成功、旧 snapshot 均不授权。
5. 撤销后 pending stop 阻止新 grant，清理结果和 token 关联，防止延迟取消误作用于新控制权。
6. 失联通知是 sticky 状态，不进入可能已满的 control queue；即使无 UI 消息仍要求可信 stop。
7. cleanup / UI 可见 / capture 排除绝不根据“发送成功”推断。

详细边界、字段长度、token 字符集、非阻塞 I/O 与 teardown deadline 责任在 README。

### 给 renderer owner 的 wire 同步（2026-09-30）

已只读核对 `src/backend/capture.rs::geometry_version`，真实格式含 origin 逗号。
`surface.id`仍由 Host 提供 `os:id`；`surface.version`是 1..128 bytes ASCII opaque geometry
版本串，允许 `[A-Za-z0-9_.:,-]`。例如 `d1:o0,0:i1512x982:c3024x1964:r0`，负 origin
也合法。请保留逗号，不套用 session identity 的更严格校验，不清洗/改写版本格式。
session id 与 error code 的允许字符集未放宽；v1 字段无变化。新增
`actual_host_geometry_versions_roundtrip_without_widening_session_identity`测试代码，待 CC 执行。

## 待协调者 CC + GLM 执行命令

```sh
cd /Volumes/doc/workspace/datagrand/rpa/rpa-computer
export CARGO_TARGET_DIR="$PWD/crates/desktop-feedback/target"
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked
cargo clippy --manifest-path crates/desktop-feedback/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path crates/desktop-feedback/Cargo.toml --all -- --check
```

分组定位（同样未执行）：

```sh
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked --test wire_codec
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked --test channel_control_transport
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked --test state_process
```

Host 集成门禁仍包括 enabled handshake 拒绝 startup、renderer loss 后旧 session 输入无效、
真实 native 释放失败显示 failed/unknown、off 不加载缺失 renderer，以及 Mac/Win 点击穿透、
停止可达、焦点、真实模型 capture 排除、交互桌面、进程与线程回收。
