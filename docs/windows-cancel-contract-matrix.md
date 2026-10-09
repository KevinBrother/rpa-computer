# Windows cancellation contract — 独立生产 Worker 测试矩阵

日期：2026-09-30。状态：**原9项纯契约已在修复后Windows产物上通过；独立目标 windows_stdio_eof 的6项真实管道测试通过（4项stdin EOF + 2项stdout断开，首败及修后证据见下文）；另有windows_tcp_disconnect的4项真实回环TCP/TLS断连测试通过；真实GUI/物理按键释放仍 deferred。**

本目标 `windows_cancel_contract` 与 root 的 lib/tests 冻结独立，不要求 root 编译此 target。仅 Windows `cfg(target_os="windows")`，不能用非 Windows 的零测试结果充当验收。共 **9 个非 ignored 的测试**，覆盖 cancel01..06、08..10；cancel07不在这9项之内，现由独立 `windows_tcp_disconnect` 的4项真实回环测试补充（见文末）；没有空测试或假通过占位。

## 边界与设计

- 真实 `Worker::start(BackendFactory::Test)` 创建生产 coordinator/native 线程；真实 Runtime 解析、规划、timing、stop generation、request ledger 和 cleanup 处理输入。
- 内存 Backend 仅实现实际 trait、提供固定 16×16 PNG/geometry、记录 `InputEvent` 和自有 synthetic held state。没有 `DesktopBackend`、native injection、窗口、OS screen capture、socket、Host、外部进程、registry 或全局 hooks。反馈默认禁用，可交 CC 在 Windows Session 0 执行纯测试；没有扩展 GUI 授权。
- `inject` 先记录真实到达 Backend 的事件、施加 synthetic press，再发 entered channel 并阻塞。监督者收到 barrier 后才调用生产 pause/close、ingress 通知、CancelHandle 或 McpService EOF。Backend 不设置 cancel，不因 barrier 自行清 held。
- pause/close 在另一条监督线程执行：主测试线程用生产 `CancelHandle::is_cancelled()` 等待已生效的 stop（deadline + yield，无 sleep 猜时序），确认前 native dispatch 保持 blocked。由此不能把排在 action 后的控制误认为中途取消。
- 全部 channel 等待 `recv_timeout(8s)`；Backend 自身有 8s hard bound，超时返回失败且 oracle 检查 `gate_timeout`，绝不把 timeout 当成功取消。release guard / Harness Drop 在 panic 时无条件发 release。Job 先有界收到结果才 join；timeout 时不无条件 join，丢弃 JoinHandle；假 Backend 可自行退阻塞。生产调用另有 8s reply deadline 和 Worker bounded shutdown。CC 应再设整个目标 **120s** 外部 watchdog，不能宣称实现进程后代 containment。
- barrier 前后 prefix 不变；已取消后新增 dispatch 只能是 Key/Button release，禁止业务 Move/Text/Press；成功 cleanup held 必须空，cancel08 cleanup 失败仍保留 synthetic held。精确 partial/completed/cancelled/error/cleanup 与 state/count 都断言，不接受一组宽泛 error codes。
- drag 使用 16ms、2 个生产插值 move（固定路径 `[0,0]→[8,8]→[15,15]`）。短 duration 不用来判定取消时机：barrier 保证中途停住；cancel02 第一个 press 后的 move 必须确为 `[8,8]`。key_hold 是 1000ms，文本为 896 Unicode scalars，首个 actual Text dispatch 必须是 `汉`。
- action 经初始化的真实 McpService `handle_line` 进入生产 sanitize/stamp/register/direct-cancel/service；wire request id `101`、runtime request id `partial` 分开。响应按 `type=text` 寻找 JSON，不依赖图片 block 顺序。生产库不使用 `cfg(test)` 内部 cancel-after-inject seam。

## 精确测试清单

