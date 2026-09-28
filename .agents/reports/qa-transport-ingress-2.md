# qa-transport-ingress-2 — harness repair + truthful RED run (frozen)

## Scope
Test-only repairs to `tests/mcp_transport_regression.py` per coordinator
instructions and `.agents/reviews/qa-ingress-test-review-1.md`. No
src/scripts/Cargo/README files were read for diagnosis or edited. Mock
backend is hardwired (`--mock-backend` in the Host constructor); there is no
native-backend option.

## Command / status / counts
```
python3 -m py_compile tests/mcp_transport_regression.py   → COMPILE_OK
python3 tests/mcp_transport_regression.py --host ./target/release/computer-host --transport both
  → exit 1 (RED, expected — candidate is known broken)
  → 24 passed, 10 failed
```
Full output: `.agents/reports/qa-transport-ingress-2-run.log`.

## Binary snapshot integrity
- SHA-256 BEFORE run: `f7b99ee375c4f4d338859aa96fbaf0a355392ec7a0a8f15426d7d0c12e0b6a50`
- SHA-256 AFTER  run: `f7b99ee375c4f4d338859aa96fbaf0a355392ec7a0a8f15426d7d0c12e0b6a50`
- Identical → results correspond to ONE unmodified candidate snapshot (valid).

## Repairs applied (each mapped to coordinator item)
1. **TCP token collision / missing `proc_tag`** — removed the undefined
   `proc_tag()` call; token file is now `token-{secrets.token_hex(8)}`,
   unique per Host within the shared workdir. `O_EXCL` + `0o600` preserved,
   token never printed. The previous `FileExistsError` on the second TCP
   Host is gone (10 TCP Hosts constructed cleanly in this run).
2. **T4b false-positive fixed** — a pending `observe(wait_ms=3000)` is now
   written FIRST to pin the worker before the 68 oversized frames, so a real
   backlog exists. ONE deadline covers writes and reads; the read loop never
   resets it. PASS now requires explicit overflow/refusal AND bounded
   termination (EOF or process exit); if everything were serviced instead,
   every one of the 68 responses + the tail ping must be individually
   accounted for, the outcome is reported as "all serviced, overflow not
   exercised" (never labelled an overflow PASS), and a live host with
   missing responses FAILs explicitly.
3. **Bounded writes** — stdio writes go through a new `deadline_write()`
   helper (nonblocking fd + `select` + `os.write`, 64 KiB chunks); TCP
   writes use `settimeout` + `sendall`. No synchronous unbounded pipe writes
   remain; `WriteTimeout` is raised instead of wedging. The T4a/T4b read
   loops use single monotonic deadlines. Cleanup only ever signals the exact
   owned process group (`start_new_session` + `killpg` on own pid); a
   constructor failure after `Popen` calls `terminate()` on the owned
   process before re-raising.
4. **`classify_cancelled` strictness** — cancellation is recognized only via
   structured signals: error code -3280/"cancelled"/"canceled", an error
   message literally containing "cancelled"/"canceled", or explicit
   `state: cancelled` / `cancelled: true` metadata. A bare `isError`
   (schema error, invalid session, worker fault) is now `other_error`, never
   relabelled a cancellation pass. T5 uses valid action kind `type_text`
   (confirmed against the design doc schema name) and ties `based_on` to a
   REAL observation_id obtained from a preceding live observe; the step's
   own disposition is recorded as INFO (its mock result is incidental — the
   tested requirement is only that the payload string never became control).
   Mock-only mode unchanged; no native backend possible.
5. **TCP documented lifecycle asserted** — one authenticated connection per
   host process; after a clean EOF the host MUST terminate within
   `EOF_BOUND` (15 s). The reconnect path and the stderr-'fault'-substring
   acceptance were removed entirely; an alive/wedged host is now a plain
   FAIL. `Host.connect()` is called exactly once, at construction.

## Truthful result summary (candidate bugs, harness behaved correctly)
Identical RED signatures on BOTH transports:
- **T1.1–T1.3 FAIL** — a `notifications/cancelled` written in the SAME write
  as the observe is never honored: the cancelled observe returns a clean
  screenshot success after the full 3 s wait (race lost 3/3 trials, both
  transports). The unknown-id control case PASSes.
- **T2 FAIL** — a resume queued BEFORE the pause is accepted
  (`"session was not paused", state=ready, isError:false`) instead of being
  refused; pre-stop ordering is not enforced.
- **T3 tcp FAIL** — host still running 15.0 s after a clean EOF on its only
  authenticated connection (documented one-connection-per-process lifecycle
  requires bounded termination). stdio T3 PASSes (rc=0 after 0.1 s).
- **T4b tcp FAIL** — explicit overflow refusal IS emitted, but the host
  stays alive and serving afterwards (refusal without bounded termination).
  stdio T4b PASSes (refusal + EOF, host exited, 1/70 frames answered).
- PASS: T4a flood refusal (both), T5 malformed-cancel and payload-text
  non-control (both), T2 drain/fresh-resume, T1 unknown-id.

## Harness self-assessment (honest residual notes)
- No harness defect remains that distorts results: the run completed within
  budget (no wedge, no timeout kill), per-Host token files worked, and every
  FAIL above is a candidate behavior, not a harness artifact.
- Minor cosmetic residual: the TCP listen-probe at startup produces a
  benign "auth rejected … connection closed before authentication" stderr
  line in host logs; it is an unauthenticated probe and does not affect the
  authenticated single-client lifecycle being asserted.
- `classify_cancelled`'s message-substring rule ("cancelled" in error
  message) is a documented heuristic for transports without a structured
  cancellation code; structured metadata (`state`/`cancelled`) is preferred
  and checked first.

Frozen. No source edits were made or attempted; RED is the expected and
recorded outcome.
