# rpa-desktop-feedback

独立 workspace 的可选 Rust 基础库，依赖只有 `serde` / `serde_json` 与标准库。
**没有输入、UI、capture、网络或 Host runtime 依赖；没有真实 renderer 启动实现。**

基线：`docs/superpowers/specs/2026-09-30-optional-desktop-feedback-contract.md`
及 `.agents/tasks/desktop-feedback-v1-wire-20260930.md`。后者锁定 v1 字段。
本库不改 root Cargo，也不意味着反馈已接入 Host。

## 公共 API

| 模块 | 入口 | 职责与成功含义 |
|---|---|---|
| `protocol` | `HostMessage::Snapshot(Snapshot)`、`RendererMessage::{Ready,Stop,Heartbeat,Error}` | 精确 snake_case v1 类型；`V1`只能序列化为 1。所有嵌套消息禁止未知/重复字段 |
| `protocol` | `Snapshot::validate()`、`RendererMessage::validate()` | 边界校验，不含正文/key/凭据/截图/应用标题或任意 metadata 字段 |
| `codec` | `decode_host_line` / `decode_renderer_line`、`encode_host` / `encode_renderer` | 解析/编码单条消息；编码结果带 LF；诊断不回显原始输入 |
| `codec` | `NdjsonDecoder::new(limit)`、`feed(bytes, callback)`、`finish()` | 增量有界读行，callback 逐帧借用，无帧列表累积；超限/坏帧后连接永久失败 |
| `channel` | `Publisher::new(max_subscribers)`、`try_publish(snapshot)`、`subscribe()` | 单槽完整最新状态；非阻塞 `try_lock`；已接受 sequence 严格递增 |
| `channel` | `Subscriber::poll_latest()` | 每订阅者只保留序号；跳过中间帧，新订阅者获得最新完整状态 |
| `channel` | `SnapshotCursor::accept(snapshot)` | renderer 展示排序；旧/重复 sequence 返回 false，不影响授权 |
| `transport` | `SnapshotWriter::new(subscriber)`、`pump(writer)` | 一次最多一次 write；最多一个部分写入帧；`Sent(sequence)`只说明写管道成功 |
| `control` | `StopInbox::new(capacity)`、`try_push` / `try_pop` | 独立有界 stop 邮箱，重复待处理 session 请求合并，不与 pointer 队列混用 |
| `control` | `ControlGate::grant`、`authorize_stop`、`revoke_current`、`finish_stop` | 验证当前 session+generation，原子撤销，实际清理确认后才能再 grant |
| `state` | `ProtocolMachine::enabled` / `disabled`、`receive`、`tick`、`failure` | ready/heartbeat/失联状态机；首次失败 O(1) 持续保留，不经易丢消息队列通知 |
| `process` | `Supervisor::start` / `feed` / `receive` / `eof` / `poll` / `transport_failed` | 延迟解析命令、握手门禁、有界 stdout/stop 处理、私有进程监督 |
| `process` | `PrivateSpawner`、`ProcessControl` | Host 专用最小子进程接口，不是通用插件/任务系统；适配器方法须非阻塞 |
| `process` | `RendererCommand::new` / `to_std_command` | 无 shell argv 构造，只允许 accent / label；构造不查文件、不 spawn |
| `process` | `begin_shutdown` / `poll_shutdown` | 根据实际 terminate/reap 结果报告进程退出；不由发送消息成功推断 |

### Wire 校验与资源边界

- wire JSON 的 `type`、`version` 和所有必需字段必须存在；
  `snapshot.session/surface/pointer` 必须显式存在，可为 null；`stop.session` 不可为 null。
- 帧总长度最多 **16,384 bytes（含 LF）**，因此单条 JSON payload 最多
  16,383 bytes。CRLF 可解析，但 CR 占 payload 限额。decoder EOF 时仍有字节，
  即使这些字节已构成 JSON，也返回 `TruncatedLine`，不会执行未结束的 stop。
- `NdjsonDecoder`缓冲容量固定为构造 limit；feed 一个百万字节无换行切片也不会扩容。
  codec 不禁用 serde_json 的递归深度限制。
- generation 和 sequence 是 u64（generation=0 可合法使用）；拒绝负数、浮点代次及溢出。
- session id、surface id：1..128 bytes，ASCII `[A-Za-z0-9_.:-]`。
  surface id 由 Host 按 `os:id`构造（如 `macos:1`、`windows:1`）。
  **surface version 专属校验**：1..128 bytes，ASCII `[A-Za-z0-9_.:,-]`，保留逗号；
  兼容真实 `Geometry.version`：`d1:o0,0:i1512x982:c3024x1964:r0`及负 origin。
  renderer 把它当 opaque 版本串，不按 session identity 的字符集拒绝/清洗。
  error code：1..64 bytes，沿用严格身份 token 字符集。不能填用户信息。
  这是基础库的有限字段校验约束；平台 renderer 不需新增 wire 字段。
