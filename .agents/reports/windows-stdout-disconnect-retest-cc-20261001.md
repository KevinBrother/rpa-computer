# Windows stdout disconnect 修后独立复验 — CC（2026-10-01）

角色：CC+GLM 编排者。前置 FIRST RED（`.agents/reports/windows-stdout-disconnect-cc-20261001.md`，idle `process.rs:395 stdout reader close/join timed out`，native 101）**原样保留**；本轮为修复后独立 review + 真实运行。未改产品/测试源码/权限/防火墙/全局设置；无 GUI/Host/listener；未触碰 Notepad24332；未覆盖任何历史 run。

## 结论：修后全绿 — 新 2 + 旧 4 = **6 次测试全过**（`--list` 不计）

| 步骤 | 结果 | native exit |
|---|---|---|
| 1. `--list`（30s） | `6 tests, 0 benchmarks`（2 stdout_disconnect + 4 旧 EOF），runner success | 0 |
| 2. `--exact …idle_stdout_disconnect…`（90s） | **1 passed / 0 failed / 5 filtered**，0.58s | 0 |
| 3. `--exact …active_stdout_disconnect…`（90s） | **1 passed / 0 failed / 5 filtered**，3.60s | 0 |
| 4. `--skip stdout_disconnect::`（120s，旧 4） | **4 passed / 0 failed / 2 filtered**，1.10s | 0 |

四个 runner summary 均为 `native_exit_code=0, outcome=success, timed_out=false, cleanup_state=root_exited`，双 drain 完整，`executable_sha256=70899867…404db`（本轮修复后 parent exe）。无任何失败尝试。

## 修复 review（通过后才执行）

- **改动范围**：仅 `tests/windows_stdio_eof/io_threads.rs`（f541235e…→ff65129f…）；其余 6 文件哈希与首轮完全一致（逐字节 cmp SAME），root 生产源码 0 变化（product-before/after 均 315fa567…，146 条基线不变）。manifest 7/7 + 两 exe 双端核对通过。
- **RCA 机制核可**：rust-src 存档（origins.json 哈希锚定本机 stable toolchain rustc 1.98.1/48a229cea 的 process.rs/child_pipe.rs）证实父端 stdio 管道为 **overlapped/异步句柄、读走 ReadFileEx（内部 OVERLAPPED+APC）**，故旧 `CancelSynchronousIo`（针对同步 pending I/O）类型不匹配且旧代码忽略其返回值——与此工具链匹配，不是猜测的 Win32 返回码（报告明确未确认旧调用实际 BOOL/GetLastError，未谎称 ERROR_NOT_FOUND/995）。primary-docs（CancelSynchronousIo/PeekNamedPipe 微软正文 HTML+txt+URL 封存）支持该区分，且如实保留 Peek 对同步句柄可阻塞的警告、把修复限定为匹配实现的异步父端。
- **新实现语义**：`ChildOutput` trait 只允许 `ChildStdout/ChildStderr`（两个调用点均 `child.stdout.take()/child.stderr.take()`，Stdio::piped 唯一读 owner）；`PeekNamedPipe` 仅查 available、只读不超过现有字节的长度（不对未来数据阻塞）；空队列 2ms `park_timeout` 轮询 + stop `unpark` 直唤醒；显式 stop → 线程实际 `drop(pipe)` → `ParentClosed` ACK（非自然 EOF，`out_eof=false` 断言不变）；Peek 失败立即 `last_os_error`，仅 109（ERROR_BROKEN_PIPE）为自然 EOF，**995 及其它错误保留为 Error，不因 stop 吞错**；移除 CancelSynchronousIo；join 里 2ms 轮询与 2s 上限不变，oracle/超时/其余源码不变。
- **quality**：有界等待/RAII/identity/hash 均保持；未伪造 OS 错误（`raw_os_error:"not_exposed_by_public_stdio_api"` 原样保留）。

## 执行（NEW 远端目录，旧 run 未覆盖）

