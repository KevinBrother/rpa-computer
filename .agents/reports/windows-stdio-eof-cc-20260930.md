# Windows 真实匿名管道 stdio EOF — CC 独立复验

日期：2026-09-30。执行者：CC（Claude），Windows-only Session 0，纯 console 父进程+owned 子进程。依据 SOURCE_FROZEN 报告 `.agents/reports/windows-stdio-eof-sol-20260930.md`。

## 结论

**4/4 新 integration 全部一次通过，native exit 0。**实际生产 stdin reader 的 EOF 取消路径已验证（synthetic backend），非 handle_eof/内存替身。--list 恰 4 项。无失败尝试。

| 运行 | 结果 | native exit |
|---|---|---|
| `--list` | `4 tests, 0 benchmarks`，恰 4 个 `windows_stdio_eof::*` | 0 |
| 默认 suite（`--test-threads=1 --nocapture`，timeout 240） | **4 passed / 0 failed / 0 ignored / 0 filtered**，1.38s，wall 15:29:10.84→15:29:12.24 UTC | **0** |

## 静态 review（独立，未轻信作者声明）

逐条核过 6 个冻结文件：

1. **子进程真实生产路径**：`Worker::start(BackendFactory::Test(...))` + 公开 `stdio::run(&worker, version, shutdown_flag)`，读写继承的真实 OS stdin/stdout；无 handle_eof/run_with_reader、无 Cursor transport（Cursor 仅用于内存 PNG）、无父 release 指令/barrier/cancel.store。fixture 的 `worker.shutdown()` 与 `call_with_deadline→Dead` 在 stdio 返回后由 owner 入口显式执行；无提前 `process::exit`。
2. **父进程真实管道**：`Command::new(fixture).stdin/stdout/stderr(Stdio::piped())` + CREATE_NO_WINDOW；`close_stdin()` 仅在收到 backend 记录成功 Press 的 `dispatch` 诊断（`held=["shift"]`）后 drop 真实 ChildStdin，且断言无 writer 持第二份句柄。无父 cancel、无 sleep 竞态（全部事件驱动 + 8s 界）。
3. **fixture 环境严格**：路径/SHA256/evidence 根三变量缺失即 panic，绝不 skip；BCrypt 对显式 fixture 文件算 SHA 与必填 env 比对；child `current_exe` SHA/身份/nonce/version/build_id 与父交叉核对（本运行已实际生效）。
4. **有界 I/O/RAII**：双 owned reader 线程各 1MiB cap，overflow 是失败；写线程 8s 结果 + 2s join；Drop 先关 stdin、kill 保留 Child handle、bounded try_wait、对自有 JoinHandle `CancelSynchronousIo` + 2s 硬限、`INCOMPLETE` 不谎称完成。
5. **诚实边界**：mock held-state 非物理 OS 按键；stdout 断开未实现即不声称（SOL 已明确 deferred，无占位测试）；`session_active()` 仅按原样记录为诊断，不编造终态 oracle；wire 缺 reply 不改写成功也不单凭缺失判败，终态以 child 本地 terminal 证据为准。

**未发现阻断缺陷；未做任何源码/测试修改。**

## 冻结与哈希核对

- `source-freeze.sha256`（6 条新文件）：执行前/后均 6/6 OK。
- `artifacts.sha256`（2 exe）：执行前/后重算均匹配（`8d9de872...72435a8` / `1fb02451...eb2c4a`）；`compile-evidence.sha256` 前后核对无 mismatch。
- Windows 远端 `Get-FileHash` 两 exe 与 manifest 一致（`logs/remote-exe-hashes.txt`）。**未重编**，直接复用 owner 编译产物 `/tmp/windows-stdio-eof-sol-20260930.3wqefjmd/target/...`。

## Windows 执行