| case | 完整测试名 | barrier / stop | 核心 oracle |
|---|---|---|---|
| cancel01 | `cancel01_drag_pressed_then_pause` | Move+Button press 完成记录 / 生产 pause | completed `[0,1]`，partial/cancelled/released，paused，held 空 |
| cancel02 | `cancel02_drag_moved_then_close` | 首个 held move `[8,8]` / 生产 close | completed `[0,1,2]`，partial/released，closed，无后续业务 move |
| cancel03 | `cancel03_key_hold_pressed_then_pause` | Shift press / 生产 pause | completed `[0]`，partial/released，paused，无额外 press |
| cancel04 | `cancel04_chord_modifier_ingress_cancel_with_negative_ids` | Ctrl press / 真实通知 ingress 匹配 wire id | wrong id、字符串 `"101"` 不匹配数字 101、缺 id、null id 不 stop/不 bump epoch/不 dispatch；匹配 active id stop；晚到 id NoMatch、不 bump generation；不注入 Shift/X |
| cancel05 | `cancel05_text_first_scalar_then_cancel` | actual Text `汉` / 生产 CancelHandle | exactly one scalar，completed `[0]`，partial/released，release_all 恰 1 次，没有剩余文本 |
| cancel06 | `cancel06_service_eof_during_action_then_production_shutdown` | drag press / `McpService::handle_eof` | Stop+空帧，取消 partial 后生产 Worker shutdown Clean，cleanup 共 2 次；重复 shutdown 不增加 cleanup |
| cancel07 | **独立windows_tcp_disconnect 4/4通过** | 真实Windows loopback TCP/TLS → 生产remote/stdio → Test Backend | 原9项suite仍不创建socket；新覆盖为idle/raw active/TLS active/不完整JSON尾帧及各自重连。OS释放、LAN授权、完整Host main和GUI仍待 |
| cancel08 | `cancel08_failed_release_faults_session_and_quarantines_shutdown` | drag press / CancelHandle；fake release_all 失败 | partial/failed、精确 `input_error`；held 不空；后续 step `session_state`；close faulted/failed；shutdown Unknown，后续 `worker_faulted`，重复 shutdown 不恢复或重试；cleanup 计数 1→2→3→3 |
| cancel09 | `cancel09_repeated_close_no_duplicate_cleanup` | held move / 生产 close | 初次 action cleanup+close cleanup 共 2；后续 3 次 close closed/not_needed，计数不变 |
| cancel10 | `cancel10_resume_requires_fresh_observation_without_replay` | Shift press / pause→resume | ready + requires_fresh_observation；显式重放旧 action 只返回原 partial ledger，不再次输入；旧 based_on 新 id stale/not_started；fresh observe 恢复；新 Text `z` 只一次，重复 fresh id 不再次输入 |

以上为**源码测试项数，不是通过项数**。cancel08 fake release_all 的 `input_failed` 映射到协议 `input_error`，不是未经核实的 `input_failed` 协议码。成功取消的 step 精确 `cancelled`，而失败 cleanup 精确 `input_error`；不抹去 failed cleanup。

## 明确限制 / deferred

1. cancel06 是实际生产 McpService EOF handler + transport owner 应做的 Worker shutdown continuation；不模拟取消标志冒称 EOF。但**没有真实 stdio reader/file pipe EOF，也没有 socket EOF**。end-to-end reader/网络断连在真实 transport suite 独立验证。
2. wrong/invalid/late request cancellation 在 cancel04 已实现。没有 session 参数的 `notifications/cancelled` 无法测试 wrong-session notification；当前生产 pause/close ingress 会先 stop 再 runtime 验 session，不把 wrong-session pause/close 当“必然无 stop”的虚构接口。未新增这类保证。
3. held-state 是 synthetic Backend oracle，不证明 Windows SendInput 物理释放；cancel01..06、09..10 真实 GUI 仍 deferred。cancel08 永远只用 fake 故障，不在用户桌面故意遗留输入。
4. compile/link 成功不证明测试成功，不称 allcancel 完成。CC first-red 要保留并反馈 root owner；不能放宽断言、偷偷补产品源码或以重复运行掩盖 red。

## CC 指令（作者未执行）

等待 root `.agents/reports/windows-multidisplay-integration-sol-20260930.md` 的 `SOURCE_FROZEN`，再核对本 sidecar 报告的自有 SHA256 和 API 基线。两份冻结互不纳入对方 owner 文件。root 若之后改接口，先报告协调者，不改其源码。