- 远端：`C:\Windows\Temp\windows-stdout-disconnect-repair-cc-20261001\`（scripts/Runner.cs 原样上传，`runner_source_sha256=ad2a607c…`、`runner_module_sha256=aced18e6…` 与历轮一致）；env 三变量同上轮但 **fixture SHA 换新值 d6fd376b…584c**，远端 Get-FileHash mismatch 即 throw（本轮 `remote_hashes_ok`）。
- 每步核对 numeric `native_exit_code`，0 才继续；无重试/无放大 timeout。超时策略：runner poll 同一 handle 不重启（本轮无超时发生）。

## 逐 case 核对（6 case 证据全部原样下载）

### stdout-idle（parent pid 22200 / child pid 28352）与 stdout-active（parent pid 5344 / child pid 26688）— 新 2 例

两例共同项（terminal.json + parent.jsonl）：
- `stdio_outcome=clean`、**before owner：cancelled=false / generation=0 / held=[] / cleanup=[]**；**after owner：cancelled=true / generation=1**、release_all 恰 1 次（cancelled=true，held_before/after 均 []）、`shutdown_status=clean`、`worker_faulted=false`、`post_shutdown_dead=true`。
- **stdout 读端主动关闭而非自然 EOF**：`stdout_read_closed_ack` 记 `parent_closed_stdout=true, out_eof=false, sole_read_owner_dropped=true, stdout_reader_joined=true`。
- **stdin 保持到 child exit 之后**：`child_exit.stdin_still_open=true`；`parent_closed_real_ChildStdin.reason=after_stdout_child_exit`、`child_exit_already_observed=true`。
- `child_exit.native_exit=0`、`owned_cleanup.kill_used=false`、`readers_joined=true`、`stdin_open_through_child_exit=true`、双端 SHA/nonce/identity/build_id（e7de29ca…6500）全符。
- idle：`before_shutdown.events=[]`、owner cleanup `since_press_ms=null`（无输入）。

### stdout-active 精确时序（共享 QPC freq=10,000,000，两端同机时钟）

- backend dispatch（child stderr）：press `dispatch_qpc=12,382,303,111,432`，release `12,382,333,114,348` → **hold 自然时长 30,002,916 ticks = 3000.3ms**。
- 父端 close ACK `ack_qpc=12,382,303,295,382` → **press→close ACK = 183,950 ticks = 18.4ms**，落在实际 3000ms hold 内（测试断言 `<3000ms` ✓）。
- `stdio_return.return_qpc=12,382,336,133,458` → **close ACK→stdio 返回 = 32,838,076 ticks = 3283.8ms**（= 剩余 hold ~2982ms + 自然 Release + 生成 reply 后**下一次 write 才发现断开**；这是返回时刻与 close 的间隔，不声称精确 write 错误时刻）。
- `before_shutdown.events` 精确 `shift press + release, 均 cancelled=false`；owner cleanup `since_press_ms=3367ms ≥ 3000`（正常完成，非取消中断）；`before.cleanup=[]`（正常 dispatched action 有 planned release，无额外 release_all）——完全符合 C1 纠正后的因果模型。

### 旧 4 例（idle/active/partial-active/partial-idle）— EOF 语义精确保持、未混同

- 全部 `reader_cancelled_before_shutdown=true`、`generation_before_shutdown=2`、`stdout_eof=true`、`stdin_still_open=false`、`native_exit=0`、`kill_used=false`、`readers_joined=true`、Clean/不 fault/`post_shutdown_dead=true`。
- cleanup 计数保持历史精确值：idle 类 0→1，active 类 1→2（active 先 cancelled cleanup 再 owner cleanup）——与首轮 4/4 GREEN 的 reader-EOF 即时取消契约一致，与 stdout 两例（cancel 只来自 owner、无提前 cleanup）**清晰区分**。

## 收尾核验

- 残留：`Win32_Process.ExecutablePath LIKE 'C:\Windows\Temp\windows-stdout-disconnect-repair-cc-20261001%'` → **RESIDUAL_NONE**（精确 owned 路径，无 name/global kill）。
- 运行后重验：`source-freeze.sha256` 7/7 OK、`artifacts.sha256` 2/2 OK——源码与工件前后未变。
- 本机证据哈希/文件读取允许范围内完成；未执行 Mac/Linux 测试。

## 边界

- 证明的是：真实匿名管道 stdout 读端主动关闭 → 生产 stdio 在下一次 reply write 失败后返回 → caller（owner）cancel+shutdown 收尾（mock backend）。**不是**物理 OS 输入/GUI/TCP cancel07/完整 Host 入口证明；**不覆盖**"无 write 时的即时断开检测"（未实现，见 C1 纠正）。
- fixture 成功 0 / 错误 70 与产品 Host 0/7 仍为不同入口边界，未混用。
- `write_failure_inference` 是公开 API 边界的推断记录（ACK/trigger/return/无 EOF 或 fault），非观察到的 Win32 错误码。

## 索引

- 报告：本文件；原 RED 报告 `.agents/reports/windows-stdout-disconnect-cc-20261001.md` 原样保留。
- runs：`.agents/runs/windows-stdout-disconnect-retest-cc-20261001/`
  - `logs/run-repair-retest.ps1`、`logs/remote-run.stdout|stderr`
  - `evidence/remote/runner-evidence/{list,idle,active,old4}/`（各 summary/stdout/stderr/config/identity）
  - `evidence/remote/case-evidence/`×6（stdout-idle-22200…、stdout-active-5344…、idle-25584…、active-25584…、partial-active-…、partial-idle-…，各 parent.jsonl/terminal.json/stdout.raw/stderr.raw）

停止：报告完毕，不追加其它任务。


## 协调者原始证据复核与报告纠正

协调逐一亲读4份runner summary/stdout/stderr和6份terminal/parent证据，重新计算QPC：hold3000.2916ms、press到closeACK18.395ms、ACK到stdio返回3283.8076ms，计数/原生退出码吻合。原本新两例小节将目录nonce中的父测试PID22200/5344误标为child；已依据parent.jsonl的spawn.child纠正为child28352/26688，原错误在此留痕。原始文件与6/6运行结果不变。fixture调用Worker::shutdown，由该方法内部置cancel，不是执行完整Host显式cancel+shutdown入口。
