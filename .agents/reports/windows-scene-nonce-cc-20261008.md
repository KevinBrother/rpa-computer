# Windows scene nonce — fresh StageB fixture visible in capture (CC), 2026-10-08

Status: the ONE authorized controlled experiment completed successfully — a freshly started StageB multiclick fixture, with a random per-run nonce, was **actually visible and readable in a real tool capture**: inner GLM's single screenshot showed `NONCE: 24Q9DM`, byte-identical to the tool image (SHA `9127aae3…ff76c7`), and **exactly matching the coordinator-only oracle's fresh trial nonce** (`multiclick-01`, trial 1, ts `2026-10-08T05:39:22.8190101Z`). `scene_verified=true`, `fresh_scene_instance_verified=true` (coordinator readback). Zero input, zero retries, one observation. The prior scene failure is NOT retroactively explained and continuous capture freshness is NOT proven by this single frame.

**Outer-session note**: the previous outer CC session hit `max_turns` and exited 1 after the experiment and most cleanup were already done. That failure record is preserved here, not rerun; this report is the authorized wrap-up (final verification, evidence fetch, no new actions).

## 1. Timing erratum (replacing my earlier misjudgment)

My in-flight claim that the fixture "self-closed spontaneously / from an unknown external cause" is **WITHDRAWN**. The coordinator's verbatim readback of my outer transcript (`/private/tmp/windows-scene-nonce-cc-20261008/coordinator-readback.json`) proves:

- my `stop-windows.ps1` tool_use `call_5185fa1fcabd4780ae39b0a2` at **05:42:11.205Z**;
- fixture `session_close` at **05:42:13.6055974Z** (written by `FormClosed` during the graceful task stop);
- the stop tool result at **05:42:13.739Z** ("task stopped and unregistered / process already ended with the task");
- backdrop watchdog exit at **05:42:16.4** = the designed ~2.9 s later watchdog reaction to fixture death.

The fixture exit happened DURING my stop command, in the authorized cleanup order. It was not a spontaneous exit; no new product defect exists; production code unchanged.

## 2. What was executed (once each; all identity pre-verified)

- **Precheck**: sole GUI owner (0 `RpaGuiRunner-*`/`AccFixture-*`/host tasks, 0 fixture processes, session 1 + explorer), Notepad 24332 intact, `WmiMonitorID` non-empty. LAN IP dynamically discovered: WiFi `100.200.20.168` — **same as configured**, so no config change was needed (mcp.json target verified as `100.200.20.168:8399`, token never printed). Port 8399 free before start.
- **Host**: existing deployed fixed bins verified in place unmodified (host `c8c1348f…`, client `445675c3…`); started via frozen `scripts/windows-remote-start.ps1` → `remote host listening on 100.200.20.168:8399 (pid 27372, session 1, task 'RpaComputerRemoteHost-9f2ef637f7464bf18cb52150a311c742')`, ssh_exit=0.
- **Fixture**: StageB bundle `start-windows.ps1` (repo-byte version, NOT the old `9c44f70e…` helpers-dir pin), PE hash `bff857b9…1eb471` guarded, Suite multiclick → `OK: fixture pid=15780 session=1`, run dir `run-995f7380cda945fbb344fed530625257`, new evidence dir `evidence-20261008-scene-nonce-cc-01`.
- **Backdrop (with logs)**: frozen `gui-neutral-desktop-20260930.ps1` (remote `43beb6f2…` byte-identical) run under frozen `windows-test-runner.ps1`+`Runner.cs` via `gui-runner-launch` in session 1; config SHA `cca7bd89…a858a`, `MaxMinutes=3`, `timeoutSeconds=210`, task `RpaGuiRunner-1c3a48b698954a0685a9d87078635014`. Pre-inner early-exit check at +33 s: root pid 22288 alive, session 1, no summary → proceeded.
- **Inner**: frozen `gui-remote-agent-glm-utf8-20260929.ps1` (remote hash == local `fc5a733e…`) via Interactive ScheduledTask (task `RpaGuiRunner-ab699496b9394aa5bff7d4f8e74670cb`), task file SHA `461f1f31…96bd` (1211 B, verified both sides; contains no expected nonce/date hints), `MaxTurns 12 / MaxSeconds 120`, 8 computer tools only, strict-mcp-config, hooks disabled.

