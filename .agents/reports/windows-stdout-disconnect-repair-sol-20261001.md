# stdout disconnect FIRST RED 修复交接 — sol / 20261001

**SOURCE_FROZEN / STOP。仅 test support 修复 + compile-only；修后 Windows 运行 PENDING，交协调 CC+GLM。** 无测试/--list/exe/SSH/GUI/Host/委派，无生产src/Cargo/root旧tests变更，无worktree/commit/push。

## RCA：确认与未确认
- 已读原 CC 报告及 raw idle summary/stdout/stderr、case-idle-panic parent.jsonl：native101，process.rs395 close/join 2秒超时，146ms close_requested 后无ACK。panic RAII随后关闭stdin、kill精确owned child，native1。active/旧4未跑；此RED不是生产stdout契约失败。
- **已确认类型不匹配**：本机 rustc 与安装manifest内rust-src/rust-std同为 **1.98.1 (48a229cea)**。`std/src/process.rs:411–414` → `sys/process/windows.rs:620–624` → `sys/process/windows/child_pipe.rs`：父端NtCreateNamedPipeFile为异步byte-stream（120–155），子端才设同步选项（178）；read为ReadFileEx（243–260），内部OVERLAPPED/APC + SleepEx等待（382–410）。不能把阻塞的Rust read API误认成同步Windows I/O。
- 此次已实际HTTP200获取微软CancelSynchronousIo/PeekNamedPipe官方正文，原HTML/解析文本/URL/摘要都封存 `primary-docs/`，匹配rust-src及manifest封存 `rust-src/`。不是引用协调消息或无正文web返回。前者针对同步pending I/O，与上述ReadFileEx不匹配；旧调用还忽略返回值。Peek的多线程同步handle阻塞警告确实存在，因此修复仅用于此匹配实现的异步父端，不泛称Peek永不阻塞。
- **未确认**：旧Cancel调用的实际BOOL/GetLastError、远端live stack未记录，不能写成观察到ERROR_NOT_FOUND/995。修复后的ACK、真实两stdout和旧4回归全部待CC运行；没有已证明生产缺陷。

## 最小修改
仅 `tests/windows_stdio_eof/io_threads.rs`。限定ChildStdout/ChildStderr（调用点仍为Stdio::piped的唯一读owner），Peek available，仅读取不超过现有字节的长度。空队列用可unpark的2ms park_timeout轮询，stop直接唤醒，不靠新增sleep延迟、stdin EOF或放大timeout；原join里的2ms轮询及2秒上限不变。

移除不匹配且忽略结果的CancelSynchronousIo；Peek的BOOL失败立即last_os_error（GetLastError），仅109为broken-pipe自然EOF；995及其它错误均Error，不因stop吞错。显式stop → 实际drop(pipe) → ParentClosed ACK → join约束不变，parent_closed_stdout不冒充out_eof。generic writer不再声称可被同步cancel，其异常回收仍由Process精确child cleanup后bounded join负责。

其余6源逐字未变；idle/active精确oracle、3000ms正常hold先Release再write检测、shutdown前cancel=false/后true、stdin保持至exit、无kill成功路径均不变。fixture0/70和Host0/7分离，非完整Host入口、OS输入、GUI或TCP测试。

## 编译/身份/旧证据
两命令exit0、compiler JSON success=true、无warning/error，限定7文件format check0；没有执行测试：
```sh
cargo test --locked --offline --no-run --test windows_stdio_eof --target x86_64-pc-windows-msvc --message-format=json
cargo build --locked --offline --example windows_stdio_eof_fixture --target x86_64-pc-windows-msvc --message-format=json
```
新target：`/tmp/windows-stdout-disconnect-repair-sol-20261001.v8dinh_9/target`。加载旧build-env立即覆盖CARGO_TARGET_DIR，CARGO_INCREMENTAL=0。

BUILD_ID：`e7de29ca2991f1c76a23a36d6132e295df8d9f1321efc3a25db8ce48e0a36500`（编译期RPA_STDIO_EOF_BUILD_ID，SHA256(product-before.sha256 bytes + compile-input-1.sha256 bytes)）；runtime version为`rpa-computer/0.1.0;windows-stdio-eof-v1;e7de29ca2991f1c76a23a36d6132e295df8d9f1321efc3a25db8ce48e0a36500`。

生产publicAPI仍是旧冻结基线；product-before/after SHA均`315fa567b3cbe74b0ef2bc9a71323071b0b6963b73dab5c3d7fda9237cb9bf7f`，146条最终核对不变，无观察到root中途变化影响compile。原始RED及旧EOF/stdout报告/证据/旧exe共85文件由prior-evidence.sha256固定且复核不变；旧7源码全文在before/、新7全文在source/。旧*.before及旧exe未覆盖。