- geometry / pointer 坐标有限；surface width/height > 0，右/下边界不能浮点溢出。
  pointer 非 null 时必须有 surface；不把原生坐标当截图像素，也不强行限制 pointer 在某屏内。
- subscriber 数 1..64；stop capacity 1..64，supervisor 默认 16。
- 内部存储：共享 1 个 snapshot、每 subscriber 1 个 sequence；每
  `SnapshotWriter` 1 个≤16KiB帧；每 decoder 1 个≤16KiB行缓冲；stop 队列最多 capacity 个
  ≤128-byte session token。单 subscriber 的标准配置是 O(1) 存储，与 pointer 事件数无关。
- 返回的 `Arc<Snapshot>`、输入切片、`PublishError.snapshot` 属于调用方；调用方不得另建无限历史列表。
- 无共享锁跨 I/O；输入侧发布只 `try_lock`。`Contended` 返回调用方完整 snapshot，
  Host 在自己的单槽待重试状态中保留**最新**权威状态，交给 worker/下一调度重试，不能忙等。
  被拒绝的 publish 没有被接受或确认，不能假称送达。`PublishError.snapshot`为 Box，
  用 `*error.snapshot`取回原状态。中间 pointer 可直接丢帧。
- stop push 的 overflow/contended/poison 不能默默丢弃：`Supervisor`转换为 sticky
  stop-required。直接用 `StopInbox`的集成者必须把该错误交给监督器/可信 Host 失联路径。

## Host 接线和顺序（必须遵守）

### 1. Off 与 startup

1. 配置 off 时，调用 `Supervisor::start(Disabled, ..., resolver_closure, spawner)`。
   Disabled 在 resolver、配置有效性、decoder/stop 分配、spawn 之前直接返回；
   `feed/eof/poll/Drop`也不会触碰 renderer。**调用方不要提前在参数求值时探测文件/构造 GUI。**
   完全不加载本库也是合法路径。
2. enabled 的 resolver 只在 enabled 分支解析被控 Host 的 renderer 路径，构造
   `RendererCommand`；命令构造不会查文件。实际 spawner 负责文件可用性和私有子进程启动。
3. spawner 必须是被控 Host 交互桌面内的子进程，stdin/stdout 私有管道，stdout 仅协议。
   Windows Session0 不能视为成功。`to_std_command`把 stderr 指向 null；若需要诊断，
   适配器必须独立有界排空 stderr，不继承/转发用户内容。
4. `start(Enabled)`的 Ok **只代表 awaiting_ready**。Host 对外服务 startup / input grant
   必须等待 `machine().can_accept_control()==true`，而不是等待一次 snapshot 写入。
   renderer 只能在真实平台初始化/窗口成功后发 ready；纯测试中的构造数据不代表真实 ready。
5. ready deadline 默认 5s；握手失败 `Failure.startup_rejected=true`，显式开启的 Host 启动失败。
   不回退 off、不报告可见。`capture_exclusion=requested`只是 API 设置成功，不是截图验证通过。

### 2. 权威生命周期、snapshot、pointer 的序号

- 一个 Host 生命周期 owner 串行处理 grant、observe/action completion、stop 和 cleanup 结果。
  维护当前真实 state，完整构造 snapshot，并为这条 publisher/renderer 流分配递增 sequence；
  sequence 只在 owner 分配，不允许多个 pointer worker 从各自旧状态克隆后分配新序号。
- 实际派发完成的 pointer 记录必须带 Host 内部 authority epoch、session+generation、
  surface id/version。owner 先匹配当前控制权和 geometry，再把 pointer 合并到**当前**完整状态；
  旧代次、旧 surface、stop 后迟到的 pointer 记录丢弃。不得根据 Agent 计划或文字发布 executing。
  这些内部关联字段不新增到 v1 wire，library 没有提供无版本保护的 pointer delta API。
- session / geometry / stopping/closed 等权威切换时清空旧 pointer，重新发完整 snapshot。
  latest 槽覆盖旧帧，序号跳跃正常；达到 u64::MAX 不 wrap，在可信生命周期中重建流。
- `try_publish`仅合并完整最新 state，不验证 Runtime 事实。其失败不能撤销已经生效的 stop/cleanup；
  最新权威状态在 owner/worker 的单槽中重试，不能反过来延迟输入取消。
- 未完成帧绝不能被新 snapshot 截断。`SnapshotWriter`先完成旧帧再取最新；退出旧 renderer 后
  新私有流使用新 writer/subscriber/cursor，不能把旧流部分 JSON 接到新流。
- `pump`需要真正非阻塞的 pipe Write 或独立 I/O worker，不能在输入线程/UI线程调用，
  不能把它包在与输入共用的外部 mutex 里。`WouldBlock/Interrupted`返回 Pending，不忙等。
  已写入的旧帧无法撤回，展示可能暂时落后；授权永远以 Host 当前 gate 为准。

