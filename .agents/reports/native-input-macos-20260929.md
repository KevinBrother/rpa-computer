# macOS native-input worker report (2026-09-29)

Task: `.agents/tasks/native-input-macos-20260929.md`
Owned files: `crates/native-input/src/macos.rs` (only). No shared file (lib.rs / types.rs / session.rs / Cargo.toml / other platforms) was created or modified.

## Status: implemented, pure tests green, live GUI injection NOT verified

## Implementation summary (`crates/native-input/src/macos.rs`)

Direct FFI only — no wrapper crates, no AppleScript/shell/clipboard:

- **Frameworks**: CoreGraphics (`CGEventSourceCreate` HID-system-state source, `CGEventCreateKeyboardEvent`, `CGEventCreateMouseEvent`, `CGEventCreateScrollWheelEvent2`, `CGEventPost` on `kCGHIDEventTap`, `CGEventSetFlags`, `CGEventSetIntegerValueField`, `CGEventKeyboardSetUnicodeString`, `CGEventCreate`+`CGEventGetLocation` probe for cursor position, `CGMainDisplayID`), CoreFoundation (`CFRelease`, `CFDataGetBytePtr`), ApplicationServices (`AXIsProcessTrusted` — never prompts), Carbon TIS (`TISCopyCurrentKeyboardLayoutInputSource`, `TISGetInputSourceProperty`, `UCKeyTranslate`, `LMGetKbdType`).
- **RAII**: `CfRef` / `Event` / `EventSource` release all CF refs on drop; null event creation fails with `input_failed` before any post.
- **Permissions**: `Platform::new` returns `permission_denied` if `AXIsProcessTrusted()` is false (no prompt); `no_display` if `CGMainDisplayID()==0`.
- **Keys**: named keys map to Apple kVK_* constants. `Key::Character` is restricted to ASCII letters/digits (contract "physical/layout key"); resolved to the keycode that produces the character (unshifted) on the *current* layout via `UCKeyTranslate` scan over keycodes 0..=127 (`kUCKeyActionDisplay`, no-dead-keys). If TIS layout data is unobtainable, a documented ANSI-US fallback map is used. Any failure returns `invalid_key` **before** any event is posted.
- **Modifier chords**: Platform tracks held Control/Shift/Alt/Meta (its own presses) and sets `CGEventFlags` (main flag + left-device NX mask, enigo-compatible) on all subsequently posted events; release removes them. Press events carry the new flag set, release events the reduced set.
- **Mouse**: `move_mouse` posts CoreGraphics-point coordinates, using `LeftMouseDragged`/`RightMouseDragged`/`OtherMouseDragged` when *our own* button calls hold a button, `MouseMoved` otherwise. `button` posts down/up at the real cursor location (probed via a non-posted `CGEvent`) with click state 1. Tracked button state is only from our own calls.
- **Scroll**: `CGEventCreateScrollWheelEvent2`, line units; positive length = DOWN/RIGHT encoded as negative wheel value (CoreGraphics convention); length 0 posts nothing (honest no-op).
- **text_scalar**: NUL → `invalid_text` before any effect; `\n` and `\r` each produce exactly ONE Return click (press+release); `\t` one Tab click; all other scalars injected via `CGEventKeyboardSetUnicodeString` with the char's full UTF-16 (supplementary planes as one surrogate pair on both down and up events), modifier flags cleared so text ignores chords. No clipboard, no sleeps, no screenshots. Partial-failure cleanup: if the up-event cannot be created after the down was posted, a release is re-attempted and the error is reported (never faked success).
- Known API limitation: `CGEventPost` returns void, so a rejected post is undetectable via API; mitigation is the `AXIsProcessTrusted` precheck plus null-checks on all event creation.

## Tests recorded (12, all pure/non-live — no `CGEventPost` under test)

Run in `.agents/runs/cargo-native-macos/scratch` (see "Compile evidence"):

