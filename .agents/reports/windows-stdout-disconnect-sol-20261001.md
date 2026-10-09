# Windows stdout disconnect Stage A — 20261001

**SOURCE_FROZEN / STOP。仅源码与交叉编译完成；新2项和本次binary旧4项运行PENDING，交协调CC+GLM。** 未运行测试/--list/exe/SSH/GUI/Host，未委派，未改生产src/Cargo/旧root测试，无worktree/commit/push。不再写源码。

## 规格与边界
- idle：真实JSON-RPC handshake/open/observe → 父唯一stdout读句柄实际drop + ACK/join → stdin保持打开，ping id=5触发生产write → stdio返回 → owner shutdown → child exit后才关stdin。
- active：mock Press后关stdout，QPC确认close ACK在真实3000ms hold内（低于5000ms input budget）。自然完成恰Press/Release且cancel=false，随后reply write才发现断开，不要求无write时立即检测。
- 两项精确oracle：shutdown前cancel=false/generation=0/held=[]/cleanup=[]；之后cancel=true/generation=1/held=[]、release_all恰一次、Clean、不fault、后续生产call Dead、fixture native exit0。正常用例不容许deadline/fault替代结果。
- 真实公开stdio::run + Worker/BackendFactory::Test；无Cursor/直接handle_eof/父cancel/release指令。reader取消阻塞read后实际drop唯一ChildStdout再ACK，父bounded join。`parent_closed_stdout=true`、`out_eof/stdout_eof=false`，不伪造自然EOF；stdin保持至child退出。成功无kill，异常只精确owned Child RAII。
- stderr/owned files诊断；stdout只断点前3个完整JSON-RPC回复。write错误在公开API未暴露，只记录`write_failure_inference`与ACK/trigger/return/无EOF或fault依据，不伪造OS错误码。保留parent.jsonl、terminal.json、raw stdout/stderr、nonce/identity/hash/version、stdin关闭时序、父线程drain/join。
- fixture直接Worker::shutdown；Host先cancel.cancel再shutdown，故非完整Host入口测试。fixture成功0/错误70与Host0/7分离；父native exit另存。child生产reader无公开JoinHandle，不声称逐线程join，仅证明父owned readers joined与child退出。
- 按现有生产源码预期通过，尚无已证明需改生产的缺陷。首败保留RED+RCA交回，不改生产/放宽oracle。真实pipe+mock input不等于OS输入/GUI/TCP；无write即时断开检测不覆盖。

## 编译与冻结
两个命令exit0，compiler JSON success=true、无warning/error；限定源码rustfmt check0。没有执行测试。
```sh
cargo test --locked --offline --no-run --test windows_stdio_eof --target x86_64-pc-windows-msvc --message-format=json
cargo build --locked --offline --example windows_stdio_eof_fixture --target x86_64-pc-windows-msvc --message-format=json
```

新target：`/tmp/windows-stdout-disconnect-sol-20261001.nxhzg5eh/target`；加载旧build-env立即覆盖target，CARGO_INCREMENTAL=0。rustc 1.98.1，交叉目标x86_64-pc-windows-msvc。

BUILD_ID：`f61a26fe56a5ae7f48707be11d4b1663124ea6f4422d47f1ef7101b2c7f93472`（编译期RPA_STDIO_EOF_BUILD_ID；SHA256(product-before.sha256 bytes + compile-input-1.sha256 bytes)）。runtime version=`rpa-computer/0.1.0;windows-stdio-eof-v1;<BUILD_ID>`，运行不需设置编译变量。

Public API基于product-before/after.sha256，二者相同且最终146条核对一致，manifest SHA `315fa567b3cbe74b0ef2bc9a71323071b0b6963b73dab5c3d7fda9237cb9bf7f`（同旧EOF root API基线）；未观察到root文件中途变化影响compile。未来变化不由冻结结论背书。旧4函数体逐字保留，共享support additive扩展；旧6源码归档*.before，旧证据/旧两exe核对未变，旧manifest不再代表当前扩展源码字节。

证据目录：`.agents/runs/windows-stdout-disconnect-sol-20261001/`（完整command/jsonl/stderr/exit/elapsed、旧源码归档、diff、生产基线）。
- `source-freeze.sha256` SHA256：`a2b940374669c9637cd6dc3e318aff3df1b398944afbd3d4d9eb56c8ccbf7714`
- `artifacts.sha256` SHA256：`5690eac66f2206cb3cbf6346a15264058190a651e98e4bd2036be447b9cd8b78`
- `compile-evidence.sha256` SHA256：`fc00bc44d67769ca063f9dc26300886bea7861df48565911db6130bf83462ae1`

