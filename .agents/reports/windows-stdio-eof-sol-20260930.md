# Windows actual anonymous-pipe stdio EOF — SOURCE_FROZEN

任务日期：2026-09-30。Owner：Codex sol。**仅源码、静态检查与 Windows 交叉 compile/link-only；没有运行测试/产物、SSH、GUI、Host、CC 或任何代理。** 不 worktree/commit/push，不改生产 src/Cargo/root 旧测试/旧 cancel9 文档或证据。

## 交接状态

**SOURCE_FROZEN / 两条 Windows compile-only 均 exit0 / 4 个新 integration case 运行 PENDING。** 这不是 EOF GREEN，不是 GUI/native input 验收，也不是 cancel07 TCP disconnect。现有 root275 + 四 integrations59（其中 cancel9）Windows 通过属于读取的 `.agents/reports/windows-root-race-retest-cc-20260930.md` 历史结果；本轮没有重跑、覆盖或拿旧结果证明新测试通过。

已读取 EOF 设计 `docs/superpowers/plans/2026-09-30-windows-geometry-and-eof.md`、旧取消矩阵、root race repair/retest 报告以及真实 stdio/reader/Worker/Runtime 源码。按已批准设计执行；skills 的 worktree、测试运行和委派步骤遵从本任务禁令，不执行。CC+GLM 的独立 review/Windows 执行仍由协调者统一安排。

## 写集（6 个全新 source；另本报告及新 evidence）

- `tests/windows_stdio_eof.rs`：4 个 Windows-only contract tests。
- `tests/windows_stdio_eof/process.rs`：真实匿名管道父进程、JSON-RPC、identity、RAII/evidence。
- `tests/windows_stdio_eof/io_threads.rs`：有界读写线程/精确 owned thread I/O cancellation。
- `examples/windows_stdio_eof_fixture.rs`：显式 test-only console entrypoint。
- `examples/windows_stdio_eof_fixture/backend.rs`：actual Backend trait 的内存 dispatch/held/cleanup oracle。
- `examples/windows_stdio_eof_fixture/identity.rs`：父/子共用 Windows PE SHA256 与进程身份读取；不是 production helper。

最大文件469行，无1000+单文件。未修改旧 `tests/windows_cancel_contract*`、`docs/windows-cancel-contract-matrix.md` 或旧报告。没有修改 Cargo、默认 product bins 或 geometry owner 文件。

## 覆盖矩阵（源码静态计数4，非运行通过数）

| 完整 test 名 | 真实过程 | 精确 oracle |
|---|---|---|
| `idle_stdin_eof_shuts_down_real_stdio_worker` | 子进程生产 stdio；父 initialize/initialized/open/observe 后真正 drop ChildStdin | shutdown 前 reader cancel=true、epoch>0、external flag=false；无输入/运行期cleanup，owner shutdown 一次cleanup；Clean、不fault、shutdown后调用Dead；两管道EOF、native0、读线程joined |
| `active_hold_stdin_eof_cancels_after_successful_press` | 同上；真实JSON-RPC step=5000ms Shift key_hold；父收到Backend记录成功Press诊断才关闭stdin | 恰一次press，cancel读取false；之后只允许 cancelled=true 的release cleanup，无新业务输入；运行期release_all一次且held空；owner shutdown后共2次；close→exit <3s 且 press→runtime cleanup <3000ms，排除正常5000ms完成 |
| `partial_frame_idle_eof_never_dispatches_input` | idle后发送截断的computer_step JSON（无换行、缺arguments值/闭合），再关闭管道 | 不阻塞等换行；无输入；不得出现id99成功；如果可见parse reply，仅id=null/-32700；EOF优先可不输出该错误；其余终态同idle |
| `partial_frame_during_hold_eof_cancels_active_action` | 实际hold Press之后，父先写截断JSON再close | 原active action被真实reader EOF中止；坏半帧不成为另一个action；partial/cleanup/held/3s/终态同active |

visible step reply（若存在）必须：`request_id=eof-held / input_outcome=partial / cancelled=true / cleanup_outcome=released / error.code=cancelled / events_total=2`。`events_completed` 与真实trace逐字关联：press索引0；若只有press则[0]，若cleanup额外发送计划release则[0,2]；绝不允许新的press、not_started或完整成功。在当前执行器取消分支通常只有press与release_all，不靠注入一个假release来填满结果。

EOF控制优先时，不能保证所有排队的错误/响应都会发到wire，所以**不把响应缺失改写为成功，也不单凭缺失判失败**。每次都必须取得child本地terminal诊断：真实backend trace、运行期cleanup、shutdown之前的生产cancel、生产ShutdownStatus/故障状态、shutdown后的生产调用Dead；并同时确认child native0、stdout/stderr EOF和父读线程回收。缺terminal或缺身份/EOF/cleanup证据不能通过。`stdout.raw`原样保存，CC必须区分“wire-visible partial”和“仅本地终态”，不能编造客户端已收到reply。