1. `named_keycodes_match_apple_constants` — spot-checks 20 named keys.
2. `every_named_key_has_a_keycode` — all 30 named keys resolve.
3. `character_keycodes_resolve_layout_aware` — real TIS/UCKeyTranslate lookup: 'a' resolves, 'A' same physical key, '1' differs from 'a'.
4. `character_outside_ascii_letters_digits_is_rejected` — `中`, ' ', '\n', '\0', 'é' → `invalid_key`.
5. `text_scalar_rejects_nul_only` — NUL `invalid_text`; `\n`/`\r`/`\t`/space/`é`/`中`/U+1F600 accepted.
6. `utf16_encoding_handles_supplementary_planes` — U+1F600 → `[0xD83D, 0xDE00]` length 2.
7. `scroll_positive_means_down_and_right` — (3, Vertical) → wheel1=-3 etc.
8. `move_uses_dragged_type_only_while_our_button_is_held` — incl. left priority when multiple held.
9. `button_event_types_are_down_and_up`.
10. `modifier_flags_accumulate_and_clear` — incl. device masks and non-modifier no-op.
11. `key_code_rejects_unmappable_character` — '/' → `invalid_key`.
12. `text nul/scroll helpers covered above`; result: **11 macos tests pass, 0 fail** (`character_keycodes_resolve_layout_aware` initially failed, fixed — see below).

**Debugging fix worth recording**: the first UCKeyTranslate probe returned `paramErr` for every keycode. Two ABI/logic bugs were found and fixed: (1) `UCKeyTranslate` needs `CFDataGetBytePtr(layoutData)` — the byte pointer — not the `CFDataRef` itself; (2) the signature args are 32-bit (`OptionBits`, `UniCharCount`) and were first declared 64-bit. Both verified with a standalone probe under `/tmp/tiskey.rs` before being applied.

## Compile evidence

- **Real shared skeleton**: `cargo check --lib` PASSES against the actual `crates/native-input` (target dir `.agents/runs/cargo-native-macos`), zero warnings in macos.rs. So the macOS platform code compiles as part of the real crate.
- **Test run**: `cargo test --lib` against the real skeleton currently fails to COMPILE only in core-owned `session.rs` test code (missing `use std::cell::RefCell;` etc. — 16 errors, none in macos.rs). Therefore the 11 macos tests were executed in a scratch copy at `.agents/runs/cargo-native-macos/scratch` whose shared files are byte-identical copies (Cargo.toml features were already fixed by core by then; only `types.rs`/`session.rs` transient compile bugs were patched in the copy). Scratch-only; original tree untouched by me.
- **Cross-platform build**: NOT verified (I am the macOS worker; `cfg`-gated windows/linux_x11 code untouched). No claim made.

## Issues in files I do not own (for core worker)

1. `src/session.rs` test code has compile errors: missing imports (`RefCell`, `Rc` — `src/session.rs:314`, and 3 more `E0433/E0425`); blocks `cargo test --lib` for everyone.
2. `session.rs` test `release_all_attempts_every_held_item_and_retains_failures` (runnable in scratch after import patch) asserts against an expected message but the actual message does name the failed item correctly (`release_all: 1 item(s) failed ... key(shift): mock release failure for key(shift)`); the assertion itself looks stale — please re-check.
3. Earlier today `Cargo.toml` had `UI_HiDpi`/`UI_Input_KeyboardAndMouse`/`UI_WindowsAndMessaging` instead of `Win32_*` and `types.rs` was missing `use std::fmt;` — both already fixed by core while I worked; noted only for the record.

## Not verified / blockers

- **Live GUI injection not verified** (contract forbids live desktop input by code workers): real key/button/scroll/text effects, chord behavior in actual apps, and whether text via `CGEventKeyboardSetUnicodeString` lands in the focused field all need coordinator-supervised GUI acceptance.
- **CGEventPost void return**: cannot detect a post rejection; needs GUI acceptance to be meaningful.
- `move_mouse` posts points directly; macOS clamps to display bounds — behavior on out-of-bounds coordinates is untested by design (no live run).
- No `Send`/`Sync` promises made (thread-bound device), per contract.
