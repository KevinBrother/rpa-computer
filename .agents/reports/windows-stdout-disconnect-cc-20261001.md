# Windows stdout disconnect — CC 独立审查与真实测试（2026-10-01）

角色：CC+GLM 编排者（非作者）。冻结源码/两 exe 双端 SHA 全部核对一致；未改任何产品/测试源码；无新 harness（仅 runner config + 编排 ps1）；无 GUI/Host/listener；未触碰 Notepad24332 与任何授权/防火墙设置。

## 结论：FIRST RED — 步骤 2（idle stdout disconnect）FAILED，按协议停止

| 步骤 | 结果 | native exit |
|---|---|---|
| 1. `--list` | **6 tests, 0 benchmarks**（恰 2 个 stdout_disconnect + 4 个旧 EOF），runner success | 0 |
| 2. `--exact stdout_disconnect::idle_…` | **FAILED**（0 passed / 1 failed / 5 filtered，2.51s） | **101** |
| 3. `--exact stdout_disconnect::active_…` | **未执行**（first-red 停止） | — |
| 4. `--skip stdout_disconnect::`（旧 4 回归） | **未执行**（first-red 停止） | — |

runner summary（idle）：`native_exit_code=101, outcome=native_failure, timed_out=false, cleanup_state=root_exited`，双 drain 完整无截断，`executable_sha256=5f27aaec…6477f`。

**失败点**：`thread 'stdout_disconnect::idle_…' panicked at tests/windows_stdio_eof/process.rs:395:9: stdout reader close/join timed out`。这是**父端测试支撑代码**的失败（`close_stdout_reader` 的 `join_bounded(2s, cancel=true)` 未能让 stdout reader 线程退出），**生产 stdio stdout 断开路径本身未被走到**。

## Spec review（通过，才进入执行）

对照任务清单逐项核过 diff 与磁盘源码（二者一致）：
1. 仅新增 `stdout_disconnect` 模块 2 例；旧 4 例函数体逐字保留（diff 无旧测试体改动，仅共享支撑 additive 扩展）。
2. 真实关闭：`Thread::reader` 拥有唯一 ChildStdout（从未 clone），`drop(pipe)` 后才发 `Message::ParentClosed`；`ParentClosed` 与 `End`（自然 EOF）是不同事件，`close_stdout_reader` 显式断言 `!out_eof`，不伪造 EOF；join_bounded(2s) + CancelSynchronousIo。
3. stdin 全程打开直到 child exit：`finish_stdout_disconnect` 断言 `stdin.is_some()` 贯穿；`close_stdin` 在 stdout 场景断言 child exit + terminal 已见后才 drop。
4. 公开 `stdio::run` + `Worker(BackendFactory::Test)`；mock backend 非 OS 输入。
5. oracle 与 C1 纠正一致：before owner `cancel=false/generation=0/held=[]/cleanup=[]`；owner shutdown 后 `cancel=true/generation=1`、release_all 恰 1 次、Clean、不 fault、`post_shutdown_dead=true`；active 用例精确 `press+release, cancelled=false`、`since_press_ms>=3000`；不要求无 write 即时检测；write 错误如实记 `raw_os_error:"not_exposed_by_public_stdio_api"`，不伪造 Win32 码。
6. active close ACK 用共享 QPC（`delta*1000 < 3000*freq`）落在真实 hold 内；identity/nonce/hash 交叉核对保留。
7. 成功路径无 kill（Drop 仅对未退出 child kill）。

## Quality review（通过）

有界等待（2s join / IO_BOUND pump / 8s recv）、RAII Drop 完整、不伪造 EOF/OS 错误、identity+hash 核对、frozen source 与磁盘一致、fixture 错误退出码 70 与产品 Host 7 分离未混用。

## RED 现场证据（child pid 10368，全部原样下载）