## 3. Inner result (raw transcript + run-summary + stderr fetched)

- `run-summary.json`: `exit_code=0`, `timed_out=false`, `stdout_drain_complete=true`, `prompt_encoding=utf-8`, `prompt_sha256=461f1f31…` (match), stderr.log **0 bytes** — empty stderr only means "no exception was recorded", nothing more.
- **Actual model**: all 9 assistant messages `glm-5.3-flash` (requested alias `sonnet`).
- **Tool sequence (exactly 4)**: `computer_describe({})` → `computer_open({"display":{"kind":"primary"}})` — default capture, NO size override → `computer_observe(session-30704-1)` **×1** → `computer_close` (`cleanup_outcome: released`). Zero `computer_step/get_step/pause/resume`, zero mouse/keyboard.
- **PNG extracted byte-for-byte from the tool image**: `observe-3.png`, 27,642 B, SHA `9127aae3b4f43eb4c28ecbb0e47d1968c590eff2568e82ae823584d45cff76c7`, 1365×768 (tool default downscale of native 1920×1080; no override passed).
- **Image content (coordinator visually confirmed == raw tool image)**: window "Computer Use Acceptance" fully visible, unobstructed (no security/firewall dialog); `MULTICLICK · multiclick-01 · Trial 1/10`; **`NONCE: 24Q9DM`**; instruction "Click the big TARGET rectangle exactly once…"; "Waiting for native input"; TARGET rectangle; "READY — perform the requirement, then Check once"; Check/Next/Close all readable.
- **Nonce match (coordinator-only oracle)**: oracle fresh trial nonce `24Q9DM` == image reading ⇒ the capture showed the CURRENT freshly started instance. `scene_verified=true`, `fresh_scene_instance_verified=true`.
- **Boundaries**: this certifies ONLY this new scene instance being visible in one capture. It does NOT prove continuous screenshot freshness, any input behavior, or the root cause of the earlier scene failure (`continuous_capture_freshness_proven=false`, `prior_scene_failure_root_cause_proven=false`). host.log new segment: single authenticated client, worker exit 0, no anomalies (49366 session clean).

## 4. Cleanup (identity helpers; verified end state)

- Inner runner stop (after natural end): `stopped GUI runner: task 'RpaGuiRunner-ab699496b9394aa5bff7d4f8e74670cb', pid 30640; record removed.`
- Fixture stop (bundle `stop-windows.ps1`, during-command session_close per erratum above): `task 'AccFixture-995f7380cda945fbb344fed530625257' stopped and unregistered (identity verified before mutation)`; run dir/oracle/record left intact.
- Backdrop runner summary then appeared: `native_exit_code=0`, `outcome=success`, `timed_out=false`, root 05:39:40.555→05:42:16.468 (session 1), stdout/stderr 0 B, no truncation, drains complete — exit via the designed watchdog after fixture stop (see erratum), NOT the 180 s lifetime timer; runner task then stopped via identity helper (`pid 25252; record removed`).
- Host stop: `stopped remote host: task 'RpaComputerRemoteHost-9f2ef637f7464bf18cb52150a311c742', pid 27372; record removed. Logs and secrets left in place.`
- **Final check (verbatim, ssh_exit=0)**: `RPAGUI=0 ACCFIX=0 HOSTTASK=0 LISTEN8399=0 FIXPROC=0 NOTEPAD_24332=Notepad` — no owned residue, 8399 has no listener, non-owned Notepad 24332 intact. No name-based kills at any point.

## 5. Evidence index (all under `/private/tmp/windows-scene-nonce-cc-20261008/`; nothing deleted/overwritten anywhere)

