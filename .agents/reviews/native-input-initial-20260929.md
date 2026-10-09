# Independent spec/code review — native-input (GLM, read-only), 2026-09-29

Scope: `crates/native-input` + host integration. Findings verified against current tree and
local Xcode SDK headers. Writers were active; line numbers are as-of-read (macos.rs 889 L,
linux_x11.rs 541 L, keysym.rs 349 L, windows.rs 182 L, builder.rs 650 L, session.rs 599 L,
types.rs 178 L, lib.rs 53 L, dispatch.rs 611 L, dispatch_core.rs 224 L, execute.rs 493 L).

## Verified test/compile evidence
- `cargo test` in crates/native-input (macOS host): 27 passed, 0 failed.
- Host workspace `cargo test` (macOS host): 35 passed + 3 passed, 1 ignored, 0 failed.
- SDK evidence: `CGEvent.h:107-110` (CGEventCreateScrollWheelEvent2 = 6 params);
  `UnicodeUtilities.h` UCKeyTranslate prototype: `UniCharCount maxStringLength,
  UniCharCount *actualStringLength`; `MacTypes.h:469`: `typedef unsigned long UniCharCount`.

## Blockers (highest risk, unfixed coordinator findings)

B1. macOS `CGEventCreateScrollWheelEvent2` FFI declared/called with 5 args
    (macos.rs:173-179 decl; call at macos.rs:713). SDK CGEvent.h:107-110 has SIX params
    (wheel3). Callee reads wheel3 from an unset register -> garbage third wheel delta
    (corrupt scroll) or crash. Coordinator finding #1 NOT fixed.

B2. macOS `UCKeyTranslate` FFI width mismatch (macos.rs:212-223): `maxStringLength: u32`
    must be `c_ulong` (UniCharCount = unsigned long = 64-bit, MacTypes.h:469) and
    `actualStringLength: *mut u32` must be `*mut c_ulong` — callee writes 8 bytes through
    that pointer into a 4-byte stack slot (adjacent locals `len`/`dead_state`, macos.rs:514-516)
    -> stack corruption. Caller passes `UC_STRING_BUF_LEN as u32` (macos.rs:526).
    Coordinator finding #3 NOT fixed. (Note: CGEventKeyboardSetUnicodeString got c_ulong
    right at macos.rs:185 — same width rule, inconsistent.)

B3. Linux `text_scalar` existing-mapping path types the WRONG character for uppercase:
    `find_mapped_keycode` (keysym.rs:172-197) matches a keysym in ANY column, including
    shift-level columns; `linux_x11.rs:420-424` then clicks that keycode WITHOUT Shift.
    On any standard layout `text_scalar('A')` -> keysym 0x041 found in column 1 of keycode
    38 -> types 'a'. Coordinator finding L5 NOT fixed. Test `find_mapped_keycode_finds_any_column_but_not_modifiers`
    (keysym.rs:319-340) asserts 0x041 -> keycode 38 and thus self-confirms the bug.

## Significant (unfixed coordinator findings)

S1. Linux `choose_spare_keycode` (keysym.rs:138-167) still ignores the pressed-key bitmap;
    no `XQueryKeymap` anywhere in the file. A physically held all-NoSymbol keycode can be
    remapped under the user. Coordinator L2 NOT fixed.

S2. Linux global `XSetErrorHandler` swap with only an AtomicBool (linux_x11.rs:110-134).
    Not serialized against other in-process Xlib users (screenshot stack): (a) errors from
    unrelated displays during the guard window set X_ERROR_SEEN -> false input failures;
    (b) unrelated code's errors hit `record_x_error` returning 0, silently changing the
    default handler's behavior. No forwarding of unrelated-display errors.
    Coordinator L3 NOT fixed.

S3. Windows `KEYEVENTF_UNICODE` events set `wVk = VK_PACKET (0xE7)` (windows.rs:81-82 via
    builder `vk: VK_PACKET`, builder.rs:264-306). MSDN KEYBDINPUT: with KEYEVENTF_UNICODE
    `wVk` must be 0. May work in practice, but it is a spec violation and unverified live —
    verify on the live Windows host before trusting the text path (task said actual text
    success needs GUI verification anyway).

S4. Linux temporary-mapping async race (map -> click -> XSync -> restore, linux_x11.rs:457-469)
    remains the coordinator's L1 class: slow clients may re-query the restored mapping.
    Mitigated honestly: module docs report the limitation (linux_x11.rs:7-14). Per contract
    this is reportable instead of a hard blocker, but note `Ok(())` is returned while the
    race is only documented, not eliminated — coordinator wanted persistent reservations or
    pre-effect rejection; neither implemented.

