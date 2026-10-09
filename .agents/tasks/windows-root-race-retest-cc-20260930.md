# Windows root race repair — independent CC+GLM review and execution

Continue previous Windows verification. Actual model must be glm-5.3-flash (sonnet is user alias); do not change global config. Implementation owner has frozen `.agents/reports/windows-root-race-repair-sol-20260930.md`: read it plus new source/artifact manifests first. Use ONLY its NEW 9 compiler-produced Windows executables, not old target globs. Coordinator will issue launch only after freeze. No product/test/build implementation edits. No Agent/Task/subagents. No macOS/Linux tests, GUI, Host network, real capture/input, renderer or security/global settings changes. Pure mock/console Windows Session0 only. Protect user Notepad PID24332. No commit/push/worktrees/reset. Preserve every first failure.

First review source changes for requested semantics and code quality, report independently:
- geometry gate really arms after open succeeded + pause succeeded and triggers during resume, not absolute geometry call count. stop race must retain cancellation and authority permanently revoked assertions.
- hold cancel is triggered ONLY after mock inject accepted press; tests must still require Partial and release/held cleanup; promptness timer starts at dispatch-triggered cancellation, not costly observation setup. No relaxing to NotStarted, no sleep increase.
- waits bounded, test panic doesn't strand a held barrier/thread, RAII unblocks on failure. Clearly distinguish accepted synthetic dispatch from actual OS release.
- no unrelated product change. Do NOT blindly trust author's claims; if a concrete blocking defect exists, report and stop without editing.

Verify manifest hashes before and after execution. Use new fresh Windows TEMP, runner in correct scripts/windows-test-runner.ps1 + scripts/windows-test-runner/Runner.cs layout. Precreate evidence PARENT not leaf. Copy all NINE new artifacts, including windows_cancel_contract; enumerate compiler JSON and list exact target names, compare SHA256 local vs remote for all9. Do not rebuild unless coordinator explicitly asks; owner compiled but did not run. Reuse proven quoting/on-disk ps1 wrappers from previous successful run, not inline JSON/shell substitutions. No modifying approved runner. Scripts for staging/invoking frozen runner may be written; these are disposable test orchestration, not reusable test implementations.

Execution order (fresh evidence per invocation; no ignored/--include-ignored):
1 Same rootlibexe two exact original failed tests, separate filters `--exact --test-threads=1 --nocapture`, timeout120 each. If failures stop and preserve.
2 Related feedback::tests_runtime and runtime::session::tests filters, timeout240 each. Same original six multidisplay filters plus token tests as previous PhaseA31, timeout120 each, and all four topology exes default --test-threads=1 --nocapture. Record exact counts, distinguish executions vs unique.
3 Full DEFAULT rootlib --test-threads=1 --nocapture timeout540 (last305s), outer wait generous. Keep ignored6. Never restart just because CLI/Bash observation interval expired: poll SAME process. First native failure stops subsequent broader phases; extract actual assertion and source location, do not rerun for green.
4 ALL four new integration binaries protocol, runtime_contract, transport_lifecycle, windows_cancel_contract --test-threads=1 --nocapture, perexe240. Keep ignored registry tests ignored. No remote_transport/real networking tests.

Read actual runner summary native_exit_code/timed_out/outcome/identity/drain fields + stdout/stderr. Launcher/CLI exit0 alone is not pass. Archive all raw runner evidence/config/wrappers/SHA/outputs under `.agents/runs/windows-root-race-retest-cc-20260930/`, report `.agents/reports/windows-root-race-retest-cc-20260930.md`. Read-only exact-owned residual check, do not name/global kill. Unknown descendants are not containment. Check source hashes after. This pure regression does not validate GUI/physical displays/feedback Stop/capture exclusion.

Report original full-suite RED273/2/6 unchanged as history, new counts/native codes separately; do not claim whole computer product accepted. Previous correction report is only documentation correction, not tests. Stop after report.

上一轮报告纠正exec74311达到max_turns退出1，但文档编辑已落盘。新轮顺便追加说明：不要声称 PhaseA和PhaseB内部有重复（未证）。A31与B27是不同binary，58只是两阶段执行次数；真正明确的重复是PhaseA31再次被full root PhaseC覆盖。不需要强行推导unique全局计数，不要为了纠正报告消耗主测试预算。
