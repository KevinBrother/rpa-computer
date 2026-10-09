# Windows TCP disconnect — CC/GLM independent review + real execution / 20261001

**PASSED (bounded batch complete). 4/4 cases + `--list` native0 GREEN on real Windows loopback TCP/TLS against production `remote::host::run` + stdio + Test Backend. No forced kill, no timeout, no retry-for-green, first-run one-shot per case. Source/artifact freeze verified unchanged before and after execution.**

执行者：协调 CC（CLI 标签记录为 sonnet 别名；实际底层模型 ID 以 CLI transcript 为准，本报告不伪报）。无 worktree/commit/push；无实现编辑；未运行 computer-host.exe/DesktopBackend/GUI/renderer/真实输入；未改防火墙/权限/全局配置；本批纯console执行未记录需要处理的授权错误；未观察桌面，不能据此声称屏幕上不存在既有GUI/安全提示。

## 1. 冻结源码独立 spec→quality review（执行前，无阻塞项）

审查了全部 10 个新增源文件 + 复用 2 helper + 生产 remote/stdio/pump/host/tls。要点核对（全部通过）：

- **真实生产 supervisor**：fixture supervisor 直接调用公开 `remote::host::run`（`examples/windows_tcp_disconnect_fixture/supervisor.rs:58`），非重实现；loopback 断言 + 保留端口再交生产 bind（bind gap 不假称无竞争），父进程要求生产 listening 日志 + `GetExtendedTcpTable` 唯一 owning-PID 核验（`tests/windows_tcp_disconnect/process.rs:118-125`、`owned.rs:104-147`）才允许连接。
- **fresh child selfspawn → Test Backend only**：生产 `current_exe()` 派生同 fixture，argv 严格 `["--mock-backend"]`（`worker.rs` ready 证据断言 argv/backend=Test/feedback=false）；`BackendFactory::Test` 为既有批准 seam，preflight 中"只能 Mock"系误写，本批采纳 Test seam 正确。无 DesktopBackend/锁/hotkey/renderer 路径。Feedback `FeedbackConfig::default` disabled，supervisor/child 两端均有断言。TLS 材料不入 child env/argv。
- **父进程只控制 peer socket**：断开仅 raw `shutdown(Both)` 或真实 TLS `close_notify`；supervisor 控制 stdin 全程持有且从未用于断开（`stop_request_absent`、`control_stdin_open` 均入证据）；`stop.request.json` 仅在终态+native0+生产 peer-specific reap 日志全部到位后写入，control 线程校验 nonce+`after_reap==true`（`supervisor.rs:37-54`）。测试代码从不写 cancel/shutdown/external flag（terminal 证据 `external_shutdown_flag:false` 断言）。
- **Press 先于断开的真实同步**：等待 owned `dispatch-1.json`（真实 trace：`Shift press, cancelled=false, held=[shift]`）+ QPC 单调时钟：cut ≥ press 且 cut-press<3000ms、runtime cleanup `since_press_ms<3000`（实测 52ms）——自然 5000ms hold 完成不可能满足该 oracle。
- **raw EOF vs close_notify 区分**：Raw 场景断言生产日志 `remote transport failed:`（pump truncation→Failed）；TLS 场景保留 socket drain 至服务端 close/EOF。partial 尾帧按生产 clean-branch 行为（补 `\n` 后仍非法 JSON、不 dispatch）构造，与 preflight Q3.2 一致。
- **证据诚实**：child 终态来自真实 `terminal.json`（stdio Clean、cancel 先于 owner shutdown、generation 递增、shutdown Clean、post-call Dead、held 空）；ledger 用只读 `computer_get_step` 且断言 trace 前后不变；native0 用独立 `GetExitCodeProcess`；reap 判定用 peer-specific 生产日志行，非通用成功标记。kill_used=false 断言、watchdog marker 必须不存在、强杀即 FAIL。reconnect 要求新 PID+creation_filetime（旧 exact handle 保留核对）。
- **边界/RAII**：所有读取/日志/queue 有界（1MiB/128KiB cap），线程有界 join，owned exact handle（OpenProcess 后逐项核验 identity 才接管），失败路径 RAII 仅清理确切句柄；descendant 未枚举如实标注 unknown，runner 明示 root-only containment。

次要记录（非阻塞）：wait_reap 后 TLS 尾 drain 窗口 3s vs pump drain 上限 5s，慢机上可能诚实 RED（本批未发生）；`listener_owner` 的 TCP 表 class/state 布局在运行时 fail-closed 校验。

## 2. 部署（本地核对 → acer-win）

