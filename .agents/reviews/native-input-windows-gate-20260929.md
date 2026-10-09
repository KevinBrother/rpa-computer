# Windows LIVE GATE review — native-input (read-only), 2026-09-29

Decision: **NO BLOCKER for the controlled GUI fixture/calc DIAGNOSTIC** with the current
Windows paths. This is a gate for a diagnostic run only, not overall acceptance.

## S3 (VK_PACKET in wVk) — CONFIRMED FIXED, both layers
- Planner: all KEYEVENTF_UNICODE events now plan `vk: 0` (builder.rs:270-281 BMP,
  291-311 surrogate); `VK_PACKET` removed from production constants and exists only as a
  documented test constant (builder.rs:341).
- INPUT conversion: `to_input` passes `wVk: VIRTUAL_KEY(vk)` (=0 for unicode) with the
  UTF-16 unit in `wScan`; plain VK events get MapVirtualKeyW scan enrichment
  (windows.rs:86-103).
- Regression tests exist at BOTH levels: INPUT-level `unicode_input_has_zero_wvk_and_code_unit_in_wscan`
  and `supplementary_unicode_input_uses_surrogate_units_with_zero_wvk` (windows.rs:220-267),
  EventSpec-level `unicode_events_never_carry_vk_packet_as_input_vk` (builder.rs:640-658).
  The windows.rs-level tests are `cfg(windows)` and appear passing in the coordinator's
  acer-win run, proving they actually executed on Windows hardware.

## Critical real fields / counts / release / pairing — verified
- SendInput return used as the injected count (windows.rs:114); `single()` fails honestly on
  0-of-1 (windows.rs:117-127); `text_scalar` full-count success, then partial path releases
  only injected downs whose up was not sent (windows.rs:158-194).
- Cleanup detail is embedded in the error, and a partial CLEANUP is reported distinctly
  (windows.rs:168-193). No fake success path found.
- Surrogate order highDown, lowDown, highUp, lowUp with `up_of = [Some(2), Some(3), None,
  None]` (builder.rs:290-314) matches the contract's candidate scheme; partial-send cleanup
  table tested for sent=0..4 (builder.rs:663-675).
- NUL/C0 prevalidation landed at BOTH layers: driver `text_plan` rejects NUL + all C0/DEL
  before planning (builder.rs:252-266); host `validate_text` rejects NUL and every C0
  control except \t\n\r over the WHOLE payload before ANY dispatch (keys.rs:174-188,
  applied at dispatch_core.rs:148) — this prevents mid-text partials from control chars.
  Note: this host rule is cross-platform and stricter than the crate contract ("NUL only"),
  so identical text behaves the same on macOS/Linux; acceptable, flag for contract report.
- Absolute coords: virtual-screen origin/extent, inclusive 0..=65535, VIRTUALDESK, i64 math
  (builder.rs:165-194); wheel sign convention positive=down/right, overflow-checked
  (builder.rs:199-219); scroll(0) now a consistent no-op Ok (windows.rs:149-153).

## DPI compile fix — verified landed
`prepare_thread` sets Per-Monitor-V2 and VERIFIES via
`AreDpiAwarenessContextsEqual(GetDpiAwarenessContextForProcess(...))` re-read, errors never
swallowed (src/backend/windows.rs:38-74). Compiles against windows 0.58 bindings
(rust-lld release build log .agents/runs/native-win-release-lld2-20260929.log finished;
only a stale dead_code warning on the since-removed const).

## Gate evidence on file
- Repaired-crate unit run on real acer-win host: 47 passed, 0 failed
  (.agents/runs/native-win-unit-repaired-20260929.log) — includes the windows-only
  to_input tests above.
- Root release cross-build via rust-lld succeeds (same log dir).

## Residual caveats for the DIAGNOSTIC (not blockers)
- SendInput success counts do not prove target-app reaction (UIPI/locked desktop produce
  honest 0-of-N errors) — outcomes must stay `dispatched`, never success claims; consistent
  with runtime OUTCOME_TRUTH.
- Cosmetic: builder.rs:666-667 comment says "release both halves" while asserting only
  events[2]; builder.rs:652 `assert_ne!(vk, VK_PACKET)` is redundant with `assert_eq!(vk, 0)`.
- macOS/Linux blockers from .agents/reviews/native-input-initial-20260929.md (B1-B3) are
  out of scope for this gate and remain open.