证据目录：`.agents/runs/windows-stdout-disconnect-repair-sol-20261001/`。
- `source-freeze.sha256` SHA256 `8033e5c5a88591fbaf15bba13edae5721a9c73a157afc558799c29cf2b0e073f`
- `artifacts.sha256` SHA256 `0dc1ae02caafae1f80fadf85e1e9e2a731a1ae6d60abf22844a3b90648deea42`
- `compile-evidence.sha256` SHA256 `dbba2cf69e27ceddceff8c0e84f8f12d6ccec3cccd57a89fa93dbb8ec8139a02`

两个新exe（来自精确Cargo compiler-artifact，不猜文件名）：
- `windows_stdio_eof`：`/tmp/windows-stdout-disconnect-repair-sol-20261001.v8dinh_9/target/x86_64-pc-windows-msvc/debug/deps/windows_stdio_eof-e59ec0bdae7ce842.exe`
  SHA256 `708998673f524067ecb67d09ccca5da4060f0eabf96066246a3e9fa3d71404db`
- `windows_stdio_eof_fixture`：`/tmp/windows-stdout-disconnect-repair-sol-20261001.v8dinh_9/target/x86_64-pc-windows-msvc/debug/examples/windows_stdio_eof_fixture.exe`
  SHA256 `d6fd376b30f2cb552500287254fa6847bb113e23d7a39622fc70b31cc2c1584c`

## CC精确执行（作者未执行）
复制上面两exe到 NEW owned Windows目录，原样使用既有scripts/windows-test-runner.ps1及Runner.cs；不写新harness。设置三个必填变量，缺失应失败：
```powershell
$Root = 'C:\Windows\Temp\windows-stdout-disconnect-repair-cc-20261001' # NEW owned目录
$Parent = Join-Path $Root 'windows_stdio_eof.exe'
$env:RPA_WINDOWS_STDIO_EOF_FIXTURE = Join-Path $Root 'windows_stdio_eof_fixture.exe'
$env:RPA_WINDOWS_STDIO_EOF_FIXTURE_SHA256 = 'd6fd376b30f2cb552500287254fa6847bb113e23d7a39622fc70b31cc2c1584c'
$env:RPA_WINDOWS_STDIO_EOF_EVIDENCE_DIR = Join-Path $Root 'case-evidence'
New-Item -ItemType Directory $env:RPA_WINDOWS_STDIO_EOF_EVIDENCE_DIR -ErrorAction Stop | Out-Null
if ((Get-FileHash -Algorithm SHA256 $Parent).Hash.ToLowerInvariant() -ne '708998673f524067ecb67d09ccca5da4060f0eabf96066246a3e9fa3d71404db') { throw 'parent hash mismatch' }
if ((Get-FileHash -Algorithm SHA256 $env:RPA_WINDOWS_STDIO_EOF_FIXTURE).Hash.ToLowerInvariant() -ne $env:RPA_WINDOWS_STDIO_EOF_FIXTURE_SHA256) { throw 'fixture hash mismatch' }
$Cases = @(
 'stdout_disconnect::idle_stdout_disconnect_then_ping_returns_and_owner_shuts_down',
 'stdout_disconnect::active_stdout_disconnect_finishes_normal_hold_before_write_detection',
 'idle_stdin_eof_shuts_down_real_stdio_worker',
 'active_hold_stdin_eof_cancels_after_successful_press',
 'partial_frame_idle_eof_never_dispatches_input',
 'partial_frame_during_hold_eof_cancels_active_action'
)
for ($i=0; $i -lt $Cases.Count; $i++) {
 $Config = Join-Path $Root ('case-{0}.json' -f $i)
 @{ executablePath=$Parent; workingDirectory=$Root; evidenceDirectory=(Join-Path $Root ('runner-{0}' -f $i)); arguments=@($Cases[$i],'--exact','--test-threads=1','--nocapture'); timeoutSeconds=90 } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $Config
 & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root 'scripts\windows-test-runner.ps1') -ConfigPath $Config
 if ($LASTEXITCODE -ne 0) { throw "first-red: $($Cases[$i]); preserve evidence and stop" }
}
```
Session0纯console，逐项1 passed/5 filtered。重点保存close ACK、无stdin提前关闭、Press/Release、before/after owner cancel/held、native exit、无kill、drain/join以及identity/hash/nonce。首败保留原始RED+RCA，不修生产、不放宽oracle。运行期不设置编译BUILD_ID；child参数仍由parent传入`--stdio-eof-fixture-v1 --nonce <owned nonce>`。

报告hash单独report.sha256。**SOURCE_FROZEN，停止源码写入；修复效果待真实Windows验证。**
