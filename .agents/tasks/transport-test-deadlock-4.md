# Immediate repair: deterministic enqueue tests, NO flood/no unsafe

Coordinator confirmed actual dependency-cycle failures in mcp-tests5. queued_open uses ONE synchronous describe loop then waits stash_len>0, which can never persist because only one request at a time exists. spin_until panics; scoped flood thread never gets flood_go=false; scope.join hangs forever. queue test AGAIN performs synchronous describe calls while gated native capture is unreleased, guaranteeing deadline/quarantine before it can fill queue. Fix exactly, do not repeat either pattern.

Already-implemented `Worker::enqueue` returns admission + Receiver without waiting. USE IT for BOTH tests, no test threads needed:

## queued decision cancellation
1. Create GatedBackend with capture_entered/release atomics; open session with call.
2. Install ReleaseGuard AFTER constructing w, so on unwind guard drops before worker and opens native gate.
3. `enqueue(observe sid)` -> retain rx_observe. spin_until capture_entered true (bounded).
4. `enqueue(resume sid)` -> retain rx_stale. Admission itself proves it is queued while native gate closed; do NOT wait stash_len or sleep.
5. `cancel_handle.cancel()` changes generation. `enqueue(pause sid)` -> rx_pause (with correct session_id). Native gate still closed.
6. release=true BEFORE waiting for any replies.
7. recv_timeout each Receiver with a generous bounded2-5s: old resume MUST cancelled, pause MUST paused/success, observe may report cancelled but must return; no worker fault/quarantine.
8. Fresh normal call(resume sid) succeeds. shutdown.

## queue bound
1. Same gated worker/open/ReleaseGuard; enqueue observe and wait capture_entered.
2. Call NONBLOCKING enqueue(describe) MAX_QUEUED_COMMANDS times, retain each Receiver. No synchronous call, no tiny reply deadline.
3. Next two enqueue calls must immediately Err Reply error.code server_busy. This proves total pending cap while no native completion can free capacity. If production cap differs, fix production atomic admission, not assertions.
4. release=true; receive observe and all pending replies using recv_timeout. All accepted describes succeed, worker NOT faulted. shutdown.
5. Native-timeout/quarantine is a SEPARATE test, never a passing premise of this bounded-queue test.

No unsafe WorkerRef, no sleeps-as-proof, no flood loop, no new wrapper architecture. Existing enqueue uses same production admission path: preserve shared logic. Run these two --exact first with pipefail and actual exit codes, then full mcp:: tests. Do not pipe tail without pipefail. Coordinator has terminated ONLY exact deadlocked test children (not your CLI), so you can continue in the same writer.

Remaining transport findings (after tests green): strict parsed reader-side cancel/pause/close, direct EOF cancel, queue overflow explicitly terminates/cancels instead of silent dropped requests, public transport-path tests. These are still outstanding in stdio/tcp source; comments aren't implementations.
