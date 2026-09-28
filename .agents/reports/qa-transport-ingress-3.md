# qa-transport-ingress-3 — surgical harness corrections + one both-transports run

Role: QA (test-only). Sole writes: `tests/mcp_transport_regression.py` + this report
(+ run log `qa-transport-ingress-3-run.log`). No src/Cargo/scripts/README edits.

## Files / hashes

- Harness SHA-256 BEFORE edits: `5b40092d53a9972d5701d2fa5cd1af60251656fae7befc6b273009769a8d0bef`
- Harness SHA-256 AFTER edits:  `13a8d2748cd84b4b4d19935c04246d064ce929c8b469a2a6b8ec5ea31ebc4400`
- Host binary under test: `target/release/computer-host` (built by others; not rebuilt, not modified here).
- `python3 -m py_compile tests/mcp_transport_regression.py` → OK.
- Full run log: `.agents/reports/qa-transport-ingress-3-run.log`
  (`python3 tests/mcp_transport_regression.py --host target/release/computer-host --transport both`, exit code 1).

## The four corrections (exactly as directed, nothing else)

### A. T5 action kind: `type_text` → `text_input`; QA2's claim retracted
Verified against `src/runtime/actions.rs` (lines ~104/146/250): the real action kind
string is `"text_input"` (serde tag + allowed-fields table + parser arm). QA2 had
followed an obsolete design doc: the previous run's `type_text` step was
schema-INVALID, so its "payload never became control" evidence proved nothing about
valid text handling. That claim is retracted.
T5 now sends `{"kind":"text_input","text": ...cancel text...}` and the final T5 check
additionally requires `step_ok`: the mock step result itself must be `isError=false`,
not classify as `other_error`, and carry an actual returned `observation_id` basis —
i.e. proof the runtime really executed a schema-valid text_input step rather than
rejecting it at the schema boundary. No product change was needed or made.

### B. `classify_cancelled` now recognizes the exact structured code
Added: `meta["error"]["code"]` inside the tool-result content-text JSON, exact match
on `"cancelled"`/`"canceled"` (case-insensitive), now classifies as `cancelled`.
This is the real cancelled tool response shape (`result.isError=true` +
`{"error":{"code":"cancelled",...}}`). Generic validation/capture/worker errors
(e.g. `invalid_params`, `capture_failed`) and any arbitrary bare `isError` still
classify as `other_error`. Added `_check_classifier_selftest()` (5 canned cases:
cancelled tool error, invalid_params, capture_failed, clean success, wire-level
-32800) executed once per transport — a harness-local check, no fake GUI evidence.

### C. TCP disconnect now delivers a real EOF (T3 was harness-broken)
Previously `self.sock.close()` left the `makefile("rb")` reader holding its own
socket reference, so the reader never saw EOF — T3's "product disconnect failure"
was harness-induced. Now:
- `Host` stores `self._sock_file` and `self._reader_thread`.
- `disconnect()` (TCP) does `shutdown(SHUT_RDWR)` first (unblocks the reader with a
  genuine EOF and delivers EOF to the peer), then closes socket + reader file, then
  joins the reader thread with a 2s bound.
- `_closed` now means ONLY "we locally disconnected"; actual peer EOF is tracked
  separately in `_peer_eof` (set only by the reader thread when `readline()`
  returns empty/errors). `_closed` is never treated as peer EOF.
- The reader thread is the sole writer of `_lines` per connection; the queue is
  fresh per connection (no reconnect exists in this harness), so no stale lines.

### D. T4 write errors no longer masquerade as termination
- 4a and 4b: `write_error` is recorded separately; `terminated`/`eof` are derived
  ONLY from `_peer_eof` (reader marker) and `proc.poll()` — never from
  BrokenPipe/WriteTimeout, which can also mean a wedged-but-alive host. This removes
  the false-PASS path for a wedged host.
- The read/drain loop now ALWAYS runs to its deadline after a write failure (a
  too-large refusal frame may still be on the wire); previously a write exception
  set `terminated`/`eof` and skipped draining entirely.
- TCP writes no longer use `settimeout` on the shared socket (which could inject a
  spurious timeout-EOF into the blocking makefile reader): `write_raw` now uses the
  same select+`os.write` `deadline_write` helper on the socket fd for both
  transports, and the socket is left in blocking mode for the reader.
