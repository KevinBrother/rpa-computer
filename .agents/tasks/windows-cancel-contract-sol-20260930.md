# Windows cancellation contract matrix — test-code implementation only

Approved scope: complete-actions design + docs/computer-use-acceptance-cases.md cancel01..10. Main root owner is implementing multiscreen under src/** and Cargo; focus fixture owner has acceptance-fixture/**. Your bounded sidecar materially strengthens execution-time cancel contracts; it is NOT GUI release acceptance or completion of allcancel.

Role: Codex gpt-6.1-sol authors code; CC+GLM executes all tests on Windows. You may compile-only (--no-run/cargo check), never run tests/binaries/GUI/SSH/CC or other agents. No Mac/Linux tests, no worktree/commit/push/reset. Do not alter production source or shared existingtests/Cargo. Root may be in flux; public API conflict report/cooperate, don't repair src.

Exclusive writes: new tests/windows_cancel_contract.rs + new tests/windows_cancel_contract/*.rs (small modules); new docs/windows-cancel-contract-matrix.md; own .agents/reports/windows-cancel-contract-sol-20260930.md. No others. Use cfg(target_os="windows") to make execution boundary clear. Read actual production public Worker/McpService/ingress APIs and existing tests/contract and transport_lifecycle for conventions, but don't copy whole harnesses or weakenassertions to matchbugs.

Implement bounded deterministic production-Worker contract tests covering:
- cancel01 actual drag alreadypressed then computer_pause,
- cancel02 drag with at leastone subsequentmove then computer_close,
- cancel03 key_hold press then pause,
- cancel04 chord after modifierpress then cancellation through actual ingress/CancelHandle as appropriate,
- cancel05 type_text with at leastone actualbackend character dispatch then cancel,
- cancel06 production McpService EOF during action, proving shutdown/cleanup (a cancel flag alone MUST NOT be called EOF),
- cancel08 fakebackend release_all failure retains truthful unknown/failed cleanup and quarantines/blocks furtherinput; never use realnative injection,
- cancel09 repeatedclose truthfulidempotency with cleanup counts,
- cancel10 resume rejectsoldbased_on, requiresfreshobserve and canrecover without replayingoldaction.
- cancel07 actualnetwork disconnect MUST remain explicitly deferred to existing realtransport suite + future GUI; do NOT create passing placeholder/ignoredemptytest or claim simulatedcancel testsocketEOF. No sockets/Host/externalprocess/desktop/registry/global hooks inthissuite, so CC can safely runpureSession0.

Use injectable test Backend implementing actualtrait, record native input events and synthetic heldstate plus cleanup attempts; Worker real path handles parsing/planning/timing/cancel/requestledger. Backend must NOT directly setcancel or clearheld to fabricate tested effect. Synchronize on realdispatch reaching an eventbarrier, then supervisor control call runs while actionstillactive. Do not sleep-and-guess, do not serializecancel behind blockingworker unknowingly. All barriers recv_timeout+RAII unconditionalrelease including panic; boundjoin/nohangingthread. Validate actionreply input_outcome/cleanup/status and no businessinputafter observedcancel (release-only cleanupallowed), exact evidence of partialdispatch, heldstateempty iffcleanupsuccess, noautomaticreplay. Include invalid/late/wrongsession cancellation negative where APIpermits; no blanketacceptederrorcodes. Keep assumptions auditable.

Only testcode now, so do not forceartificialred: missing semantic behavior found byCC becomes actionableproductiondefect forrootowner. Report actual compile-only evidence, exact testnames/count and contractmapping, mockvsrealGUI limitation. Freeze fullsourcehashes+artifactcommands forCC after rootSOURCE_FROZEN. Rootbuildenv maybereused with NEW CARGO_TARGET_DIR from .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh; don't overwrite oldartifacts. If root transientcompilerfails note and report ratherthan touchingit or creating stubproduction.
