# Coordinator continuation review 3 — 2026-09-24

All code/script changes remain Claude Code-owned. No commit/push/worktree.

## Executed evidence
- Windows deploy diagnostic (session74289) exit1: uploaded manifest last-line CRLF caused missing windows-stop entry. Read-only remote diagnostic proves entry exists. More importantly original verifier only read uploaded manifest rather than hashing remote files; ps_remote rejected dollar signs but its own parse/ACL scripts contained them. QA3 has live feedback, is repairing.
- Windows fixture PID23072 verified in same Session1 as explorer PID8648; scheduled task still running at19:49. This is NOT screenshot/input acceptance.
- runtime-tests-3(session65219) exit101 E0502 mid-edit execute.rs; core2 received exact evidence. Core's later scoped test runs report green; coordinator independent reruns still required.
- Coordinator registry1000 explicit ignored-test run session9040 is ongoing; do not omit the gate.
- Windows all-target check session57502 is ongoing.
- macOS IOConsoleUsers still reports CGSSessionScreenIsLocked=Yes. No password input or unlock bypass attempted.

## Concrete transport findings sent to transport3
Actual stdio/tcp reader uses unbounded channel and only enqueues EOF/cancel. Synchronous service tool call blocks main, so control messages cannot interrupt native work promptly. Absolute auth deadline missing (only per-byte read timeout). TCP shutdowns worker then reuses it without fatal break; stdio logs fault but returns Clean. Headers/comments are not proof. Require actual public transport-path tests and bounded pending memory. Preserve desktop exclusion until process teardown when native work may be orphaned.

## Semantic decision
CONTRACT now clarifies successful get_step LOOKUP vs recorded failed STEP. Known record lookup is successful, includes explicit step_is_error, and preserves all original outcomes/error. Unknown identifiers are errors. Step replay remains exact original status; never infer success from partial/dispatched alone. Core2 received clarification. This retains independent capture_fail retrieval assertion without weakening it.

## Current writers
- core-fix-2 session1163, FIFO .agents/runs/core-fix-2.in
- qa-fix-3 session34507, FIFO .agents/runs/qa-fix-3.in
- transport-fix-3 session28969, FIFO .agents/runs/transport-fix-3.in — initial user message sent19:45, now actively editing.
Inspect result frames, not just process existence: streaming workers may idle awaiting more messages.

## Later verified results (20:25)
- Runtime independent tests5 exit0:68passed, protocol12passed, contract35passed1ignored. Explicit1000registry separately1passed305.34s.
- Core2 delivered report and minimal core lint cleanup. Completed CLI68041 terminated only after final success result and no descendants verified; original session1163 terminal exit143 (intentional idle-agent cleanup, NOT task failure). Source core frozen.
- Debug host mock protocol2:50passed0failed1skip(native writer exclusion inapplicable to explicit mock). Native no-input lock verification2:3passed0failed after display wake with caffeinate (did NOT unlock Mac). Script tests3:73passed0failed, exit0; still fake-CLI/script-level evidence, NOT real GUI.
- Windows diagnostic TCP failure root cause: nonblocking listener's accepted Windows socket needed explicit set_nonblocking(false). Claude patch applied. After actual cross-build3/deploy4/start3, tools/list8 + native computer_open succeeded, observe returned capture_error with access denied0x80070005, close returned released. Same Session1 has LogonUI.exe; user asked to unlock Windows as well as Mac. No input or screenshot success.
- Diagnostic Host26452 auto-exited on client EOF; stop3 exit0 removed only exact owned task. Previous17416/20460 also verified cleaned. No Windows Host currently left running by this continuation.
- Full MCP unit gate still incomplete; transport3 owns repair, modules being split. Clippy/fmt/fulltest remain unverified.
