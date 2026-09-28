# Continuation review 5 — coordinator, 2026-09-24

Previous goal turn classified as **progress**: real release cancellation regression found; dual-platform builds and new81-check runner suite independently executed. Goal remains active; GUI lock blocker does not prevent remaining software work.

## Current authoritative state
- transport-closeout-4 exec56365 confirmed live this turn. Sole src/mcp/** + src/bin/** writer, no FIFO. Prior transport3 is confirmed exited143; no overlap.
- readme-closeout-2 exec75358 confirmed exit0. README + report reviewed: actual default temp release policy corrected; late cancellation failure and no-GUI-evidence status explicit; SSH command quote fixed. README now frozen. Both source-isolated desktop requirements remain incomplete.
- qa-transport-ingress-1 exec39257 launched as independent TEST-ONLY sidecar: tests/mcp_transport_regression.py + own report. Task tests real compiled binary with --mock-backend over stdio/TCP; never native GUI. Expected first snapshot may RED; do not count mock images as real screenshots or false-green the cancellation semantics. No shared write scope with source writer.
- coordinator-lock-state-5.log21:06:20: macOS CGSSessionScreenIsLocked=true; Windows explorer8648 + LogonUI25084 in Session1. Read-only, no input or new failing GUI attempts.

## Review notes for integration
- Old transport3 left an unintegrated TransportGate in worker.rs before exit; this alone does not repair actual ingress. Transport4 has read it. Need race-free accepted→active transition, per-request cancellation without cancelling unrelated current work, ingress generation stamps, and truly bounded normal/oversized refusal.
- TCP current contract is **one authenticated connection per Host process**: clean disconnect shuts runtime down and exits listener; do not invent persistent reconnect requirement. QA may accept terminal clean exit as documented, but any surviving old reader must not continue operating. New Host process required for subsequent trials.
- Preceding209pass snapshot and binary hashes are NOT source freeze evidence. On source writer terminal report, hash Cargo/source set before build and after tests/build; any differences require revalidation. Only then call artifacts final candidates.
- No final goal completion / no waived ten-trial gate. No commit/push/worktree or parent-project modifications.

## New independent evidence and test-quality correction
Coordinator exec28625 ran actual candidate TCP harness: first cancelledobserve returned cleanimage after3.06s (same defect independently reproduced on TCP); then harness crashed on FileExistsError because perHosttoken path reused. Log coordinator-ingress-tcp-red-1.log; no GUI backend. QA1 later made partial token-name fix then hit maxturns35; verified terminal1. QA2 exec2608 launched after terminal, test-only write scope.
Intermediate harness T4b incorrectly accepted errors>0 for68frames and lacked pendingobserve; flagged as false-positive, not trustworthy coverage. Review qa-ingress-test-review-1.md. Correct action kind is text_input; coordinator's initial type_text suggestion was wrong and corrected in task/review based on actual source. Pending reviewer must verify final schema usage and not blindly trust either coordinator or worker report.