`case-evidence\stdout-idle-19300-1790786237972630600-0`：
- **child 侧健康**：stderr `EOF_FIXTURE ready`（nonce/qpc_frequency=10000000/SHA 085df7da…20c4/build_id f61a26fe…3472 全符）；`stdout.raw` 恰 3 条完整 JSON-RPC reply（id 1/2/3，initialize/open/observe）。
- **parent.jsonl 时序**：spawn 1ms → handshake 写 32-83ms → `stdout_close_requested, stdin_open=true` @146ms → **（2s 内无 `stdout_read_closed_ack`）** → panic Drop：`drop_stdin_cleanup, parent_panicking=true, stdin_was_open=true, parent_closed_stdout=false`；`owned_cleanup: kill_used=true, child_exited=true, native_exit=1`（**1 是父端 TerminateProcess 终止码，非 fixture 自身退出码**）。
- **机制观察（RCA 交 owner，不下最终结论）**：`io_threads.rs:77-99` 的 join_bounded 确实执行了 `stop=true` + 对真实 JoinHandle 句柄反复 `CancelSynchronousIo` + 2ms 重试，但被阻塞在 `pipe.read`（`io_threads.rs:55`，子进程写端仍打开 → ReadFile 阻塞）的 reader 线程 2s 内未退出。即：stop 标志 + CancelSynchronousIo 未能在该匿名管道阻塞读上达成解除。可能方向（均未证实，需 owner 验证）：CancelSynchronousIo 对该 ReadFile 未生效（返回值未检查）或 std 层对 OPERATION_ABORTED 的处理/重入。**此缺陷在测试支撑（本轮新增 io_threads/process 支撑），不是生产 src；也不是"产品 stdout 断开取消不了"的证据。**

## 执行合规

- 远端目录 `C:\Windows\Temp\windows-stdout-disconnect-cc-20261001\`（NEW），含两冻结 exe（远端 Get-FileHash 由编排脚本自校验，mismatch 即 throw；本地 shasum 双端一致）、原样 runner（`runner_source_sha256=ad2a607c…`、`runner_module_sha256=aced18e6…`，与既往轮一致）、config×2、evidence 父目录预建，runner leaf 未预建。
- env 由调用 runner 的父 ps1 设置：FIXTURE=新路径、FIXTURE_SHA256=085df7da…20c4（**未复用旧轮 SHA**）、EVIDENCE_DIR=case-evidence。
- 每步核对 runner summary 的 numeric `native_exit_code`；首个非零即 throw 停止，无重试/无反复碰绿。首个 wrapper 错误原样保留（无 wrapper 层错误；首个失败即 test RED）。
- 残留检查：`Win32_Process.ExecutablePath LIKE 'C:\Windows\Temp\windows-stdout-disconnect-cc-20261001%'` → **RESIDUAL_NONE**（按精确 owned 路径，无 name/global kill）。父端 RAII 已 kill child。
- 运行后重验 `source-freeze.sha256`（7/7 OK）与 `artifacts.sha256`（2/2 OK）——源码与工件未变。
- 无 GUI/Host/网络/授权变更；Session 0 纯 console。

## 边界

- 本轮**不构成**任何 stdout disconnect 契约 GREEN；active 用例与旧 4 项回归均未运行（不是通过，是未执行）。
- 不要求产物 Host 退出码（fixture 错误码 70/产品 7 为不同入口）；非完整 Host 入口验证、非物理按键释放/GUI 证明。
- RED 的 root cause 未证实；修复归 owner（测试支撑侧），本 CC 未改任何实现。
- 未执行 Mac/Linux；历史 run 目录未覆盖。

## 索引

- 报告：本文件（`.agents/reports/windows-stdout-disconnect-cc-20261001.md`）
- runs：`.agents/runs/windows-stdout-disconnect-cc-20261001/`
  - `logs/run-stdout-disconnect.ps1`（编排，first-red throw）、`logs/remote-run.stdout` / `remote-run.stderr`
  - `evidence/remote/list/`（summary/stdout=6 tests/stderr/config/identity）
  - `evidence/remote/idle/`（summary native101/stdout/stderr/config/identity）
  - `evidence/remote/case-idle-panic/stdout-idle-19300-1790786237972630600-0/`（parent.jsonl、stdout.raw、stderr.raw）

停止：first-red 已保留，active 与旧 4 回归未运行，等待 owner 对 stdout reader close/join 超时的 RCA。
