# Windows root PhaseC race repair — SOURCE_FROZEN

日期：2026-09-30。Owner：Codex；仅代码/测试源码与 Windows compile/link-only，实际测试由 CC 执行。

## 交接状态

**SOURCE_FROZEN；两项失败的修复后运行结果待 CC，未宣称 GREEN。** 本轮仅修改2个指定测试及专用test helper，没有改产品代码、Worker API、validator、generation token、原6RED/多屏3feedback/token tests或任何独立owner文件。

已生成并保留完整 **9个 Windows test exe**，两条 `cargo test --no-run` 均 exit0。没有执行任何测试exe、应用CLI、Host、SSH、GUI、截图/输入、CC、Mac/Linux测试；不 commit/push/worktree/reset。所有旧失败、报告、manifest、target、release/fixed exe 未覆盖或删除。

原历史证据保持：CC token retest PhaseA31/PhaseB27各自native0；PhaseC **273pass/2fail/6ignored，305.12s，native101**；PhaseD未跑。A/B是不同binary的执行计数，不在这里推导跨阶段unique数量；PhaseA部分由PhaseC再次执行。本轮9exe不能借用历史GREEN。

## RCA：先证明原测试落点，再修改同步

依据 `.agents/CONTRACT.md`（含最新amendments）、`.agents/reports/windows-multidisplay-token-retest-cc-20260930.md` 及 raw `.agents/runs/windows-multidisplay-token-retest-cc-20260930/evidence/phaseC.output`。旧代码快照保存在本轮 runs 的 `src__...before`，本轮精确增量为 `repair.diff`；共享git diff含更早任务，不能代替本轮delta。

### 1. stop_racing_resume_geometry_cannot_restore_authority_or_clear_cancel

raw在旧 `src/feedback/tests_runtime.rs:222` 主线程 `feedback.snapshot().session.unwrap()` panic None，未证明resume执行到了预期竞态点。

静态确定的调用路径：

1. `Runtime::open` 调用 `FactBackend::select_display` → `HookBackend` 未重载的 `Backend::select_display(Primary)` → `HookBackend::geometry()`：第1次。
2. `display::check` 经 FactBackend 转发 `HookBackend` 默认 display_snapshot，返回None，不授予feedback session。
3. `Runtime::open` 为防open期间拓扑变化，再调用 backend.geometry → `HookBackend::geometry()`：第2次。
4. **只有上述检查成功后**，open才构造Session并调用 feedback.grant。
5. 旧测试把 `geometry_barrier_at=2` 作为resume入口，在第3步就阻塞worker并唤醒主线程。因此session仍None；panic也会令旧两阶段 `Barrier::wait` 缺少第二个参与者，有遗留等待线程风险。

修复为语义边界：main线程创建Runtime → **open成功并取得id → pause成功且反馈Phase::Paused、已授予session验证 → arm gate** → 调用resume；helper只有实际进入 HookBackend.geometry checkpoint 后才执行Stop。未把2改3，不依赖任何调用次数。其他原测试仍使用其已有count/barrier；本轮不扩写其他测试。

断言保留并增强：resume必须cancelled；cancel仍true；feedback永久terminated；后续open和resume均必须cancelled且不能清flag；真实Runtime shutdown成功后snapshot.session为空。

#### 原 control() callback 的附加测试缺口

旧测试 `super::tests::control()` 创建的 callback **只递增计数器，并不设置这个Runtime使用的cancel Arc**。这不是此次None的直接原因（None由open前gate导致），也不是已证明的产品缺陷；但它不模拟真实Stop送达输入取消flag的连接，旧末尾cancel断言依赖Runtime返回时的永久撤销检查重新锁存。

新测试为本测试单独创建 `FeedbackHandle`，callback设置与Runtime相同的 `Arc<AtomicBool>`；helper在Stop后立即断言该flag为true。没有修改共享control()或产品回调。随后仍验证resume不能清除它，而不是靠test cleanup补成true。

### 2. cancellation_during_hold_stops_promptly

raw在旧 `src/runtime/session/tests.rs:416` 得到NotStarted而预期Partial；前面的rec.cancelled断言已通过。证据证明取消在首个成功dispatch之前发生，不证明按键已经held。

静态路径：`execute_step` → `validate_step` → `display::check` → `FakeBackend::display_snapshot` → geometry + **decode_png(1600×900)**；真正输入之前，`display_input::execute` 的逐事件guard还会再查询snapshot/解码。旧helper从调用step前独立sleep120ms开始计时，无法保证CPU/解码完成，也无法保证Key Press已派发。取消若提前送达，生产executor返回NotStarted正是诚实结果。raw没有逐次解码时间戳，不在这里捏造“恰好是哪次PNG解码超过120ms”。

新测试在FakeBackend完成key校验、记录Press并更新模拟held-state之后进入one-shot gate，helper此时才设置cancel。由被测executor实际调用mock inject触发，而非测试直接插入记录或提前置flag。保持5000ms请求hold，不新增调度sleep，不接受NotStarted。

