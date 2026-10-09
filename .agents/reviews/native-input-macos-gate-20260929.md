# macOS LIVE GATE review — native-input (read-only), 2026-09-29

Decision: **NO BLOCKER for the controlled GUI diagnostic** with the current Mac paths.
Gate for a diagnostic run only, not overall acceptance. Reviewed current files:
`crates/native-input/src/macos/{ffi,keymap,mod}.rs`, `lib.rs` wiring, host
`src/backend/macos.rs`. Tests: filtered `cargo test macos` = 19 passed, 0 failed
(targeted run only; no broad suite re-run per task).

## Prior findings — all verified fixed against SDK, not stale reports
- **B1 (ScrollWheelEvent2)**: FFI now declares SIX params (ffi.rs:116-123), call passes
  `wheel3 = 0` (mod.rs:349-359); doc cites CGEvent.h:107-109. Verified locally:
  CGEvent.h has the 6-param prototype.
- **B2 (UCKeyTranslate widths)**: `maxStringLength: c_ulong`, `actualStringLength:
  *mut c_ulong` (ffi.rs:174-175); caller passes `UC_STRING_BUF_LEN as c_ulong` and
  `&mut len: c_ulong` (keymap.rs:236-251). Verified locally: UniCharCount = unsigned long
  (MacTypes.h:469). CGEventKeyboardSetUnicodeString remains c_ulong (ffi.rs:127-131).
  CGEventType is now `u32` throughout (ffi.rs:52-61, 108-113) — the old i64 mismatch is gone.
- **S5 (press/release cache)**: `CharacterKeycodeCache` retains the press-time code and
  reuses it for release; commits happen ONLY after a successful post, so a failed release
  keeps the physical code for retry (mod.rs:121-172, 317-341). Cache mutations
  (commit_press/commit_release) are provably post-success (mod.rs:330-338). ASCII case
  aliases one physical key; repeated press never overwrites (mod.rs:162-165). Tests cover
  layout-switch reuse, failed-release retention, uncommitted-press non-poisoning
  (mod.rs:482-594).
- **S6 (CFData validation)**: `CFDataGetBytePtr` + `CFDataGetLength` both validated BEFORE
  UCKeyTranslate; length must be >= K_UC_KEY_LAYOUT_HEADER_SIZE (256) (keymap.rs:224-230,
  25). ANSI fallback removed entirely — an unresolvable layout is now an explicit
  `input_failed`/`invalid_key`, never a guessed wrong physical key (keymap.rs:173-207).
- **TIS mutex scope**: `TIS_LOCK: Mutex<()>` held across the ENTIRE
  TISCopy→TISGetInputSourceProperty→CFDataGet*→UCKeyTranslate loop lifetime, with poison
  recovery (keymap.rs:17, 196, 200-254). Scope matches the "entire TIS + UCKeyTranslate +
  CF layout-query lifetime" requirement.
- **GUI-embedding thread restriction**: clearly documented — TIS/TSM abort on concurrent
  threads; mutex serializes only this library; GUI embedding additionally requires
  main-thread TIS (keymap.rs:11-16 and 181-186). Documented-not-enforced is the honest
  limit for a library; host is a non-GUI-embedded worker process, so this is acceptable
  for the gate.
- **No unsafe events before permissions check**: `Platform::new` runs AXIsProcessTrusted
  (never prompts) and CGMainDisplayID before creating the event source, and nothing posts
  before that (mod.rs:186-207); host `create_driver` double-preflights and maps errors
  honestly with permission_denied (src/backend/macos.rs:59-74).

## Coordinator SDK constants — verified in installed headers this session
- `kCGScrollEventUnitLine = 1` (CGEventTypes.h:48) — the old code used 0 (Pixel); fixed.
- `NX_NONCOALSESCEDMASK = 0x00000100` (IOLLEvent.h:268; CGEventTypes.h:98 maps
  kCGEventFlagMaskNonCoalesced to it) — old 0x0100_0000 wrong; fixed (ffi.rs:71).
- NX_DEVICEL* masks 0x01/0x02/0x08/0x20 (IOLLEvent.h:253-258) — correct (ffi.rs:73-76).
- Regression test pins all of these from SDK-copied values (ffi.rs:256-267).

## Unicode cleanup — verified
`post_unicode_char` posts down with the UTF-16 string; if the up creation fails, a
cleanup release is attempted with the same payload, and a CLEANUP FAILURE is now merged
into the reported error instead of dropped (mod.rs:241-293). Event RAII (CfRef/Event Drop)
unchanged and correct; TISGetInputSourceProperty data follows the get-rule, never released
(keymap.rs:221-224).

## Host adapter (src/backend/macos.rs) — unchanged in scope, consistent
`prepare_thread` is an honest no-op (no DPI transform on macOS); permission preflights
never prompt; capture-side checks are separate from input. Single held-state authority
unchanged (BackendCore over raw driver; `Input` unused by host).

## Residual notes (not blockers)
- `text_scalar` driver-level validation now rejects C0/DEL except \n\r\t (keymap.rs:159-171)
  — stricter than the crate contract's "NUL only", consistent with the host
  `validate_text`; include in the contract report.
- Double-click limitation may be documented separately per the task; no new API needed —
  `CGEventSetIntegerValueField(clickState=1)` stays at 1 (mod.rs:225), which is correct for
  single clicks.
- `CfRef::retain` performs no CFRetain (name cosmetic only); the objects it wraps are
  +1-owned, so refcounts are correct.
- The keycode cache grows only with distinct pressed character keys and is cleared on
  release; a chord abandoned without release retains one entry — bounded and harmless.

## Gate verdict
All previously flagged macOS blockers (B1, B2) and significant items (S5, S6) plus the
coordinator's constant fixes are landed and SDK-verified; unit evidence is green. Safe to
proceed to the controlled GUI diagnostic on this Mac. Actual text/chord success still needs
the live run to confirm (unit tests cannot prove posting behavior).