## 真实路径与 mock 边界

1. 父 `Command` 使用独立fixture的**显式绝对路径**，`Stdio::piped()` 提供真实 OS 匿名 stdin/stdout/stderr；CREATE_NO_WINDOW，不是 Host/GUI/网络程序。
2. child只构造 `Worker::start(BackendFactory::Test(...))`，调用公开生产 `stdio::run(&worker, version, shutdown_flag)`；它实际读取继承的 `io::stdin()`，输出继承的 `io::stdout()`。不调用handle_eof/run_with_reader、不用Cursor当transport input。
3. `Cursor` 只在fake的16×16 PNG字节编码中出现，与stdin或传输无关。Capture为内存图像，不调用OS capture。
4. mock inject接受并记录synthetic key Press/held，写一次stderr诊断并立即返回。**没有父release指令、barrier等候、stdin读取、mock内sleep、cancel.store或CancelHandle.cancel**。5000ms hold由生产Runtime负责，EOF由生产reader触发cancel。
5. parent在Press诊断后只drop自身ChildStdin。child读取 `worker.cancel_handle().is_cancelled()` 的时间在owner `worker.shutdown()` **之前**，避免把shutdown自己设置的cancel伪装成reader EOF。external AtomicBool只创建false，从不写入。
6. `stdio::run`本身不负责最终Runtime shutdown；fixture显式执行入口owner应做的生产Worker.shutdown并记录返回值，随后用生产call_with_deadline确认已Dead。`session_active()`源码明确只反映lifetime clock、不是Runtime终态，shutdown未清该诊断clock；本套只原样记录 `session_clock_diagnostic`，不编造“false才代表关闭”的错误oracle。终态依据Clean/不fault/不再可调用、实际cleanup和child退出。
7. JSON-RPC stdout只来自生产stdio；所有fixture诊断写 `EOF_FIXTURE <JSON>` stderr，父evidence写owned文件。初始化返回的serverInfo.version也校验，不能以stderr自说版本替代真实握手。
8. synthetic held-state不是Windows SendInput的物理按键证据。始终不构造DesktopBackend、不启动Host/renderer/窗口/OS input、网络、registry或全局hooks；可交CC在Windows Session0纯console运行。

## 有界 I/O / 回收与残留证据

- 父stdout/stderr各一个owned reader线程；每stream cap1MiB，消息内存同样受总字节cap约束，overflow是失败，不静默截断通过。主线程recv_timeout，单阶段8s。
- 每次stdin write在独立owned writer线程，最长8s结果等待；成功返回真实ChildStdin给父、线程joined后才能下一次write/close。关闭stdin时必须不存在writer持有的第二份句柄。所有请求<16KiB。
- 正常finish要求child已退出、两个reader都读到真实EOF、buffers无截断、native exit精确Some(0)、两个reader各2s内join。
- panic/timeout时Process RAII先close stdin，再检查/终止这个**保留的Child handle**并bounded try_wait（5s）。不按PID重新open、不做name/global kill、taskkill或进程树扫描。然后仅对自己的JoinHandle调用CancelSynchronousIo，stop标志+重试处理read竞态，每次join2s硬限；没有无界join。失败时记录 `INCOMPLETE`，不能称清理已完成。
- evidence的`parent.jsonl`包含spawn身份、argv、stdin writes、收到Press、关闭stdin、nativeexit、readers_joined、RAII的kill_used/child_exited/writer_joined/readers_joined/parent_panicking。stdout/stderr保留原字节，terminal另存json。文件create_new/unique case目录，不删除旧attempt。
- child内部生产stdio reader没有公开JoinHandle，本fixture不能证明单独join了该内部线程；证明的是实际reader EOF取消路径与子进程退出。父owned read/write线程有join证据。整个进程退出不等于证明所有内部线程逐一join。
- outer watchdog建议240s（全4项）或单case90s。**外部强杀parent会绕过Rust Drop**；已有runner不保证descendant containment。若发生，必须保留parent.jsonl中child PID/image/creation/session/nonce并按协调者精确身份流程检查残留，不能自动宣称后代清理。常规pass必须kill_used=false且全部joined/child_exited=true。

## stdout断开：已评估，明确 deferred

当前生产stdio在发送response时才发现write/flush失败，并退出loop；若正在5000ms hold，尚没有stdout写操作，单独关stdout不等于reader EOF，也不能确保立即中断hold。另reader可仍阻塞在保持打开的stdin，直到child退出。可以另设计idle ping触发写失败的生命周期测试，但它不能证明本任务active cancellation或TCP断连。为了不加入没有可靠即时取消oracle的假通过，本次**不添加stdout断开测试**，不创建ignored空占位；stdout断开与cancel07网络断连分别继续deferred。