源码写集及精确hash（仅自有7文件）：
```text
10da320aefa473aad6507f4a99344bc20df83e6d145b2aa388eaeb9365b5b58f  tests/windows_stdio_eof.rs
f541235e3697131e4b5eca95966feddb44e03119fcd5e6d02a68b65bb0b3cc78  tests/windows_stdio_eof/io_threads.rs
e6a157697d3c134036d76db977c4c5aa950950911f1e99eee3805b8e3f6c12d0  tests/windows_stdio_eof/process.rs
f633a5915ac58facfcbf7a06025ffd5d42c4e850e03beae89c10b99dfed6be50  tests/windows_stdio_eof/stdout_disconnect.rs
dd9ff99252f01e4559747a7f559df466d5bbc71c59a9ec60a564b0329b9c522e  examples/windows_stdio_eof_fixture.rs
c4e4ad4e91de2759e2d1909dca23b71381df20616f84b32966d7e422c4a40534  examples/windows_stdio_eof_fixture/backend.rs
2d9cdcbf896bc88acaf958ce1be0b684358d260c940215b4dfdd9328016eeedf  examples/windows_stdio_eof_fixture/identity.rs
```

两exe由Cargo JSON精确target取得（不glob猜名）：
- `windows_stdio_eof`：`/tmp/windows-stdout-disconnect-sol-20261001.nxhzg5eh/target/x86_64-pc-windows-msvc/debug/deps/windows_stdio_eof-e59ec0bdae7ce842.exe`
  SHA256 `5f27aaec64cec5e4c267d58287a58aca2c2c70954f2114a5aafb46d64656477f`
- `windows_stdio_eof_fixture`：`/tmp/windows-stdout-disconnect-sol-20261001.nxhzg5eh/target/x86_64-pc-windows-msvc/debug/examples/windows_stdio_eof_fixture.exe`
  SHA256 `085df7dae869335027bd179f092b8fc3fc27242d507269791f674f7ae4ac20c4`

## CC精确清单/命令（作者未执行）
以下$Cases依次为新2项+受影响原4项完整名。按该明确布局复制两个冻结exe、既有scripts/windows-test-runner.ps1及scripts/windows-test-runner/Runner.cs；路径是CC新建目的地，不假定已存在。只用runner现有5字段config，无临时harness。三个环境变量必填，缺失失败不skip；parent自动传child `--stdio-eof-fixture-v1 --nonce <owned nonce>`。

```powershell
$Root = 'C:\Windows\Temp\windows-stdout-disconnect-20261001' # NEW owned目录；如已存在须换唯一目录
$Parent = Join-Path $Root 'windows_stdio_eof.exe'
$Fixture = Join-Path $Root 'windows_stdio_eof_fixture.exe'
$env:RPA_WINDOWS_STDIO_EOF_FIXTURE = $Fixture
$env:RPA_WINDOWS_STDIO_EOF_FIXTURE_SHA256 = '085df7dae869335027bd179f092b8fc3fc27242d507269791f674f7ae4ac20c4'
$env:RPA_WINDOWS_STDIO_EOF_EVIDENCE_DIR = Join-Path $Root 'case-evidence'
New-Item -ItemType Directory -Path $env:RPA_WINDOWS_STDIO_EOF_EVIDENCE_DIR -ErrorAction Stop | Out-Null
if ((Get-FileHash -Algorithm SHA256 $Parent).Hash.ToLowerInvariant() -ne '5f27aaec64cec5e4c267d58287a58aca2c2c70954f2114a5aafb46d64656477f') { throw 'parent hash mismatch' }
if ((Get-FileHash -Algorithm SHA256 $Fixture).Hash.ToLowerInvariant() -ne $env:RPA_WINDOWS_STDIO_EOF_FIXTURE_SHA256) { throw 'fixture hash mismatch' }
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
 @{
  executablePath=$Parent
  workingDirectory=$Root
  evidenceDirectory=(Join-Path $Root ('runner-{0}' -f $i)) # NEW，不预创建
  arguments=@($Cases[$i], '--exact', '--test-threads=1', '--nocapture')
  timeoutSeconds=90
 } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $Config
 & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root 'scripts\windows-test-runner.ps1') -ConfigPath $Config
 if ($LASTEXITCODE -ne 0) { throw "first-red case=$($Cases[$i]); preserve evidence and stop" }
}
```
逐项预期1 passed/5 filtered；first-red停止。若协调组合：新2 filter `stdout_disconnect:: --test-threads=1 --nocapture`，预期2 passed/4 filtered；全部6用 `--test-threads=1 --nocapture`，runner timeout240，预期6 passed/0 failed/0 ignored。

CC保存真实numeric native exit与完整case证据，核对stdout ACK、正常Press/Release、owner前后held/cancel、exit时stdin_still_open、随后after_stdout_child_exit、无kill、readers_joined。外层强杀parent绕过Rust RAII，按owned身份核残留，不namekill。Session0纯console，不启动Host/GUI/网络。

报告SHA另存同目录report.sha256，避免自引用。**SOURCE_FROZEN，停止源码写入；Windows运行PENDING。**