### 3. stop race 与取消清理

1. renderer reader 用 `Supervisor::feed`或已严格 decode 的 `receive`，ready 前 stop 被拒绝。
   ready 后 stop 只进入 bounded raw-request inbox，**不会在读帧时提前授权**。
2. Host 生命周期 owner 在消费时调用 `ControlGate::authorize_stop(request_session)`。
   `sequence`不参与授权；当前 session 的 id 或 generation 任一不符均拒绝。
   `authorize_stop`在同一次调用内把 current 置空、建立 pending `StopToken`。
3. owner 在撤销后立即使 Host 真实旧控制权失效（拒绝新输入）、触发执行取消与 native cleanup。
   gate 是辅助状态，不会替你改变现有 Host authentication/session Runtime。
   随后 publish stopping/pending/session=null，而不是等待 renderer ACK 才取消。
4. cleanup 的真实结果决定 released/failed/unknown 的 snapshot。
   `finish_stop(token, Released|NotNeeded)`才解除 pending；失败/未知/pending 不允许再 grant。
   grant 也不允许静默替换仍存活的 current。重新授予遵从 Host 原有认证与生命周期；
   Host 负责 session id 不重用，或维持跨 session 的 generation 发行记录，本库不建无界历史表。
5. 旧请求 A 已排队、owner 已撤销清理 A 并 grant B 时，消费 A 得到 WrongSession，B 不受影响。
   A 被授权后 pending 没有完成时不能 grant B，因此没有“延迟取消 A 却取消 B”的交叉窗口。
   `StopToken`的私有 epoch 只用于可信 Host cleanup 关联，不能作为 renderer 或远程认证 token。
6. `Snapshot::safe_completion()`仅是展示辅助：closed+failed/unknown/pending 是 false；
   NotNeeded/Released 也必须来自真实输入清理结果。发送、EOF、进程终止均不构成该证明。

### 4. renderer loss 与 teardown

- Host 必须定时 `poll(monotonic_elapsed)`，默认 heartbeat timeout 5s（renderer 每秒 heartbeat）。
  只 feed 不 poll 无法检测沉默 child。heartbeat 只由 heartbeat 更新，stop/snapshot 不续命。
  在 deadline 边界及之后到达的 ready/heartbeat 都先判 timeout，不能复活失败流；倒退时钟失败。
- duplicate ready、坏版本/帧、EOF、child exit、read/write loss、control overflow、timeout
  都产生 sticky `Failure.stop_required=true`；Host 立即停止授予控制并对当前 gate 执行
  `revoke_current()`，使真实控制权失效、取消和清理，即使 inbox 已满或没有任何 snapshot。
- `failure()`始终可重读，不存在“通知排队失败就忽略失联”的路径。故障恢复须新监督器、
  新私有流和真正 ready；不会自动恢复输入控制。
- 主动 Host teardown 先安排上述 input stop，随后 `begin_shutdown` / `poll_shutdown`驱动
  child terminate/reap；`ShutdownStatus.reaped`只有适配器确认实际 reap 才 true。
  terminate/reap 错误单独返回，可重试；不发送虚构“quit成功/已清理”消息。
- Drop 只做非阻塞 best-effort terminate，不能保证 reap。Host 必须另设有限 teardown deadline，
  回收子进程/线程/管道并准确报告失败。本库不假称执行了平台 cleanup。

## 编译与后续验证分工

Codex 当前只实现与编译。测试源码先于相应实现编写；**未运行 RED/GREEN、测试、clippy 或验收**。
开发编译命令（不会执行测试）：

```sh
cd /Volumes/doc/workspace/datagrand/rpa/rpa-computer
CARGO_TARGET_DIR="$PWD/crates/desktop-feedback/target" \
  cargo check --manifest-path crates/desktop-feedback/Cargo.toml --all-targets --locked
```

以下命令由协调者调度真实 Claude Code + GLM 执行，本任务未执行：

```sh
cd /Volumes/doc/workspace/datagrand/rpa/rpa-computer
export CARGO_TARGET_DIR="$PWD/crates/desktop-feedback/target"
cargo test --manifest-path crates/desktop-feedback/Cargo.toml --locked
cargo clippy --manifest-path crates/desktop-feedback/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path crates/desktop-feedback/Cargo.toml --all -- --check
```

测试为纯 Rust/fake child/fake stream，不调用真 UI/spawn/SSH。
可分别执行 `--test wire_codec`、`--test channel_control_transport`、`--test state_process`定位。
真实 GUI 点击/焦点/穿透/capture 排除、远程 Host 交互桌面、native cancellation/held-input
清理及 off 模式原输入回归仍需后续 Host 接线后，由 CC + GLM 验证。独立库测试不能替代。