## 编译/API基线与静态验证

只读依赖基线是root race repair后的工作区，root公共API未由本任务改动。校验root repair `source-freeze.sha256`所有条目，再加旧cancel矩阵/报告等共179条，编译前后未变化。独立监测Cargo/src/crates共146条前后完全相同，见`root-api-before.sha256`和`root-api-after.sha256`。

- Build ID（完整生产源/API manifest SHA256）：`315fa567b3cbe74b0ef2bc9a71323071b0b6963b73dab5c3d7fda9237cb9bf7f`。
- 父、子共同嵌入version：`rpa-computer/0.1.0;windows-stdio-eof-v1;315fa567b3cbe74b0ef2bc9a71323071b0b6963b73dab5c3d7fda9237cb9bf7f`。
- 源码semver只是0.1.0，**不能单靠semver声称同一版本**；应联合manifest/build ID/两个exe SHA确认。编译期BUILD_ID缺失时test/fixture运行会报错，不silent skip。
- 执行命令前加载既有xwin环境并立即覆盖CARGO_TARGET_DIR至全新`/tmp/windows-stdio-eof-sol-20260930.3wqefjmd/target`，CARGO_INCREMENTAL=0。target约1.1GiB，不在剩3.1GiB的工作卷堆构建文件；不删除任何旧产物。

```sh
source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh
export CARGO_TARGET_DIR=/tmp/windows-stdio-eof-sol-20260930.3wqefjmd/target
export RPA_STDIO_EOF_BUILD_ID=315fa567b3cbe74b0ef2bc9a71323071b0b6963b73dab5c3d7fda9237cb9bf7f
export CARGO_INCREMENTAL=0
cargo test --locked --offline --no-run --test windows_stdio_eof   --target x86_64-pc-windows-msvc --message-format=json
cargo build --locked --offline --example windows_stdio_eof_fixture   --target x86_64-pc-windows-msvc --message-format=json
```

| 命令 | 完整原始证据（新runs目录） | 数字exit / elapsed |
|---|---|---|
| integration compile+link only | integration-1.command/.jsonl/.stderr/.exit/.elapsed | 0 / 17.331s |
| fixture compile+link only | fixture-1.command/.jsonl/.stderr/.exit/.elapsed | 0 / 0.823s |
| 限定6文件rustfmt --check | format-check.log/.exit | 0 |

两条编译无warning/error。第一条Cargo还会按包约定构建computer-host/client companion bins；**未运行它们，也不把它们当本任务fixture或交付测试目标**。本次只冻结下面由本次compiler-artifact JSON精确提取的两个exe，无glob旧文件。四项静态test清单及baseline核对在static-checks.json；没有 --list/测试/例子CLI实际执行。

## 完整自有源码冻结 SHA256

```text
68c6b6a8c84bb2da2e542c6c09c3724e1649047a0014d1e415da8c1ae391756a  tests/windows_stdio_eof.rs
5c67822b2ddf2d3a09a4f927035ce740d559ec657f072a95e7deb09c315cda5e  tests/windows_stdio_eof/io_threads.rs
03c2ef42a0e2bd0ee408c56133aa725c0b9aba0fe864562a637120eac49c3f19  tests/windows_stdio_eof/process.rs
d903dabec02fcba44afe2e0f50ab1c0f929a4953e3cf7c38c23d62b335fe76b0  examples/windows_stdio_eof_fixture.rs
440fd33ab4d4158607f0bc2aabe4f911033e8182be01dd3fdad473245e624715  examples/windows_stdio_eof_fixture/backend.rs
a4f392aca391b837d4026f2686ed9439d871a9c1ed16670e9badf9542411167c  examples/windows_stdio_eof_fixture/identity.rs
```

## 两个新exe冻结（精确compiler-artifact路径）

- **windows_stdio_eof**
  - `/tmp/windows-stdio-eof-sol-20260930.3wqefjmd/target/x86_64-pc-windows-msvc/debug/deps/windows_stdio_eof-e59ec0bdae7ce842.exe`
  - SHA256 `8d9de8721392531715fbd65f034f161753c8bc074b9ce2dfb8aa8a1bb72435a8`
  - 1487872 bytes

- **windows_stdio_eof_fixture**
  - `/tmp/windows-stdio-eof-sol-20260930.3wqefjmd/target/x86_64-pc-windows-msvc/debug/examples/windows_stdio_eof_fixture.exe`
  - SHA256 `1fb02451edce25dd2cf723095289848ced2b6e439ab7cd26aac82b60e3eb2c4a`
  - 11452928 bytes