- NEW TEMP：`C:\Users\Administrator\AppData\Local\Temp\windows-stdio-eof-cc-20260930\`（`exe\`、`scripts\windows-test-runner\Runner.cs`、`config\`、`evidence\`、`eof-evidence\` 父目录先建；runner 叶目录原子预留）。
- runner 冻结未改；一次性 `eof.ps1` wrapper 在调 runner 前设置三个必填 env（`RPA_WINDOWS_STDIO_EOF_FIXTURE[_SHA256]`、`RPA_WINDOWS_STDIO_EOF_EVIDENCE_DIR`），runner 子进程与 fixture 孙进程继承。--list 与默认 run 各独立 config/evidence 目录。
- runner summary（两次运行）：`native_exit_code=0`、`outcome=success`、`timed_out=false`、`cleanup_state=root_exited`、stdout/stderr drain 完整无截断、`executable_sha256=8d9de872...`、root_session_id=0、identity 源 `exact_absolute_ProcessStartInfo_FileName`。

## 逐 case 子进程证据（child native 0，kill_used=false，双管道 EOF，线程全 join）

| case | child PID | 关键时序（parent_elapsed_ms） | 终态要点 |
|---|---|---|---|
| `active_hold_stdin_eof_cancels_after_successful_press` | 28872 | press 诊断 204 → 关 stdin 205 → 退出 287 | runtime cleanup **press 后 25ms**（<3000，排除 5000ms 自然完成）；before_shutdown 恰 1 次 cancelled release_all、held 清空；owner shutdown 后共 2 次 cleanup；`reader_cancelled_before_shutdown=true`、generation=2、`external_shutdown_flag=false`、Clean/不 fault/`post_shutdown_dead=true` |
| `partial_frame_during_hold_eof_cancels_active_action` | 5656 | press 199 → 截断帧+关 stdin 200 → 退出 281 | 同 active 语义；坏半帧未成为新 action |
| `idle_stdin_eof_shuts_down_real_stdio_worker` | 24896 | 关 stdin 144 → 退出 207 | 无任何输入事件；仅 owner shutdown 1 次 cleanup；Clean |
| `partial_frame_idle_eof_never_dispatches_input` | 20008 | 截断帧+关 stdin 148 → 退出 213 | 无输入；未见 id=99 成功 reply |

四个 case 的 `owned_cleanup` 均记录 `kill_used=false, child_exited=true, writer_joined=true, readers_joined=true, parent_panicking=false`。terminal.json 明确 `session_clock_diagnostic=true` 仅作原样记录（不清零是已知产品诊断行为，未当终态 oracle）。注意：生产 stdio 内部 reader 线程无公开句柄，子进程整体退出 + EOF 取消路径为证明边界（SOL 已声明）。

## 残留检查（只读，按精确 owned 路径）

按 `Win32_Process.ExecutablePath` 匹配本轮 TEMP 路径：**0 存活**。四个 child PID（28872/5656/24896/20008）均已由 parent.jsonl `child_exit` + `child_exited=true` 证明退出；无 name/global/taskkill。未知后代不构成 containment。

## 冻结后的边界 / Pending

- 本轮证明**真实匿名管道 EOF → 生产 stdio reader 取消**（synthetic backend），不构成 GUI、物理 OS 按键释放、TCP 断连 cancel07、stdout 断开（deferred）、remote 17 的证据。
- root 275 / cancel9 等既有 GREEN 为历史结果，本轮未重跑无关 suite。
- 执行后 `source-freeze/artifacts/compile-evidence` 三 manifest 再核对均 0 mismatch；本轮仅写本报告与 runs 目录（`eof.ps1` 为一次性编排，非可复用测试实现）；无 commit/push/worktree/release。

## 索引

- runs：`.agents/runs/windows-stdio-eof-cc-20260930/`
  - `logs/remote-exe-hashes.txt`、`evidence/eof.output`（runner 原始输出）、`evidence/residual-check.*`
  - `evidence/remote/runner/EOF-{list,run}/`（runner summary/stdout/stderr）
  - `evidence/remote/fixture/<nonce>/`×4（stdout.raw、stderr.raw、parent.jsonl、terminal.json）
  - `evidence/remote/configs/`×2、`eof.ps1`

停止。