- The single T4b deadline is unchanged (no per-chunk reset).
- Null-id error replies (too-large frames cannot reliably expose their original id)
  are counted as `oversize_errors` and offset the missing-id accounting; the
  original id is no longer demanded for every oversized error.
- The overflow check still REQUIRES an explicit refusal PLUS actual bounded
  termination for a PASS. The previous "tore down without refusal → PASS" branch is
  now a FAIL ("required explicit refusal is missing"); the all-serviced path still
  only records INFO ("overflow not exercised"), never a pass.

## Actual run evidence (one both-transports run, 26 passed / 10 failed, exit 1)

Identical results on stdio and tcp:

| Check | stdio | tcp |
|---|---|---|
| classifier self-test | PASS | PASS |
| T1 cancel-behind-request honored | **FAIL ×3** | **FAIL ×3** |
| T1 unknown-id cancel leaves valid request alive | PASS | PASS |
| T2 three queued responses by id / pause took effect / fresh resume first-try | PASS | PASS |
| T2 pre-stop queued resume refused | **FAIL** | **FAIL** |
| T3 bounded termination on EOF/disconnect | **PASS (rc=0, 0.1s)** | **PASS (rc=0, 0.1s)** |
| T4 flood refusal-or-termination / no answers after refusal | PASS | PASS |
| T4 oversized: explicit refusal AND bounded termination | PASS | PASS |
| T5 malformed cancel frames never cancel | PASS | PASS |
| T5 text payload never cancels later request (with step_ok) | **FAIL** | **FAIL** |

Key log lines:
- `T3 stdio exit :: rc=0 after 0.1s` / `T3 tcp classification :: host exited rc=0 after 0.1s on clean EOF`
  — with correction C, the alleged product disconnect failure disappears on BOTH
  transports. QA2's T3 product-defect claim was harness-induced and is retracted.
- `T4 oversized classification :: overflow_refused=True write_error=None peer_eof=True host_alive=False answered=1/70 null_id_errors=5 elapsed=0.1s` (stdio; tcp identical except `host_alive=True` at check time with real peer EOF)
  — explicit overflow refusal + actual bounded termination: genuine PASS on both transports.
- T1 failures: cancel notification in the SAME write behind the observe loses the
  race — observe returns a clean success after the full 3000ms wait, 3/3 trials,
  both transports. Harness side shows no defect here (single write, exact-id
  matching, classifier self-test passing); consistent with a real product-side
  cancellation-handling gap. Attribution to product is noted but source diagnosis
  is out of my scope.
- T2 failure: pre-stop queued `computer_resume` returns clean success
  (`{"note":"session was not paused","state":"ready"}`) on both transports.
- T5 failure: the valid `text_input` step returns `isError=false` with
  classification `clean_success`, but its content carries NO `observation_id`
  (observation basis), so `step_ok=False`. The cancel-text-as-data never cancelled
  the later observe (classification `clean_success`), but the new stricter check
  correctly refuses to pass without proof of a real executed step with a returned
  observation basis. Whether the mock step response SHOULD embed an observation_id
  is a product question, not diagnosed here.

## Retractions of QA2's misleading claims
1. QA2's `type_text`-based T5 "valid text handling" evidence: retracted (kind was
   schema-invalid; real kind is `text_input`).
2. QA2's T3 "product fails to terminate on disconnect": retracted — harness failed
   to deliver EOF (makefile held the socket ref); with a real shutdown+EOF the
   host terminates in 0.1s on both transports.
3. QA2's T4 PASSes were partially unfalsifiable (write timeout counted as
   termination; refusal-free teardown counted as pass). Corrected as in section D;
   the current T4 PASSes rest on an explicit refusal frame + real peer EOF.

## Bottom line
Known-broken candidate remains RED (exit 1): 10 failures — T1 ×3 per transport,
T2 ×1 per transport, T5 ×1 per transport. The previously-alleged T3 product
failure was a harness bug and now PASSes on both transports. No production source
was read beyond confirming the `text_input` lines, and nothing outside the two
permitted files was modified.