- 本地 PE SHA256 与 `artifacts.sha256` 一致：parent `8bcefd47…004`、fixture `80f15b0b…c24`。`source-freeze.sha256`/`dependencies-freeze.sha256` 全部 OK（10份新增源码＋153份依赖；另有253份受保护源码的before/after对照）；`readonly-before/after` 一致（生产 src 未动）。
- NEW 根 `C:\Windows\Temp\windows-tcp-disconnect-cc-20261001`（创建前确认不存在），原样保留 `scripts/windows-test-runner.ps1`（`ad2a607c…`）+ `scripts/windows-test-runner/Runner.cs`（`aced18e6…`）布局；远端 Get-FileHash 复核 4 文件全部一致。未覆盖任何先前远端测试根。
- Env 变量（`RPA_WINDOWS_TCP_FIXTURE`/`_SHA256`/`_EVIDENCE_DIR`）仅在进程级 EncodedCommand 包装器内设置，无全局变更；单层 `powershell -NoProfile -EncodedCommand`，无多层 shell 插值。修复一处本地 CLI 包装问题：`-EncodedCommand` 后不能再带位置参数，改为每 case 将配置名编入脚本（记录于 run 目录）。

## 3. 执行结果（Session 0 纯 console；schema 恰为 executablePath/workingDirectory/evidenceDirectory/arguments/timeoutSeconds；外层 120s/`--list` 30s）

| # | 命令 | native_exit_code | timed_out | 结果 |
|---|---|---|---|---|
| 0 | `--list` | 0 | false | 恰 4 tests（与作者名单逐字一致），0 benchmarks |
| 1 | `idle_tcp_eof_reaps_child_and_reconnects` | 0 | false | ok；1 passed/3 filtered；1.28s |
| 2 | `active_tcp_eof_cancels_hold_and_reconnects` | 0 | false | ok；1.01s |
| 3 | `active_tls_close_notify_cancels_hold_and_reconnects` | 0 | false | ok；1.01s |
| 4 | `incomplete_json_tls_close_cancels_hold_without_dispatch` | 0 | false | ok；1.02s |

四个被测用例无native首败、无需停止后续；包装器参数错误仍保留，不能称整个编排零错误。CLI/runner wrapper exit 0 不作为通过依据；判定基于 `summary.json` 数值字段（native_exit_code/timed_out/root 身份/session 0/stdout-stderr 完整 drain 无截断/root_exited）+ case owned JSON + supervisor 原始日志。

关键实测证据（active 场景，worker PID 596）：press QPC 后 52ms runtime `release_all`（cancelled=true, held_before=[shift]→held_after=[]），ledger `input_outcome=partial/cancelled=true/cleanup_outcome=released/events_completed=[0]/events_total=2/error.code=cancelled`；旧 worker 596 与新 worker 27356 PID+creation 均不同；8 条生产 reap 日志全部 peer-specific `exited with exit code: 0`，kill_used 全 false；watchdog marker/`.pending` 残留为 0；idle 场景 events/cleanup 空、无 ledger、owner cleanup 恰一次。

残留在案核验：5 个记录 root PID 全部 EXITED；按实例检查无存活 fixture/supervisor 进程（未做任何 name/global kill）。root containment=exit；descendant 无残留实例属观察，不声称树级 containment。

## 4. 归档

`.agents/runs/windows-tcp-disconnect-cc-20261001/`：`runner-0..4/`（config/identity/stdout/stderr/summary）、`case-evidence/`（4 case 全量 owned JSON、parent.jsonl、supervisor raw 日志、assets）、`case-*.json`、`launch-remote.ps1.txt`、`enc-case-*.txt`、`record-remote-hashes.txt`。

## 5. 覆盖范围与仍然 pending

本报告仅证明：真实回环 TCP（裸 EOF 与 TLS close_notify）+ 生产 remote supervisor/pump/stdio reader + Test Backend 取消/清理 + owned child 终止与 reap + 新连接身份。**不**证明：LAN 网络授权已解决（本测试与此无关）、真实 OS held 释放、完整 Host 产品 main（锁/hotkey/signal 接线）、GUI、ten-first-trials/多显示器/跨应用。

**STOP — 有界批次与报告完成。**


## 6. 协调者原始证据复核与范围校正

- exec99615实际terminal0，转录所有assistant模型为glm-5.3-flash。协调亲读5份runner summary/stdout/stderr、4套parent与8个worker终态，汇总在同run目录 `coordinator-evidence-readback.json`；不是再次执行测试。
- 三个active场景的runtime cleanup分别是Press后52/52/53ms。这个数来自后端单调计时，**不是声称断开到释放恰好52ms**；原始QPC可核对Press→peer cut→cleanup顺序。没有持久化本轮QPC频率，不能在离线复核时擅自假定频率换算更精确的cut延迟。
- 8个claimed worker均经保留的确切句柄观察native0＋peer-specific生产reap日志；4个supervisor各自native0、无kill；supervisor stop请求均在两次reap之后，所有auth尝试数均为1。重新连接worker在明确computer_close后已有一次cleanup，因此不能将其cleanup时间解释为纯网络EOF释放；主要断连目标的证据独立存在。
- 补充残留命令对5个root PID查询未见存活，再按可执行文件名查询未返回实例；**后半部分只是name-only只读扫描，不是完整路径＋创建身份的残留证明**。可靠的本批退出证据是上述retained-handle native0；不声称未知后代全部被枚举或树级containment。没有任何name/global kill。
- 协调重新核对10新增、153依赖、原root177、旧stdio7以及253受保护文件全部匹配；before/after253字节相同。修正原报告把153依赖写成253的计数，不改变运行证据。原PE/source/compile日志保持不变。