CC 可重编译，仅指定本 integration target；在全新唯一 target/evidence 目录运行：

```sh
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR="$PWD/.agents/runs/windows-cancel-contract-cc-20260930-ATTEMPT_UNIQUE/target"
cargo test --locked --offline --target x86_64-pc-windows-msvc \
  --test windows_cancel_contract --no-run --message-format=json
```

保留完整 stdout JSON、stderr、**数字**编译 exit。只从本次 JSON 的 `reason=compiler-artifact`、`target.name=windows_cancel_contract`、`profile.test=true`、`executable!=null` 提取唯一 exe，不 glob 旧产物。记录本次 exe SHA256，复制到新的 Windows TEMP 后核对同 SHA；作者冻结报告也列其本地 compile-only artifact 指纹。

只有协调者安排的 CC+GLM 在 Windows 执行：

```powershell
# 使用协调者批准的 bounded runner，EXE_EXACT_PATH 替换成本次核验的路径。
& 'EXE_EXACT_PATH' --list
$nativeList = $LASTEXITCODE   # 立即保存，必须是非 null 数字
& 'EXE_EXACT_PATH' --test-threads=1 --nocapture
$nativeRun = $LASTEXITCODE    # 立即保存，必须是非 null 数字
```

上面是 runner 内 payload，不授权无界外层执行。外部 120s watchdog 必须记录 owned child PID、exe path、creation time、SessionId 再等待；仅对身份核实的自有子进程超时处理；wrapper exit/null Process.ExitCode 不等于 native0。`--list` 应恰 9 个上述非 ignored 测试、run 应 9 passed / 0 failed / 0 ignored / 0 filtered 才能报告纯契约 GREEN。任何 list/hash/compile/run 失败立即 STOP，保留 first-red 和实际 cleanup，报告精确断言/文件/行；不启动 Host/GUI/renderer/SSH 服务，不改 registry、不触碰用户 Notepad。

CC 报告应另存独立报告与唯一 evidence attempt，保留 source/artifact hashes、命令、native exits、实际 counts、timeout/cleanup、mock-vs-GUI 边界。单凭本9项纯契约不能升级网络disconnect或GUI状态；后续独立网络证据见文末，所有真实GUI仍deferred。

## 追加：真实匿名管道EOF（2026-09-30）

原 `windows_cancel_contract` 的cancel06只证明McpService入口，不把它升级为真实pipe证据。新独立 `tests/windows_stdio_eof.rs` + test-only `examples/windows_stdio_eof_fixture.rs` 实际通过OS匿名stdin/stdout管道驱动公开生产stdio::run及Worker，Backend仍为synthetic。CC报告 `windows-stdio-eof-cc-20260930.md`，原始 `.agents/runs/windows-stdio-eof-cc-20260930/`。

Windows真实执行4/4、native0：idle EOF、已接受mock Press后的active hold EOF、idle半帧EOF、hold期间半帧EOF。两个active case均记录Press后25ms开始cancelled cleanup，非等待5000ms正常结束；owner最终shutdown之前已读取到生产cancel=true、外部shutdown_flag=false。四个子进程native0，双输出EOF，owned I/O线程joined，kill_used=false、held模拟状态为空。协调逐一核对runner summary/terminal.json/parent.jsonl。

这新增的是**真实传输→生产reader→取消/清理**证据，不是原生按键释放或GUI证明。TCP断连cancel07、stdout断开、物理held/release与GUI恢复仍pending。旧9项/旧失败/原产物冻结不变；此文档追加不重写历史结果。

## Windows stdout 断开扩展：首败保留（2026-10-01）

在 `windows_stdio_eof` 独立目标新增两项真实 stdout 断开用例，保留原四项输入EOF用例；当前目标共六项。新增用例仍使用生产 `stdio::run`/Worker 与模拟Backend，不是完整Host入口或OS输入测试。

首次 CC+GLM Windows 执行：`--list` 列出6项；idle stdout 用例 **0 passed / 1 failed / 5 filtered，native101**。失败位于测试支撑 `close_stdout_reader`：2秒内 reader 未退出，因此没有 `stdout_read_closed_ack`，还没有验证生产断输出路径。panic时精确owned child被RAII终止，此次 `kill_used=true` 不属于成功收尾。active用例与新binary的旧四项回归按first-red规则**未运行**。历史四项EOF通过的记录不作废，也不冒充新产物通过。

