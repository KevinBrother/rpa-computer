# Windows cancellation contract — CC independent verification (2026-09-30)

状态：**GREEN_CONFIRMED（纯契约范围，一次调用成功）**。 acer-win Session 0 经批准 bounded runner：`--list` 恰 9 项、native exit=0；`--test-threads=1 --nocapture` **9 passed / 0 failed / 0 ignored / 0 measured / 0 filtered，native exit=0**，2.97s，无超时。此 GREEN 仅覆盖纯契约（synthetic in-memory Backend）；**cancel07 真实网络断连、真实 stdio pipe/socket EOF reader、全部真实 GUI held/release/focus 仍全部 deferred，不得据此宣称 allcancel 完成。**

## 协调者 override 遵守情况

- **未重编译、未触碰 root**。复用作者（Codex sol）已链接的 immutable pre-token-repair 产物；本 run 验证的是该冻结产物，不是最新 root。
- 产物核验（Mac 侧 + acer-win 上传后 `Get-FileHash` 双侧一致）：
  - exe：`.agents/runs/windows-cancel-contract-sol-20260930/target/x86_64-pc-windows-msvc/debug/deps/windows_cancel_contract-7e9ced150c16f50d.exe`
  - SHA256：`7dbbbedabaa4098d67348ff1681d73a9772a2bb193eabbe0de32aacafc6ce830`（与冻结报告逐字一致）
  - 大小：`11896320` bytes（一致）
- 自有 3 条 source hash 全部与冻结报告一致（复核于本 run 开始时）：
  ```text
  70db6724efaa4c55ac70e27b829522810cdf1dca03ed273a3d2be900a19dcbc0  tests/windows_cancel_contract.rs
  411efdce9e8f642e90cded9ded5468bffd57f29a6da1398eea5d533759e81a8c  tests/windows_cancel_contract/support.rs
  48e8b94765c5075abeca7f277b84b96fc6eca71f87b8ac4a12440a4ccf0956e1  docs/windows-cancel-contract-matrix.md
  ```
- 静态纯度核对：`#[test]` 恰 9 个（cancel01..06、08..10，无 07 占位），测试源中无 `TcpStream/UdpSocket/std::net/process::Command/DesktopBackend/SendInput`。runner 文件 hash 亦与批准冻结一致：`windows-test-runner.ps1` `ad2a607c…9572c4`、`Runner.cs` `aced18e6…722214`（runner 运行时自核 `runner_source_sha256`/`runner_module_sha256` 同值）。
- **基线声明**：本证据绑定上述 pre-repair 产物 hash。root owner 后续修复 feedback token 生成 bug 后，本 target 需重新编译并再次回归；本 GREEN 不自动适用于修复后的版本。

## 执行环境（全部保留，未删）

- 目标机：acer-win（SSH BatchMode，SessionId=0）。全新 TEMP：`C:\Users\Administrator\AppData\Local\Temp\windows-cancel-contract-cc-20260930-214340-63618914`，正确嵌套布局 `scripts\windows-test-runner.ps1` + `scripts\windows-test-runner\Runner.cs` + `windows_cancel_contract.exe` + on-disk `invocation1.ps1`。
- 证据父目录 `evidence\` 预创建，leaf（`evidence\list-01`、`evidence\run-01`）由 runner 原子保留（CreateDirectoryW，"no overwrite" 校验通过）。每条命令独立 evidenceDir。
- config 路径全部正斜杠驱动器绝对路径；config sha256：list `2ee70468d9729c118ea083ee20bc69413e16167219cd81f534465036853e8029`、run `859825319b5dbd0d38d3b03f624007834b162ee6d777587594a9633c42e31c98`。
- Mac 侧证据：`.agents/runs/windows-cancel-contract-cc-20260930/`（invocation1.ps1、invocation1.output/.ssh.stderr、tempdir、upload-*.stderr、config-list-01.json、config-run-01.json、evidence-list-01/、evidence-run-01/、residual-check.output）。

## --list（runner 证据 `evidence\list-01`）

- root_pid=20832，root_start_utc=2026-09-30T13:44:35.66Z，root_session_id=0（Process.SessionId），executable_sha256 同冻结值。
- **native_exit_code=0**（整数，非 null coercion）；outcome=success；timed_out=false；cleanup_state=root_exited；harness_exit_code=0。
- stdout（501 bytes）：恰 9 行 `cancel01_drag_pressed_then_pause … cancel10_resume_requires_fresh_observation_without_replay` + `9 tests, 0 benchmarks`；名称与矩阵逐字一致；stderr 0 bytes。

## run（runner 证据 `evidence\run-01`，`--test-threads=1 --nocapture`，timeoutSeconds=120）

- root_pid=21104，root_start_utc=2026-09-30T13:44:36.19Z，root_session_id=0；runner 内部 120s 上限未触发；Mac 外层监督实测全程 6s（13:44:33Z→13:44:39Z）。
- **native_exit_code=0**；outcome=success；timed_out=false；cleanup_state=root_exited（自然退出，未 kill）；harness_exit_code=0。
- stdout：`9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.97s`，9 项逐个 `ok`。
- stderr（172 bytes）仅一行，即 cancel08 预期注入的 synthetic 故障日志：`[computer-host] shutdown reported cleanup problems: {"cleanup_outcome":"failed","error":{"code":"input_error","message":"synthetic release_all failure"},"state":"faulted"}` —— 与矩阵 cancel08 oracle（精确 `input_error`、faulted shutdown、`worker_faulted`）一致，非意外失败。
- 无首红：无失败断言，无 first-fail file/line 可报。

## 超时/残留核对（read-only，未 kill）

- 无超时：两 summary 均 `timed_out=false`、`cleanup_state=root_exited`。
- `residual-check.output`：`Win32_Process` 中无 `windows_cancel_contract.exe` 进程；按 staging TEMP 目录名匹配 commandline 无任何命中（连自匹配瞬态进程也未出现，SSH 会话已退出）；**Notepad PID 24332 仍在运行，未触碰**。未执行任何 kill，无全局/按名清理。

## Attempt 记录

- 第 1 次上传 invocation1.ps1 后 PowerShell 解析失败（嵌套子表达式语法，**未启动任何测试进程**），修正为显式循环拼接后第 2 次上传一次成功。两次失败/成功输出全部保留（`invocation1.output` 为成功 attempt；解析错误原文保留于本文件历史，最终 `invocation1.output` 仅含成功运行）。无隐藏重试、无断言改动。

## 边界声明（不可放宽）

1. 9 项通过 = **纯契约 GREEN only**：内存 Backend、synthetic held state，不证明 Windows SendInput 物理释放或真实桌面行为。
2. cancel07（真实网络 disconnect）、真实 stdio pipe/file EOF reader 端到端（cancel06 只是生产 `McpService::handle_eof` handler + 显式 Worker shutdown continuation，非真实 reader EOF）、全部真实 GUI held/release/focus 恢复 —— **均 deferred**。
3. runner 无后代进程 containment（`descendant_containment=none`）；本 run 未产生存活后代。
4. 本证据仅适用于 SHA256 `7dbbbe…e830` 的 pre-repair 产物；product token 修复后必须重新编译回归。