artifact路径与完整compiler-artifact对象另存`artifacts.json`，哈希清单`artifacts.sha256`。Windows父测试实际运行时，BCrypt对显式fixture文件算SHA，与必填环境变量比较；child再次对`current_exe()`算SHA并报告。父自身也对current_exe算SHA。父读实际Child handle的image/PID/creation FILETIME/SessionId，子读自己的相同身份并交叉比较；argv nonce逐字echo，不靠临时filename猜路径。此代码已交叉编译，**这些Windows运行期校验尚未执行，待CC验证**。

## CC 无需临时编写harness的执行指令（作者未执行）

1. 协调者安排CC+GLM独立review本6文件与报告；核对root race freeze/本source manifest和两exe SHA。若重编译，必须NEW独占/tmp target，按完整root API manifest重新设BUILD_ID并构建两个target；不覆写本次或旧freeze。以此次JSON精确executable为准。
2. Windows创建NEW唯一owned TEMP目录，复制**上述两个exe**并校验Get-FileHash与artifact manifest一致。无需Host或任何临时Rust/Python/C#测试harness；父integration exe自带全部协议/子进程逻辑。可以同时使用已有冻结的`windows-test-runner.ps1`+`Runner.cs`作为外层有界执行器，按其已通过契约运行。
3. 在调用既有runner的同一个Windows父环境设置三个**必填**变量（runner创建的parent及其fixture继承环境）：

```powershell
$env:RPA_WINDOWS_STDIO_EOF_FIXTURE = '<复制后实际子fixture绝对路径>'
$env:RPA_WINDOWS_STDIO_EOF_FIXTURE_SHA256 = '1fb02451edce25dd2cf723095289848ced2b6e439ab7cd26aac82b60e3eb2c4a'
$env:RPA_WINDOWS_STDIO_EOF_EVIDENCE_DIR = '<本次已创建的owned evidence根绝对路径>'
```

   不要将fixture变量指向computer-host，不要猜Cargo临时filename。变量缺失、非绝对路径、文件非PE/hash不符、child身份/version不符都会错误退出，不skip。
4. 既有runner JSON只用其支持的5字段，不临时改runner：

```json
{
  "executablePath": "<复制后实际windows_stdio_eof integration exe绝对路径>",
  "workingDirectory": "<本次owned TEMP目录>",
  "evidenceDirectory": "<NEW唯一runner证据目录，按既有runner规则不可预存>",
  "arguments": ["--test-threads=1", "--nocapture"],
  "timeoutSeconds": 240
}
```

   用现有`windows-test-runner.ps1 -ConfigPath <config>`，不要创建新的测试harness。可先用独立config arguments=["--list"]（30s）确认恰4项；`--list`不验证环境/执行行为。全4项运行预期4 passed / 0 failed / 0 ignored / 0 filtered才可报新纯EOF GREEN。若协调者要求first-red即停，可逐项用 `["<上表完整test名>","--exact","--test-threads=1","--nocapture"]` +90s config，一项失败即停止剩余项；不改源码、不反复尝试到绿。
5. 保留runner summary的真实numeric native_exit_code（不是null/wrapper0）、PID/image/creation/session、stdout/stderr drain/timeout、两exe hashes；保留每case新目录的stdout.raw/stderr.raw/parent.jsonl/terminal.json。核对wire step reply有无、运行期cleanup与owner shutdown各计数、close→exit/press→cleanup的实际证据；mock held不能写成物理按键释放。
6. 任意Windows compile/run RED先保留first-red、精确断言/源码行与相关原始JSON，RCA交协调/root owner；不得擅改生产src、测试oracle或复用旧cancel9 GREEN掩盖。本轮没有制造已知假RED，也没有真实运行结果可供宣称。
7. GUI、OS input、网络/TCP、stdout断开继续deferred；不启动Host/DesktopBackend/窗口、不触碰用户桌面。原桌面授权没有变化。本报告冻结后停止写源码。

## Evidence manifest

目录：`.agents/runs/windows-stdio-eof-sol-20260930/`（全新，不覆盖旧freeze）。

- `source-freeze.sha256` SHA256：`900dbf8ef2fd3a7493644a8b4925cf3a93c307d1ff7faa6dd099546c2975b7fa`。
- `artifacts.sha256` SHA256：`f5b16ad2d3a566a0cc3425ae853d0c0457e75988054ab1ca42bc60be4391fda5`。
- `compile-evidence.sha256` SHA256：`5684e3291c872d06c5b0caa3afcf97ec91f47e19c1846c9a2fc312183cc73b5a`。
- 报告自己的hash单独存`report.sha256`，避开自引用。

**STOP / SOURCE_FROZEN。仅编译证据；新4项运行仍PENDING。**