原始证据 `.agents/runs/windows-stdout-disconnect-cc-20261001/evidence/remote/`；报告 `.agents/reports/windows-stdout-disconnect-cc-20261001.md`。修复限测试辅助层，真实根因与修后运行另行记录；不通过扩大timeout或改写oracle使其通过。

契约边界：仅物理关闭stdout读端不保证即时取消；主循环在下次write时才发现错误。正常hold应先自然完成Press/Release，stdio返回后由owner shutdown清理。必须保持stdin打开直到子进程退出，避免将reader EOF取消误记为stdout断开成功；主动read-close与自然EOF分别记录。

### 修后复验：6/6通过，首败不抹除

`windows-stdout-disconnect-retest-cc-20261001.md`记录CC+GLM在Windows执行2项stdout和4项旧EOF：1+1+4共6项通过、0失败；另有独立--list6，四runner均native0、无timeout、root_exited、双drain完整。仅测试helper `io_threads.rs`修复，产品未变，原2秒期限/oracle未放宽；原首败归档完整保留。

六个child均native0、kill_used=false、父owned读写线程joined。stdout两例关读端ACK后仍保持stdin到child退出，before owner cancel=false/gen0，Worker::shutdown后true/gen1；active精确正常Press/Release，hold3000.2916ms，Press后18.395ms关闭stdout，ACK后3283.8076ms才观察到stdio返回。该结果验证下次reply write发现断开后的收尾，而非中途立即取消。idle无输入；旧EOF两active取消cleanup为Press后25ms，旧4语义保留。

这是公开生产stdio/Worker配模拟输入Backend，不是产品Host main入口、真实OS按键或TCP断连测试；内部生产reader没有显式join证明，父线程join和child进程退出分别记录。7源码、2exe与root177源码冻结由协调核对，不能由这些结果推定GUI验收完成。


## cancel07续项：Windows真实回环TCP/TLS 4/4通过

证据报告：`.agents/reports/windows-tcp-disconnect-cc-20261001.md`（目录后缀为跨时区批次标签；执行UTC为2026-09-30）。原9项与6项stdio历史不覆盖，新target `windows_tcp_disconnect` 独立计数。

| 新测试 | 实际结果 |
|---|---|
| idle_tcp_eof_reaps_child_and_reconnects | 1 passed，native0；空闲目标无业务输入，EOF后正常退出并新child重连 |
| active_tcp_eof_cancels_hold_and_reconnects | 1 passed，native0；裸socket断开→取消，mock held清空，重连使用新child |
| active_tls_close_notify_cancels_hold_and_reconnects | 1 passed，native0；真实close_notify→取消/清理与重连 |
| incomplete_json_tls_close_cancels_hold_without_dispatch | 1 passed，native0；clean close补newline后的真正残缺JSON不额外派发，活动hold仍取消 |

CC＋GLM实机执行四例各一次；另有--list4，五runner native0、无timeout、双drain完整、root_exited。三次5000ms hold的runtime cleanup为Press后52/52/53ms，QPC记录Press≤cut≤cleanup；ledger均partial/cancelled/released、events_completed=[0]/total2，不把正常hold完成或owner最终shutdown当中途取消。

八个claimed worker保留确切句柄确认native0，与peer-specific生产reap日志相互核对；每例两worker PID＋创建身份不同。四supervisor正常退出0，stop请求在worker终态/退出/reap之后。无强杀或watchdog标记；不据此声称未知后代tree containment或每个生产内部线程均显式join。

执行链是真实公开remote::host::run、TLS pump、真实子进程stdin/stdio reader与Worker，输入后端为Test且反馈关闭；没有启动computer-host.exe、DesktopBackend或实际键鼠。**LAN授权、产品Host main/锁/热键/信号、真实OS按键释放与恢复、GUI仍未完成。** 这四例也不覆盖全部网络故障/消息队列组合，不能升级为网络层穷尽验证。
