# Windows TCP disconnect — sol implementation / 20261001

**SOURCE_FROZEN / STOP。4项实现及Windows交叉编译完成；独立spec/quality gate与真实运行全部PENDING，交协调CC+GLM。** 作者按指定gpt-6.1-sol角色工作，当前接口未提供可独立核验的底层模型ID，不伪报路由。没有测试/--list/exe/SSH/GUI/应用/socket行为运行，没有委派或重复CC preflight review；无worktree/commit/push，父目录项目未动。

## 隔离与最小规格
- 新integration父进程用真实rustls ClientConnection/TcpStream；新fixture supervisor调用公开`remote::host::run`，生产代码`current_exe()`真实派生同一fixture的**唯一`--mock-backend`子路由**。该路由仅构造`BackendFactory::Test`、公开`stdio::run`、真实Worker；不是产品Host入口，不构造DesktopBackend/锁/hotkey/renderer，不注入OS输入。FeedbackConfig::default保持disabled。采用批准的Test seam，不采纳preflight里误写的“只能Mock”。
- 仅固定127.0.0.1临时端口：fixture先reserve 127.0.0.1:0再交生产bind。交接gap不假称无竞争；必须看到生产成功listening日志且GetExtendedTcpTable核验唯一listener owning PID为确切supervisor，才允许连接。绑定/身份不明确fail closed。TLS私有测试CA验证localhost，不用insecure verifier。
- 父integration才嵌入既有throwaway cert/key/token（不含CA私钥），运行时写到case目录外的owned assets目录；fixture exe不嵌入TLS材料。仅supervisor argv获得cert/key/token文件路径；生产子argv严格只有--mock-backend，未通过env传TLS材料。继承的test env仅诊断目录/nonce/supervisor身份等非秘密信息；同一OS账户文件权限不声称安全隔离。token不入日志。
- 父supervisor-control stdin从spawn保持到supervisor自然exit；它不用于网络断开或解阻塞。独立owned `stop.request.json`控制通道只有在目标及重连child的终态、真实native0、生产reap日志全部记录后才写入。supervisor atomic flag之前始终false；测试不调用cancel或handle_eof。child的external stdio shutdown flag永远不写。

| 精确case名 | 契约 |
|---|---|
| `idle_tcp_eof_reaps_child_and_reconnects` | auth/initialize/open/observe后真实shutdown+drop socket，无TLS close_notify。生产日志必须出现remote transport Failed；child reader EOF取消、自然退出0，再重连。 |
| `active_tcp_eof_cancels_hold_and_reconnects` | 5000ms mock Shift hold，owned文件确认真实成功Press后才raw cut；runtime cleanup距真实Press <3000ms，不能把自然hold完成当取消。 |
| `active_tls_close_notify_cancels_hold_and_reconnects` | 同样Press barrier，但发送并flush真实TLS close_notify；保留socket并drain至服务端close/EOF，证明不是用父控制stdin触发。 |
| `incomplete_json_tls_close_cancels_hold_without_dispatch` | active中发送真正截断到arguments冒号的JSON尾巴，再close_notify。生产clean bridge会补newline，仍不是有效JSON，不能新增input；parse-error/reply允许被EOF优先控制抢先，不伪造必须到达的reply。 |

四项均在生产reap后重新TLS认证，必须新PID/creation identity（旧exact handle仍保留），initialize/open/observe/close成功后自然reap第二child。仅生产busy交接期间**尚未认证的TLS EOF/reset**可在有界窗口重试；认证/RPC失败不重试、不碰绿。

### Oracle是当前源码行为，不提高生产契约
raw TCP EOF在pump中是Failed，不是clean；clean close_notify走ClosedByPeer并补newline转发尾帧，fatal路径丢尾帧。本批partial有意选择clean分支，不宣称另测了fatal partial。

当前execute.rs:128–132/152–156在loop head或Sleep中取消直接break到278–285的release_all；并不走deadline/error的计划Release pass。因此健康active精确要求：events仅`Shift Press(cancelled=false)`；runtime release_all恰一次、cancelled=true、held_before=[shift]→held_after=[]、since_press_ms<3000；owner shutdown再cleanup一次，不新增业务事件。以生产read-only `computer_get_step` ledger确认partial/cancelled/error=cancelled/events_completed=[0]/events_total=2/cleanup=released，而非写一个success字段代替结果。lookup在stdio返回后、owner shutdown前，仅用父从实际open reply获得的session_id/request_id；前后trace必须不变。正常5000ms结束/deadline/fault不是可接受替代结果。

idle断开前没有input/cleanup，owner cleanup恰一次。所有目标要求stdio Clean、reader cancel在owner shutdown前已true、generation>0、external flag=false；shutdown Clean、不fault、post-call Dead、held空，after-owner cancel仍true且generation增加。fixture owner直接Worker::shutdown，不是完整Host main。

