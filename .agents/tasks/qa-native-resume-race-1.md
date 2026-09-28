# One deterministic worker regression, TEST ONLY
Sole write tests/transport_lifecycle.rs and .agents/reports/qa-native-resume-race-1.md. Never modify src/Cargo/scripts/other tests. No GUI, real backend, remote, commit/push/worktree/model changes. Another writer owns mcp implementation. Limit reading to public Worker/BackendFactory/Backend traits and resume method lines; no entire source dump.

Coordinator inspected exact source: Runtime::resume checks backend.geometry() then at runtime/runtime.rs~378 unconditionally cancel.store(false). Worker checks stop generation in coordinator before dispatch; native WorkItem currently lacks epoch. Thus a new cancel DURING the native resume's geometry() is later erased by its store(false), so STOP can be lost. Need concrete deterministic test, not timing guess.

Create ONE integration test via PUBLIC Worker::start(BackendFactory::Test(...)) and Backend mock. Steps:
- open then pause a fake session (no OS calls).
- arm backend geometry gate ONLY for next resume; geometry signals entered then waits release flag. Other geometry calls return stable geometry (16x16 sufficient).
- run w.call(computer_resume,sid) on scoped thread; release-on-unwind guard MUST be inside scope closure so it drops BEFORE implicit scoped-thread join. No synchronous call behind unreleased gate.
- bounded wait until geometry entered; w.cancel_handle().cancel() while it is definitely inside geometry; release gate BEFORE receiving/joining.
- assert new stop is NOT erased: cancel_handle.is_cancelled() remains true; resume must NOT report clean ready success; expect structured cancelled error, not unrelated failure. Subsequent observe must remain cancelled until NEW deliberate resume. Then fresh deliberate resume (gate disarmed) succeeds on first call and observation requires fresh basis.
- no native input; capture can deliberately error if reached unexpectedly, but don't count capture_error as cancelled success; only exact error.code cancelled satisfies blocked observe.

Run cargo test --test transport_lifecycle --offline -- --nocapture with pipefail. Current implementation EXPECTED RED; report actual panic/assertions and codepath, do not weaken assertions/change source. Source writer may fix concurrently; label binary/source snapshot accordingly. At most150-220lines. Use existing deps only, no task/retry framework. At most12turns: read narrow signatures, Write test, run, report, STOP. No all-target rebuild loops.
