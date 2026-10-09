# Desktop feedback 基础库 CC 独立验证报告

日期：2026-09-30。角色：CC 只审查/执行测试，未写/改任何源码、测试或脚本。本报告结论仅覆盖
`crates/desktop-feedback` 纯 Rust 库；Host 接线未实施，不代表实际子进程、GUI 或截图排除已验证。

## 1. 执行结果（实际运行，日志保留）

| 命令 | 结果 | 日志 |
|---|---|---|
| `cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked`（CARGO_TARGET_DIR=crate target） | **exit 0**，33 passed / 0 failed | `.agents/runs/desktop-feedback-core-cc-test-20260930.log` |
| `cargo clippy ... --all-targets --locked -- -D warnings` | **exit 0**，无警告 | `.agents/runs/desktop-feedback-core-cc-clippy-20260930.log` |
| `cargo fmt --all -- --check` | **exit 0**，无 diff，未改任何文件 | `.agents/runs/desktop-feedback-core-cc-fmt-20260930.log` |

实际执行 target 与计数（与实现者报告的静态 33 一致，本次为真实执行数）：
- unittests src/lib.rs（channel 内嵌测试）：2 passed
- tests/wire_codec.rs：9 passed
- tests/channel_control_transport.rs：8 passed
- tests/state_process.rs：14 passed
- doc-tests：0（空）

## 2. 规格独立检查（基于实际代码，非测试通过推断）

- **严格 v1**：`V1` 自定义 Deserialize 仅接受整数 1（protocol.rs:14-23）；`deny_unknown_fields` 覆盖全部结构体与 tagged enum；serde derive 对重复字段报错。wire 字段与任务 spec 的 snake_case 枚举一一对应。
- **16KiB 有界 wire**：`MAX_FRAME_BYTES=16KiB` 含 LF；编码侧 `LimitedWriter` 按行上限拒绝超帧；解码侧 `NdjsonDecoder` 固定容量缓冲，逐字节检查，达 limit 即 `LineTooLong` 终止；EOF 无 LF 拒绝（`finish` → `TruncatedLine`）；诊断仅枚举，不回显内容。
- **latest snapshot**：`channel::Publisher` 单槽 `Option<Arc<Snapshot>>`，无事件队列；严格递增序号（含 u64::MAX 耗尽检查）；订阅上限 1..64；订阅即从当前完整状态重同步；全部 `try_lock`，竞争返回原 snapshot 不阻塞。
- **session/generation 授权**：`ControlGate::authorize_stop` 按当前 session 完全匹配，`grant` 拒绝同 id 代次不增长；授权与失效在同一次调用完成；`finish_stop` 仅 Released/NotNeeded 解锁 pending；pending 期间拒绝新 grant；stop token 含 epoch，无法从公共 API 伪造。
- **ready/heartbeat sticky 故障**：`ProtocolMachine` 默认 5s deadline；重复 ready、renderer error、时钟倒退、超时均收敛为 sticky `Failure`（stop_required），`tick` 先于 `receive`，晚到消息不能复活；`can_accept_control` 仅 Disabled/Ready。
- **off 不解析路径/不 spawn**：`Supervisor::start` 在 `mode == Disabled` 时于校验 timeouts、调用 `resolve_command`、分配 decoder/stop 队列、spawn 之前直接返回（process.rs:141-150）；有对应测试 `off_does_not_resolve_validate_spawn_read_or_poll_renderer`（真实执行通过）。
- **无输入/UI 依赖**：crate 无 GUI/输入依赖，直接依赖仅 serde/serde_json（Cargo.toml/lock 一致，`--locked` 通过），`#![forbid(unsafe_code)]`。
- **surface.version 逗号**：`validate_surface_version` 允许 1..128 ASCII `[A-Za-z0-9_.:,-]`（protocol.rs:216-220），真实 `d1:o0,0:i1512x982:c3024x1964:r0` 及负 origin 通过（有 roundtrip 测试）；Session.id 未放宽逗号。

## 3. 质量独立检查（具体证据）

- **锁竞争**：`Publisher::try_publish`/`Subscriber::poll_latest`/`StopInbox` 全部 `try_lock`；测试用外部持有锁验证 `Contended` 且序号/游标不前进（channel.rs:161-193）。
- **部分 write**：`SnapshotWriter::pump_once` 每次最多一次 `Write::write`，`offset` 跟踪部分帧，`WouldBlock/Interrupted` 返回 `Pending` 不丢帧；旧部分帧不被新 snapshot 截断（新帧只在旧帧完成后才取）；`Ok(0)`/超量写均判 `TransportLost`。无跨流残留（新 Subscriber/Writer 全新状态）。
- **序号**：发布侧拒绝回退；`SnapshotCursor` 渲染侧同样拒绝回退，无重置于活流路径。
- **过期 stop**：排队中的旧 stop 在 dequeue 后由 Host 按 `authorize_stop` 当前 session 重新验证（WrongSession/NoControl 拒绝），不预授权；`revoke_current` 幂等返回既有 pending token。
- **清理 token**：token 绑定 session+epoch；`finish_stop` 用完整 token 等值比较，错误 token 拒绝；Failed/Unknown/Pending 不解锁，可重试。
- **shutdown**：`begin_shutdown`/`poll_shutdown` 区分 terminate 请求、reaped 与失败（`TerminateFailed`/`ReapFailed`）；无进程时直接 reaped=true；Drop 仅 best-effort terminate，不声称回收。
- **overflow/poison**：stop 队列溢出经 `machine.fail` 转 sticky stop-required，不静默丢弃；Mutex poisoned 映射 `TransportLost`。

## 4. 限制与不宣称事项

- 所有测试基于抽象 `ProcessControl`/`PrivateSpawner` fake 与纯状态机，**不能**等同于实际子进程 spawn/reap、私有管道行为或 GUI 已验证。
- `capture_exclusion: Requested` 仅 API 设置成功的 ACK，测试通过不代表实机截图排除验证。
- Host 责任（周期 poll、单调时钟、ready 前拒 grant、非阻塞管道适配器、teardown deadline）仍未实施，本库无法强制。
- 次要观察（非失败，不自行修改）：`codec::json_error` 以错误串包含固定 marker 判定 `UnsupportedVersion`；攻击者可用含该 marker 文本的未知字段名使错误被归类为 UnsupportedVersion 而非 InvalidJson——两者均为终止性诊断、无内容回显，仅分类偏差。另：`src/` 目录残留 5 个非源码文件（windows-vision-probe*.log/png、xwin-env.log），不属于本 crate 交付，建议协调者决定处理。

## 5. 结论

33/33 真实执行通过，clippy `-D warnings` 与 fmt check 均通过（exit 0）。规格与质量契约在纯库层面成立。未发现需回报实现者修复的阻塞性缺陷；上述次要观察交协调者裁量。Host 安全门禁（真实 renderer、输入释放、截图排除）仍待接线任务。