- `inner-run/`: `transcript.jsonl` `d7a80d6be3f72255244ef30ceb782d4662b2ff3e344cdb7934b2fb89603f001f`, `run-summary.json` `05976b83b422732767d4082fd2da03fb3ecc36b723581c0cbb0befe26ddb5f07`, `stderr.log` 0 B (`e3b0c442…7852b855`).
- `observe-3.png` (byte-exact tool image) `9127aae3b4f43eb4c28ecbb0e47d1968c590eff2568e82ae823584d45cff76c7`.
- `oracle-fresh.jsonl` (pre-stop snapshot, 811 B, BOM) `e71016aa509a7e9fadbf61f8716acbe58e7bc84c35480f7aa890a83507c14a48`; `oracle-post-stop.jsonl` (with `session_close` 05:42:13.6Z) `2ebf8fc70109754d84a14dab62b0cc9435430fb4d9e2ba9888ee1c02f2bb7aea` — coordinator-only, never sent to inner.
- `backdrop-first-evidence/`: `summary.json` `6d2b25b9ddae3d0f1b98e84b32b8dd20d37ad9cba5281aa2161b4736a1d4d6c1`, `identity.json` `1af9dde38b83c4498ccb25e48bf7d0c6374bbae8f84aef1c542a58bd1b0c19fa`, `config.json` = `backdrop-config.json` `cca7bd89c09a54dafeba6a57097a994ec33c08b591debe6e16fdffea752a858a`, `stdout.log`/`stderr.log` both 0 B.
- `host-log-snapshot-scene-nonce-20261008.txt` (read-only full snapshot, new filename, 6016 B) `ab4400004a246f1f3d244125fcf613147dc431a9feb3d3a8cf0cc148e800fa26`.
- `logs/` raw ssh/scp stdout+stderr for every step (unfiltered, incl. SSH post-quantum warnings), `inner-task.txt`, `fixture-record` copies, orchestration scripts (`fixture-start.ps1`, `backdrop-config-launch.ps1`, `backdrop-earlycheck.ps1`, `wait-inner-summary.ps1`, `wait-backdrop-final.ps1`).
- Coordinator machine-readable readback: `coordinator-readback.json` (models, tool sequence, nonce match, `scene_verified=true`, timing-erratum block).
- Remote originals preserved: `C:\computer-cc-preflight-20260930-191500\evidence-20261008-scene-nonce-cc-01\` (inner run dir, fixture run dir incl. oracle), `C:\Temp\windows-scene-nonce-cc-20261008\` (backdrop evidence), `host.log`.

No new services/GUI/capture were started during this wrap-up; no further actions taken. Stopped per instructions.

## 协调者最终核对

已亲自查看 `observe-3.png`，可读MULTICLICK / multiclick-01 / Trial 1/10 / NONCE 24Q9DM及完整TARGET、Check/Next/Close；新鲜oracle trial在05:39:22.819Z记录同nonce。原始用户tool-result的message投影只有一张图；它同时出现在转录的tool_use_result投影中，递归遍历会重复计数，但不是第二次观察。两投影解码字节均与PNG（SHA `9127aae3b4f43eb4c28ecbb0e47d1968c590eff2568e82ae823584d45cff76c7`）完全一致。

已读inner summary native0/no timeout，以及describe/open(primary默认尺寸)/observe/close四次实际调用，无输入/重开/降尺寸；实际响应模型均GLM。只证明这个新场景实例能被当前链路捕获，不认证连续刷新，也不解释旧场景失败的全部原因。

已直接读backdrop summary native0、root Session1、无timeout、stdout/stderr0；05:42:13.605Z的fixture关闭位于05:42:11.205Z开始的stop工具调用之内，明确撤回自退推断。终态原始工具输出：RPAGUI=0、ACCFIX=0、HOSTTASK=0、LISTEN8399=0、FIXPROC=0、NOTEPAD_24332=Notepad。原外层exec退出1（max_turns）保留，收尾exec退出0，未重跑inner或GUI。全部actor已终态。

协调机器可读索引：`/private/tmp/windows-scene-nonce-cc-20261008/coordinator-readback.json`。未修改产品代码；未运行macOS/Linux；未提交或推送。