## 生命周期/证据/首跑界限
- child owned JSON：ready（nonce/build/exe SHA/PID/creation/session/supervisor/argv）、dispatch-N、cleanup-N（QPC及实际backend trace）、step-context（只读lookup标识）、terminal（真实stdio/Worker/ledger及owner前后状态）。atomic publish不覆盖文件。父parent.jsonl记录身份、握手帧、cut前后QPC、cleanup、原生产reap日志与独立GetExitCodeProcess native0、最后才发control stop。
- supervisor stdout/stderr直接继承owned文件；没有复制旧stdio reader或新建阻塞drain线程。父文件读取有1MiB cap；TLS plaintext累计1MiB cap、非阻塞读写；JSON证据128KiB cap。TLS clean路径显式drain，raw断开的不可达reply不假称收到。成功末尾记录文件长度；test-owned control/watchdog threads实际join。
- 生产bridge顺序为pump.finish→out/in joins→reap；5s grace从reap进入才开始，不从cut/child stdin EOF开始。本测试target终态+native退出+生产reap等待上限20s，独立active runtime cleanup上限3s，避免较宽reap等待掩盖5000ms正常hold。生产内部JoinHandle/Outcome不公开；不声称逐一观察到了所有生产线程join或原始pump enum。
- 启动ready 8s/listening 5s；TCP connect单次2s，TLS handshake/单次flush/line绝对5s；worker ready5s、dispatch evidence8s；reconnect readiness窗口8s只在失败间检查，可能另加当前最多7s连接尝试；clean tail drain3s，最后supervisor stop8s。fixture每个进程自有75s hard watchdog，触发写marker并native71，只是失败保护，不设置cancel/shutdown让测试假绿。CC outer runner每case120s、--list30s。
- 正常路径强杀FAIL：retained exact Child handle用于supervisor；worker PID来自owned ready，OpenProcess后先逐项核验creation/image/session才保留handle。自然退出用WaitForSingleObject/GetExitCodeProcess，reap另核生产peer-specific日志；listener admission与child exit不互相冒充。错误RAII只清理这些确切句柄，禁止global/name kill。尚未claim的失败启动后代明确unknown（各fixture self-watchdog限时），不假称runner提供进程树containment。外层强杀parent会绕过Rust RAII，CC仍需按owned身份核残留。
- fixture自然0/错误70/watchdog71；测试失败libtest native101；失败cleanup对verified worker的TerminateProcess自用72。这些不等于产品Host的7。本批不证明GUI/OS held释放/完整Host网络授权；任何Windows权限提示应停止，不点授权或改规则。

## 写集与编译冻结
仅新增10源文件（如下）；复用的旧identity/backend只读，无复制大模块。新最大模块process.rs400行。现有src/Cargo/stdio/旧tests/runner共253份before/after摘要一致，未观察到共享publicAPI中途变更影响compile。当前API以dependencies-freeze.sha256标识（153依赖：本地产品源码/Cargo锁、复用2 helper、5证书/token、2 runner源），不借旧Windows GREEN代表本批通过。

- `examples/windows_tcp_disconnect_fixture/common.rs`
- `examples/windows_tcp_disconnect_fixture/supervisor.rs`
- `examples/windows_tcp_disconnect_fixture/worker.rs`
- `examples/windows_tcp_disconnect_fixture.rs`
- `tests/windows_tcp_disconnect/assets.rs`
- `tests/windows_tcp_disconnect/oracle.rs`
- `tests/windows_tcp_disconnect/owned.rs`
- `tests/windows_tcp_disconnect/peer.rs`
- `tests/windows_tcp_disconnect/process.rs`
- `tests/windows_tcp_disconnect.rs`

```sh
cargo test --locked --offline --no-run --test windows_tcp_disconnect --target x86_64-pc-windows-msvc --message-format=json
cargo build --locked --offline --example windows_tcp_disconnect_fixture --target x86_64-pc-windows-msvc --message-format=json
```
最终integration-3 / fixture-3均exit0、compiler JSON success=true、无warning/error；rustfmt check0。第一轮fixture E0277（TcpError未实现Error）及全文/源码已保留，修在新supervisor的错误转换；第二轮已compile0，第三轮是上述执行器oracle校准后的最终重编。没有真实运行RED或GREEN，未运行--list。

工具链详见rustc-version.txt；NEW target `/tmp/windows-tcp-disconnect-sol-20261001.fymcbg4l/target`。加载既有build-env后立即覆盖target，CARGO_INCREMENTAL=0，未覆盖旧target/产物。

BUILD_ID：`49f66fa2d2db6176aedda7f9977c6c7427ae997fba6d39156055ee4a2ad70a56`；编译期变量`RPA_TCP_DISCONNECT_BUILD_ID`。算法SHA256(source-input-3.sha256 bytes + dependencies-input-1.sha256 bytes)。runtime version为`rpa-computer/0.1.0;windows-tcp-disconnect-v1;49f66fa2d2db6176aedda7f9977c6c7427ae997fba6d39156055ee4a2ad70a56`。运行期不需设置编译变量。