promptness计时从helper执行cancel.store紧邻之前的Instant开始，到step返回立即记录的Instant结束；不把observe/PNG/preflight或helper join时间算进取消响应。保留 **cancelled / Partial / CleanupOutcome::Released / <2s**，新增首事件shift Press、恰好一次Press、恰好一次release_all、held_keys为空断言。held-state仅FakeBackend模拟，不是真实OS按键证据。

### 产品结论边界

静态调用路径与raw支持这两处测试同步前提失效，目前未发现需要修改生产逻辑的具体证据，所以本轮不改产品。不是宣称产品无竞态：新测试若在正确语义落点仍失败，应保留RED再分析生产逻辑，不能削断言。最终结论需CC真实运行。

## 写集（4文件 + 本报告）

1. `src/feedback/tests_runtime.rs`：只替换目标竞态测试、HookBackend增加可选geometry gate/default None；保留其他测试语义。
2. `src/runtime/session/tests.rs`：只替换目标hold-cancel测试（格式化可能影响邻近空白，无其他断言修改）。
3. `src/runtime/testutil.rs`：注册race helper；FakeBackend增加默认None的key-press gate、模拟held_keys bookkeeping；仅成功release清空，失败保持held。
4. **NEW** `src/runtime/testutil/race.rs`：共享但专用的test-only语义gate/RAII helper。

`runtime::testutil`原本受cfg(test)控制，本轮不改变这一点。没有新增test函数，原两个完整filter名字不变。原6RED/多屏3feedback/token tests及其他root源码按before.json逐字核对未变；Cargo/lock/冻结库/renderer/fixture/focus/cancel9/runner均未改。

## 有界等待 / RAII 不掩盖真实断言

- RaceGate初始未arm，setup查询直接通过。RaceTask在成功setup后arm，helper等实际checkpoint进入；两端Condvar等待各有10s上限，timeout是测试失败，不自动成功。
- helper执行的Stop/cancel包在catch_unwind内，失败时设置cancel并release gate，再重新抛出panic；失败不会悄悄吞掉。
- RaceTask主线程异常Drop也设置cancel/release gate，bounded wait线程完成后才join；`join`只对is_finished=true的handle调用。
- 正常 `finish()` 取走并join已完成handle后，Drop不再设置cancel、不再release。正常helper仅release gate，不额外补cancel。**成功路径不能替生产代码重锁cancel，不能mask resume清flag的缺陷。**
- finish等待最多10s；异常Drop额外cleanup等待最多10s。若线程仍未终止，打印明确test-harness残留诊断并detach，不做无限join、不谎称已回收；此时对应finish已失败或测试正在unwind。没有OS线程强杀。正常路径必须成功join。
- 1ms park_timeout仅用于bounded teardown的is_finished轮询，不用于决定Stop/cancel派发时间。没有提高原120ms sleep；已完全移除该调度假设。
- mock checkpoint位于已接受/记录Press但inject尚未return的边界，取消后仍由生产executor记录成功事件和运行release_all。此测试证明“press已接受后取消”的合同，不单独证明恰在sleep函数内部唤醒；不伪装为真实桌面持键验收。

## 编译与空间预算

编译前df：`/Volumes/doc`余3,281,872 KiB（约3.1GiB），`/tmp`所在Data卷余29,943,048 KiB（约29GiB）。上一轮target约2.0GiB。因此本轮新target放到本机独占目录，**没有清理旧runs**。

**保留的绝对target路径：**

```text
/private/tmp/windows-root-race-repair-sol-20260930.gxjBQq/target
```

编译后target约1.5GiB；Data卷余28,522,712 KiB，/Volumes/doc余3,281,668 KiB。见 `disk-before.txt` / `disk-after.txt` / `target-size.txt`。

加载既有CC build-env后立即覆盖NEW CARGO_TARGET_DIR，设置 `CARGO_INCREMENTAL=0` 限制空间。编译命令：

```sh
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR=/private/tmp/windows-root-race-repair-sol-20260930.gxjBQq/target
export CARGO_INCREMENTAL=0
cargo test --target x86_64-pc-windows-msvc --locked --offline --no-run   --lib --test protocol --test runtime_contract --test transport_lifecycle   --test windows_cancel_contract --message-format=json
cargo test --manifest-path crates/display-topology/Cargo.toml   --target x86_64-pc-windows-msvc --locked --offline --no-run --message-format=json
```

| 编译（不执行） | 原始JSON / stderr / 数字退出码 | 结果 |
|---|---|---|
| root lib+4 integration | `root-artifacts.jsonl` / `root-build.log` / `root-build.exit` | exit0，20.71s，无warning |
| topology 4 targets | `topology-artifacts.jsonl` / `topology-build.log` / `topology-build.exit` | exit0，0.86s，无warning |
| 限定本人文件diff-check | `diff-check.log` / `diff-check.exit` | exit0 |

上述文件均在 `.agents/runs/windows-root-race-repair-sol-20260930/`。仅明确4文件rustfmt且skip_children=true。no-run表示编译/链接，不是测试通过；实现者没有运行这些9exe。

## 精确9exe（保留给CC直接使用，不重复重编）

