# Core integration findings from executed tests (2026-09-24)

Do not weaken independent tests. Core worker still active; this is queued feedback to consume after it finishes or on next repair task.

Evidence: coordinator-runtime-tests-2.log, freshly compiled Runtime filter exited101:50passed,10failed,72.25s. qa-contract-results.log:36 tests,25passed,10failed,1explicitly ignored slow registry test,3.66s. These are no longer old binary-only evidence. core changes since run may fix some; re-run exact tests.

Root-cause observations:
1. Runtime::step always Reply::ok even when record.error exists, including validation/stale/conflict. Must error flag reflect error without erasing truthful partial/dispatched metadata. get_step cached error status must be consistent. No invalid/conflicting request should attach unrelated cached image from an earlier request with same id.
2. **Pre-input geometry revalidation missing**: execute_step validate_step only consults stored session/observation geometry. Backend live geometry is queried after input when capturing, so independent geometry-change test actually dispatched click before noticing new geometry. Add current backend.geometry check before ANY input, fault/reject stale geometry with not_started, zero events. Also compare enough geometry identity/dimensions, not only a misleading version if contract permits.
3. Resume currently preserves pre-pause observation validity. Design says require fresh observation after successful resume; invalidate stored bases/pending decisions, keeping cached request result retrieval intact.
4. Public observation JSON put dimensions only under image, contradicting CONTRACT which requires width_px,height_px at top level; independent tests and several core tests fail on Null. Establish exact contract shape and emit consistently for observe and step observation.
5. Key name parser requires len==2 for f* so f10/f11/f12 fail though backend supports them. This is a real parser bug, not a test expectation bug.
6. Image ratio test assumes1366x768 but1600x900 constrained by768 height actually rounds1365x768. Preserve aspect ratio; fix incorrect expectation while asserting decoded PNG agrees metadata. Coordinate test setup defaultmax1366x768 DOES downscale1600x900; expected800,450 vsactual938,527 is bad fixture assumption. Test both downscaled and explicitly non-downscaled cases meaningfully.
7. New mid_text_cancel test uses time sleep30ms against instantaneous mock inject, so cancellation occurs during settle AFTER all text input. Must deterministic injection hook trigger cancellation after N chunks, not arbitrary thread sleep. Do not alter real input semantics merely to force test timing.
8. hold deadline release test expects explicit release even though do_release_pass has deadline already expired; cleanup release_all spy may not record actual releases. Ensure real cleanup occurs/retries and assert actual contract, not false 'we called cleanup' proof.
9. Partial test expects1 event but executor performs extra planned release; inspect if extra release actually belongs to a successfully/possibly pressed item. Do not release future unpressed keys or mutate user-held keys as blind cleanup.

Independent test split import repair is complete: tests/contract/scaling.rs imports backend::Capture. The small Claude task hit max_turns12 AFTER compilation and test run; no completion report from it. Tests are unchanged except missing import; source-control coordinator has verified this exact import.
