This is the follow-up CORE repair task. DO NOT START until previous core worker terminal status is verified. All implementation done by Claude Code, no commit/push/worktree. Exclusive writes core ownership from README: Cargo*, src/lib.rs, src/runtime/**, tests/protocol.rs, .gitignore. Do not touch backend, transport or independent QA files.

Read CONTRACT and .agents/reviews/initial-review.md. Fix current compile issues first using installed image 0.24 actual API, Rust borrow-safe disjoint field borrowing, visibility/imports and type inference, and run focused tests before writing more code. Current errors are in .agents/runs/coordinator-compile-3.log; they may be stale, always rerun first.

Review findings to address with regression tests:
- Result registry must retain exact original outcome metadata for all registered requests (<=1000), even if old PNGs evicted. A replay or get_step after 256 requests cannot become falsely unknown. Compare canonical original request body or collision-resistant representation, not risk a noncryptographic hash collision changing side effects unnoticed. Never replay inputs on cache eviction.
- Account ALL retained image bytes, including result metadata references and current/previous observations. store_observation currently drops last then overwrites it with current again when over budget. Use a single-counted image cache or strip PNG from older results; bounded tests with controllable limits. Preserve input metadata on image eviction and surface image availability accurately.
- Text input split into bounded per-event chunks with cancellation checks, not one 4096-char native call. Add cancellation mid-text test; no real input. Respect key/button release on every partial or cancelled path.
- Total session deadline cannot be extended by action activity; input phase deadline cannot silently shorten a requested max hold and report success. Make advertised limits match execution behavior.
- Use precise outcome on first failed event (possible side effects). No success claim when cleanup failure cached, no clearing Faulted merely by another close/open without successful cleanup.
- Schemas/tool descriptions must reflect actual behavior and not cause duplicate observation requirements when step already returns a fresh image.
- Tests require real close idempotency; malformed unrecognized fields must not silently change action semantics.
- .gitignore must exclude sensitive/generated .agents/runs transcripts and captures plus __pycache__, not just target. Keep planning task/report Markdown tracked as desired. Do not hide source/tests from review.
- Preserve standalone runtime/backend/transport boundaries, modular <1000-line files. No model API deps.

Run cargo fmt --check (coordinate changes only own files) and cargo test --lib / --test protocol. Independent tests may exercise defects and must not be weakened. If other worker compile error prevents tests, report exact file/error and use bounded isolation only if necessary; don't label isolated shim tests full product tests. Write .agents/reports/core-fix-1.md with actual commands, counts, remaining gaps, no fabricated green status.