路径统一位于上述target的 `x86_64-pc-windows-msvc/debug/deps/`。`artifacts.json` 存每个绝对路径、大小、SHA256、来源compiler JSON、target kind及fresh=false；`artifacts.sha256` 可直接核对绝对文件路径。全部来自本次JSON的 `compiler-artifact` / `profile.test=true` / `executable`，不是glob拾取旧文件。

| target | exe文件名 | SHA256 |
|---|---|---|
| `capture` | `capture-b4eca34d8542019e.exe` | `fbd0ba31d3e323cca86953c032cc35c88cf7bf1c72352c0df3d1b901fc8e9e16` |
| `mapping_drag` | `mapping_drag-9e348d69e6347be5.exe` | `9dc11c9c93d9aa82415f9691871ca1a7c1cf80b8f0f368da4b948efe5d003f8c` |
| `protocol` | `protocol-414dd87c6433781f.exe` | `fa82a4d230e69e8931f9d68118953214032de659489ee81920530086aeb6c3cb` |
| `rpa_computer` | `rpa_computer-d743cec7e10c1ea5.exe` | `22d6fa768cef9842a00a4fec1890476673f2b921025b82c636a7e12e89d913fa` |
| `rpa_display_topology` | `rpa_display_topology-960dad6d015a6a21.exe` | `9405258d3bc7d800056194a051e2655973f692bfe8cd2381d3a151564ff95ea1` |
| `runtime_contract` | `runtime_contract-f70972752ebd4711.exe` | `c38b3dfa3946cfbf30cc6ef7ebbd2ff7631319d29b73aff4f950ac1c07a3e6c4` |
| `topology` | `topology-447b3fcf95852ce6.exe` | `485ad9b5ca6ae82c83ce6dc554cd259d73910c410b1d2ea88f7f18e3797db665` |
| `transport_lifecycle` | `transport_lifecycle-8d087cfcff46f1ce.exe` | `5e94685d3a26f11cf68599a466383be8adc675dcafedc75eb9d1ce6a27b65680` |
| `windows_cancel_contract` | `windows_cancel_contract-1a27a1187942ba22.exe` | `64111f9ef2cfe3c2ea228c6f9e705e601ddc2a05d3257165a2741f581ffc9772` |

## CC执行清单（由协调者启动，实现者不执行）

已读取 `.agents/tasks/windows-root-race-retest-cc-20260930.md`，以其具体runner/timeout/首败停止规则为准。无需重编，用上面9个frozen exe；全部复制到新Windows目录并核对双端SHA。

1. rootlib两项原失败分别 `--exact --test-threads=1 --nocapture`：
   - `feedback::tests_runtime::stop_racing_resume_geometry_cannot_restore_authority_or_clear_cancel`
   - `runtime::session::tests::cancellation_during_hold_stops_promptly`
2. 相关 `feedback::tests_runtime::` / `runtime::session::tests::`；原PhaseA31 filters与四个topology exe默认suite。
3. rootlib全默认suite，保留6ignored，不加ignored选项；旧305秒不是新结果，等待同一runner完成，不因观察窗口短重开。
4. **全部四个** integration：protocol、runtime_contract、transport_lifecycle、windows_cancel_contract（包括取消9项重新验证）。不执行remote_transport、真实网络/桌面。
5. 保存每次stdout/stderr、native numeric exit、timeout/drain/identity字段；首败保留并停止后续广域阶段。执行前后核对源码和9exe manifest。新的pass/fail单独记录，不覆盖旧273/2/6。

## 冻结索引与风险

- `source-freeze.sha256`：177条当前root src、tests/资产及本地相关Rust依赖源清单。SHA256：`980c8eb67aceb3489687d979e66df6a9ca27c554f6bbaef09f93c5fc86660d8f`
- `changed-source.sha256`：仅本轮4文件。SHA256：`2a2ead7d5e5f2a5faf195804f5eb6632a0e1f86ad7ccb4ace45ea2ddf1ca4d1b`
- `compile-evidence.sha256`：本轮两次编译JSON/log/exit及target/env记录。SHA256：`5a0566329c4231b56dc9170f16b10da17123df49fe3e6c95ce76097751a9c390`
- **`artifacts.json` SHA256**：`c6e119995f51624450454846b05a15bee1710e9d291799f7a30d21ac35d41447`
- **`artifacts.sha256` SHA256**：`e0ab8181d1e6c158b6f97b55993a956f08054feadf6273b76daff1a232d7f46b`

新manifest在本轮runs，不覆盖token/PhaseC旧manifest。报告自身hash另存 `report.sha256`。源码和所有9exe已只读hash复核；旧报告/raw不作为新运行结果。源文件4项变更外，before.json中的源/测试/Cargo均核对未变。

剩余风险：helper仍依赖OS线程最终能被调度；10s harness超时与2s取消响应断言会诚实暴露严重调度/生产问题，不保证任意过载机器一定通过。FakeBackend新增held bookkeeping为测试模型，不是物理按键证明。新测试/9exe尚未执行；GUI、物理多屏、真实feedback Stop、截图排除等不在验收范围。/private/tmp产物保留，不由实现者清理；CC应按精确绝对路径取用，不能换旧exe。

**停止源码写入，等待独立CC静态review与Windows运行证据。**