**windows_tcp_disconnect**
- `/tmp/windows-tcp-disconnect-sol-20261001.fymcbg4l/target/x86_64-pc-windows-msvc/debug/deps/windows_tcp_disconnect-dfe12a12c9cdde2c.exe`
- SHA256 `8bcefd4729a273eb4a27848d7ddf2a9da3b182c7e4a896fc868665708d30b004`
**windows_tcp_disconnect_fixture**
- `/tmp/windows-tcp-disconnect-sol-20261001.fymcbg4l/target/x86_64-pc-windows-msvc/debug/examples/windows_tcp_disconnect_fixture.exe`
- SHA256 `80f15b0b7b109cf2b2570005536e56011b559176494b29cf955d7f9af4bfcc24`

证据目录：`.agents/runs/windows-tcp-disconnect-sol-20261001/`；source/保存完整源码与依赖，attempt-1/2-source及所有compile日志保留。
- `source-freeze.sha256` SHA256 `cd4e410a7f9bc58e70e330dff4621916c50d3930a7b69be83a9b690174e02b44`
- `dependencies-freeze.sha256` SHA256 `15ea11345dca0555994bc1090bf3e63e9854a7dde2f15d1a16c6d26d4a4db71a`
- `artifacts.sha256` SHA256 `c1d6f3f8c188e9f8ba342799aca6ae37a61e7eae086ec316253c9df97767bafb`
- `compile-evidence.sha256` SHA256 `23f3fa25fe2208181e5970676996655dcd0d076bd04fee640d91ab37e9a28f84`

## CC部署与精确命令（作者未执行）
先最终spec/quality gate，再复制两exe和冻结runner到NEW owned目录下所示布局；只有parent嵌入测试证书，CC不需制作harness/手写fixture。三个变量均必填，缺失失败不skip。先--list核4项，再逐项执行；首个native失败停止并保留原始证据，不反复重跑、不修改生产。
```powershell
$Root = 'C:\Windows\Temp\windows-tcp-disconnect-cc-20261001' # NEW；若已存在换唯一目录
$Parent = Join-Path $Root 'windows_tcp_disconnect.exe'
$env:RPA_WINDOWS_TCP_FIXTURE = Join-Path $Root 'windows_tcp_disconnect_fixture.exe'
$env:RPA_WINDOWS_TCP_FIXTURE_SHA256 = '80f15b0b7b109cf2b2570005536e56011b559176494b29cf955d7f9af4bfcc24'
$env:RPA_WINDOWS_TCP_EVIDENCE_DIR = Join-Path $Root 'case-evidence'
New-Item -ItemType Directory $env:RPA_WINDOWS_TCP_EVIDENCE_DIR -ErrorAction Stop | Out-Null
if ((Get-FileHash -Algorithm SHA256 $Parent).Hash.ToLowerInvariant() -ne '8bcefd4729a273eb4a27848d7ddf2a9da3b182c7e4a896fc868665708d30b004') { throw 'parent hash mismatch' }
if ((Get-FileHash -Algorithm SHA256 $env:RPA_WINDOWS_TCP_FIXTURE).Hash.ToLowerInvariant() -ne $env:RPA_WINDOWS_TCP_FIXTURE_SHA256) { throw 'fixture hash mismatch' }
$Cases = @('--list',
 'idle_tcp_eof_reaps_child_and_reconnects',
 'active_tcp_eof_cancels_hold_and_reconnects',
 'active_tls_close_notify_cancels_hold_and_reconnects',
 'incomplete_json_tls_close_cancels_hold_without_dispatch'
)
for ($i=0; $i -lt $Cases.Count; $i++) {
 $Argv = if ($i -eq 0) { @('--list') } else { @($Cases[$i],'--exact','--test-threads=1','--nocapture') }
 $Limit = if ($i -eq 0) { 30 } else { 120 }
 $Config = Join-Path $Root ('case-{0}.json' -f $i)
 @{executablePath=$Parent; workingDirectory=$Root; evidenceDirectory=(Join-Path $Root ('runner-{0}' -f $i)); arguments=$Argv; timeoutSeconds=$Limit} | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $Config
 & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root 'scripts\windows-test-runner.ps1') -ConfigPath $Config
 if ($LASTEXITCODE -ne 0) { throw "FIRST RED: $($Cases[$i]); preserve evidence and stop" }
}
```
预期list=4 tests，逐项1 passed/3 filtered/0 ignored；这只是待验收规格，不是已观察结果。原样收回runner summary/native exit与stdout/stderr、全部case owned JSON和supervisor原始日志，核对watchdog/kill标记、native0/reap、身份变化、shutdown请求晚于reap。Session0纯console，只回环无外网/防火墙变更，不启动desktop Host，保护既有桌面进程。

报告摘要另存report.sha256。**STOP / SOURCE_FROZEN，后续真实运行与独立review只由协调CC+GLM安排。**
