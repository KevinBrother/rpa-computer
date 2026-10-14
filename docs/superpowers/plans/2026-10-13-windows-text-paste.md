# Windows Text Paste Implementation Plan

> **For agentic workers:** use subagent-driven-development; no worktree, commits or pushes. Windows verification via Claude CLI + GLM.

Status: implementation, optimized Windows tests, native service-station clipboard lifecycle and independent review complete; interactive GUI/DingTalk acceptance pending desktop lease release.

**Goal:** Expose explicit Windows host clipboard paste without SSH clipboard workarounds or hidden native fallback.
**Architecture:** Public `text_input.method` selects existing native path or a whole-text paste event. Host/backend owns clipboard mutation and tracked Ctrl+V, native-input remains native-only.
**Tech Stack:** Rust, windows-rs 0.58, existing runtime planner/held-state machinery, Windows owned GUI fixture.

## 1. Runtime/protocol slice
- [x] Add parser/schema regressions for paste, default/explicit native, unknown method, invalid/empty/multiline/Unicode text.
- [x] Observe RED before implementation via existing parsing expectations.
- [x] Extend Action/PlanEvent/InputEvent and map one whole payload event for paste, no scalar sleeps or key synthesis. Keep `kind()` text_input. Cross-check exhaustive matches.
- [x] Verify cancellation before event and truthful dispatch semantics; update tool metadata/tests and public README.

Write scope: src/runtime/**, src/backend/mod.rs (InputEvent only), README.md, tests/text_paste_contract.rs.

## 2. Windows backend slice
- [x] Add focused backend tests for explicit paste, empty no-op, unsupported platform, preflight errors/no input, held keys, partial chord cleanup and encoding.
- [x] Extend NativeInput with prepare_text_paste/verify_text_paste guards (default unsupported); DriverInput holds optional Windows prepared-paste receipt.
- [x] Add host-only clipboard module using OpenClipboard(owned HWND), EmptyClipboard, SetClipboardData(CF_UNICODETEXT), ownership-safe UTF-16 global allocation. Check focus/modifiers/clipboard sequence before chord.
- [x] Dispatch Ctrl-down/V-down/V-up/Ctrl-up through BackendCore held-state; no key loop retry, arbitrary delay, paste retry or restore timer.
- [x] Add required root Cargo windows features only. Cross-build tests and candidate host.

Write scope: src/backend/** excluding mod.rs, Cargo.toml/Cargo.lock. New helper modules should stay small; tests in separate module when appropriate.

## 3. Independent verification and integration
- [x] Inspect local Claude routing without printing secrets; record actual model response tags.
- [x] Claude executes Windows test binaries on acer-win from an isolated staging directory; capture native exit codes and logs.
- [x] Build/deploy candidate to a separate directory; do not overwrite/restart existing host.
- [ ] Run GUI via real Claude + GLM on the owned multiline fixture. BLOCKED: existing original host client holds the desktop writer lease; candidate returns lease_conflict. Do not kill or bypass it.
- [x] Independent read-only code review, repair findings, rerun impacted checks.
- [x] Final diff/format/build checks; report exact passing and untested cases, clipboard limitations, and candidate path. No commit/push.

## Review repair and evidence

Clipboard finalization now proves own live HWND + exact bounded published UTF-16 under a reopened lock before accepting the post-close sequence. See `windows_clipboard_memory.rs` and `windows_clipboard_tests.rs`; owner destruction is explicit before the chord. Native isolated regression confirms text persists after destruction.

Actual Windows results: old HEAD RED 3 failures/native101; final contract12 pass; release library291 pass/7 ignored; explicit noninteractive native lifecycle1 pass; fixture non-GUI self-test native0. Full debug attempt timed out at90s on an existing PNG-heavy test, not counted as passing; optimized full suite passed in6.16s without omissions. Actual CC assistant model tags are glm-5.3-flash via sonnet route. Detailed report: `.agents/reports/windows-text-paste-20261013.md`.
