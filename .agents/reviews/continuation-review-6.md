# Continuation review 6 — coordinator, 2026-09-24 21:55 CST

## Real progress / independent evidence
- Native regression independently reproduced: coordinator-resume-race-red-1.log, 0passed1failed exit101. Runtime resume erased stop delivered inside geometry; not capture/mock artifact.
- QA ingress3 was terminal0 but T5 was still a harness error (superseded observation basis; step response observation metadata is nested). Assigned surgical QA4. Its edits survived provider429.
- Independent QA4 harness run against SAME older knownbroken binary: coordinator-ingress-contract-4.log, **32passed8failed**, exit1. Source hash and binary hash before/after identical. T5 now genuinely executes mock text_input (dispatched, observation available, nested observation ID, imageblock) on both transports; T3 genuine disconnect and T4 explicit overflow+EOF pass. Remaining8 failures: T1 three trials pertransport lostcancel, T2 one pertransport stalequeuedresume. No GUI evidence implied.
- Python harness SHA a0b299ed7f6ca1e0763b453829b2cb89d71d6e957bdeee858cd2640cead6f944; binary SHA f7b99ee375c4f4d338859aa96fbaf0a355392ec7a0a8f15426d7d0c12e0b6a50. Candidate NOT rebuilt, still knownbroken.
- Read-only locks21:44: coordinator-lock-state-6.log: Mac locked=yes; Windows explorer8648/LogonUI25084 Session1. No GUI retry/input or remote Host launch.

## Provider failure and CURRENT source status
- cancel-handoff-5 exec37787, stdio-tests-repair-5 exec90286, qa-ingress-contract-4 exec27566 ALL confirmed terminal exit1 provider429. No reports completed. Their partial edits retained; NOT accepted.
- Source now DOES NOT COMPILE: coordinator-check-after-rate-limit-1.log, cargo check --all-targets --offline exit101,7errors. Partial unified ActiveRequest and native WorkItem epoch signatures not wired to constructors/callers. Do not cite old209pass snapshot as currentgreen.
- Recovery cancel-handoff-6 exec78802 started SERIAL after all writers terminal, owns src/mcp/** + transport_lifecycle.rs (not Python). First request still429 retrying at21:55; no implementation output yet. Do not switch model/credentials/globalconfig.
- Goal stillactive: first continuation encountering provider429, not3consecutive goalturns. Do not markblocked prematurely. Desktop-only blocker does not by itself justifyblocked while code can progress.

## Remaining concrete review concerns (partial code, not final acceptance)
1 Native epoch patch currently postchecks only cleanopen/resume; capture cancellation still returns success. Pre-native staleepoch guard missing. CancelHandle still flagstore then genbump; can miss overlappingstop. Original task requires correcting these.
2 Cancelledopen may have internally created a session but outwardreply is error; patch suppresses lifetimeclock (comment says intentionally), leaving hidden live session without watchdog. Need rollback/cleanup with honestfailure, or observable identity/bookkeeping; task6 explicitly requires this.
3 Unified tracker still returns true for queued cancellation and ingress always globallycancel; callers not integrated. TombstonedID removed from queued allows duplicate reuse to erase tombstone. Capacity must count entire queued/active/tombstoned population. Same-lock active-match PLUS cancel effect needed (not unlocked match→globalcancel race). Task6 covers semantics.
4 Rewritten stdio/tests.rs fixture has NEW read_for_id bug: after first unmatched frame, pushes pending; next_frame always removes firstpending then read_for_id pushes it back, never reads socket. Need stash unmatched locally while reading wire (or separate wire reader), retain order, direct unit regression for requesting idB when idA precedes.
5 next_frame(timeout200ms) still uses fixed5sec sockettimeout (not remainingbudget); read_line_bounded unused, comment falsely says stashespartial but returnsNone dropping localchunk. Use one bounded wire-read helper with persistpartial, deadline-aware set_read_timeout(remaining), then read_for_id cannot recycle its ownpending forever. Don't call cargo test hang PASS; ensure guards drop before joins.
6 Overflow tests loop while host.outcome().is_none(); if host already exited before reader starts, they readzero frames then fail thoughrefusal buffered. Need drain until truewireEOF/boundeddeadline even if outcomealreadyset, count exactlyone refusal; timeout must not countEOF. Source could be correct despite this fixturefalsefailure.
7 QA4 per-call MSG_DONTWAIT socket send is improvement; local backpressure check passes. It does not currently catch BlockingIOError after select race; this is residual robustness issue, not current observedfailure. Do not blindly call current Python harness perfect/Windowsportable.

## Next
- Confirm exec78802 terminal before any newwriter. Providerrecover then finish task6 and fixture corrections, independently review+test, freezeallsource then dualreleasebuild/hash.
- Preserve alloriginal GUI gates, sourceisolation,8toolsonly,10trials/task/platform>=8success. Neither build nor mocktests count GUI.