S5. macOS: no press->release keycode cache (coordinator #4). `key()` re-resolves via
    `char_to_keycode` on every direction (macos.rs:695-703); a layout change between press
    and release mismatches the codes. Medium-low practical risk.

S6. macOS: coordinator #2 partially fixed — `CFDataGetBytePtr` now used (macos.rs:511), but
    `CFDataGetLength` is never called (not even declared, macos.rs:191-195) so the layout
    blob length is unvalidated before UCKeyTranslate.

## Minor / inconsistencies (fix opportunistically)
- `scroll(0)`: macOS returns Ok (macos.rs:706-710), Linux Ok (linux_x11.rs:366-368), Windows
  returns `input_failed` (builder.rs:197-202). Cross-platform divergence; contract silent.
- CGEventType declared `i64` in FFI (macos.rs:36-45, 167-172) though the SDK enum is
  uint32_t. Register-passed so benign today, but violates the "exact Apple widths" rule.
- `MappingGuard::restore_now` sets `restored = true` even when restore FAILED
  (linux_x11.rs:499-503) -> Drop backstop is skipped after a failed explicit restore.
- Linux `scroll` release-half failure returns Err without retry (linux_x11.rs:379-386),
  unlike `click_keycode`'s release retry (linux_x11.rs:257-265); wheel button can be left down.
- `XSync` `status == 0` treated as failure (linux_x11.rs:215-220, 490, 521). Could not verify
  libX11's return contract on this host (coordinator L4 asked to check); the X_ERROR_SEEN
  check already covers the error case — consider dropping the status check or verifying
  against libX11 source before shipping.
- builder.rs:619-621 test comment says "release both halves" but asserts only `events[2]`
  (comment wrong, code right). Cosmetic.
- Contract literal listed windows-crate features as "UI_..."; actual Cargo.toml correctly
  uses `Win32_UI_...` prefixes for windows 0.58 (crates/native-input/Cargo.toml) — correct
  for the real crate, no action.

## Contract conformance checks that PASSED
- Public abstraction independence: `types.rs`/`session.rs`/`lib.rs` have zero host, serde,
  screenshot or network deps; crate Cargo.toml clean; `Driver` seam matches contract
  signature (types.rs:129-139); enums Copy/Debug/Eq/PartialEq/Hash (types.rs:14-88);
  InputError codes as specified (types.rs:90-116).
- Single held-state authority: host wraps the RAW driver in `BackendCore` and does NOT use
  `Input` (dispatch.rs:10-14, desktop.rs:52-54; `native_driver()` at backend/{macos,windows,linux}.rs);
  `Input` exists for standalone users. No double authority found.
- Held-state semantics: press recorded before dispatch, release removed only on success,
  release_all attempts all and retains failures — implemented identically in session.rs:83-198
  and dispatch_core.rs:104-206, with meaningful mock tests (not self-confirming).
- Text integration: NUL rejected before any dispatch; CRLF normalized once
  (dispatch_core.rs:148-155); \n/\r/\t each exactly ONE click; Return/Tab clicks flow
  through the same held-state machine. Cancel/deadline: per-event boundary cancel checks,
  absolute input-phase deadline, mandatory release pass with dedicated allowance, honest
  partial/dispatched/not_started mapping (execute.rs:81-311); mid-text cancellation test is
  deterministic via a test-only seam (execute.rs:353-380).
- Windows absolute coords: virtual-screen origin/extent, inclusive 0..=65535, VIRTUALDESK,
  negative origins via i64, clamp + monotonicity tests (builder.rs:162-191, tests 450-510).
- Windows partial-send cleanup: `up_of` plan correctly releases only injected downs whose
  up was not sent, including surrogate halves (builder.rs:59-67, 314-324; windows.rs:148-181).
- Linux session validation: explicit Wayland rejection, DISPLAY required, XTEST required,
  keycode-range sanity (linux_x11.rs:148-204); blocking calls documented in module header.
- macOS: AXIsProcessTrusted without prompt (macos.rs:576-583); dragged event types while
  buttons held (macos.rs:319-329); modifier flag accumulation incl. device masks
  (macos.rs:355-377, tests 866-882); unicode cleanup path re-posts a matching up on partial
  failure (macos.rs:655-669).

## Bottom line
Do NOT proceed to live GUI validation before fixing B1-B3 (B1/B2 are ABI/memory-corruption
class on the primary macOS path; B3 silently corrupts text on Linux). S1-S2 are the next
tier. Everything else can ship after review sign-off.
