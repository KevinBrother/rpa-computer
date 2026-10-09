# native-input platform repair (macOS + linux_x11) — 2026-09-29

Task: `.agents/tasks/native-input-platform-repair-20260929.md`. Scope owned and
modified: `crates/native-input/src/macos.rs` → `macos/{mod,ffi,keymap}.rs`,
`crates/native-input/src/linux_x11.rs`, `crates/native-input/src/linux_x11/keysym.rs`.
No shared core files, no Windows files touched. Real implementation, not review-only.

## Verification (all with CARGO_TARGET_DIR=.agents/runs/cargo-native-repair)

- `cargo test -p rpa-native-input --target aarch64-apple-darwin`: **29 passed, 0 failed**
  (pure tests only; no input injection, no GUI).
- `cargo clippy -p rpa-native-input` (host + `x86_64-unknown-linux-gnu`): clean, 0 warnings.
- `cargo check -p rpa-native-input --target x86_64-pc-windows-msvc`: clean.
- `cargo check --workspace` (host): clean.
- `rustfmt` run ONLY on the five owned files (package-wide fmt avoided; windows.rs
  belongs to another worker).
- No live desktop input / GUI / SSH performed. No commits. No subagents.

## macOS repairs (FFI verified against installed Xcode SDK headers, not Enigo)

1. **Scroll SIX arguments (critical, fixed).** `CGEventCreateScrollWheelEvent2`
   was declared with 5 parameters. SDK `CoreGraphics/CGEvent.h:107-109` declares
   SIX: `source, units, wheelCount, wheel1, wheel2, wheel3`. Added `wheel3: i32`,
   always `0`; vertical scroll posts `wheelCount=1` (wheel1 only), horizontal
   `wheelCount=2` (wheel2 only). Verified all other CG FFI widths against SDK:
   `CGEventType`/`CGMouseButton`/`CGScrollEventUnit`/`CGEventTapLocation`/
   `CGEventField` are `CF_ENUM(uint32_t)` → fixed event-type parameters from
   `i64` to `u32`; `CGEventSourceStateID` is `int32_t` (i32 kept);
   `CGEventFlags` u64; `CGEventKeyboardSetUnicodeString` `UniCharCount` =
   `unsigned long` → `c_ulong` (was already correct).
2. **UCKeyTranslate ABI (critical, fixed).** `maxStringLength` and
   `actualStringLength` were declared `u32`/`*mut u32`; they are
   `UniCharCount` (`unsigned long`, CFBase.h:139) → `c_ulong`/`*mut c_ulong`.
   The old widths corrupt the argument frame on a 10-argument call.
   Other parameters verified: `virtualKeyCode`/`keyAction` UInt16,
   `modifierKeyState`/`keyboardType`/`keyTranslateOptions` UInt32/OptionBits,
   `deadKeyState *mut UInt32`, return `OSStatus` = i32.
3. **CFData layout-pointer validation (fixed).** `char_to_keycode` now calls
   `CFDataGetBytePtr` (it already did) AND `CFDataGetLength`, rejecting null
   pointer or length < 256 (kUCKeyLayoutHeaderSize) before any `UCKeyTranslate`.
   Previously the raw `CFDataRef` path risk existed; byte-pointer + length
   validation closes it.
4. **Press keycode cached for release (fixed).** `Platform` keeps
   `character_keycodes: HashMap<Key, u16>`: press resolves via the current
   layout and caches; matching release consumes the cached code, so a layout
   change between press and release cannot split a chord across two physical
   keys. Named keys are constants and are not cached. Only this driver's own
   presses are cached; unrelated physical keys are never touched or released.
5. **No silent ANSI fallback (fixed).** The silent hard-coded US-ANSI fallback
   when TIS query fails was REMOVED. When the layout source, Unicode layout
   data, or byte pointer/length is unavailable, an explicit `input_failed`
   error is returned ("cannot be resolved without guessing a layout"); when a
   character is produced by no keycode, `invalid_key`. No known-unsupported
   character ever falls through to a guessed physical key.
6. **Partial Unicode cleanup detail preserved (fixed).** If the up event of
   `text_scalar` fails after the down was posted, the cleanup release is
   attempted; if that also fails, the returned error message contains BOTH the
   original failure and the cleanup failure (original error code retained).
7. **Split into private modules (done).** `macos.rs` (914 lines) →
   `macos/ffi.rs` (FFI decls + CF/CGEvent RAII), `macos/keymap.rs` (keycode
   resolution + UTF-16 encoding + its tests), `macos/mod.rs` (driver + helpers
   + tests). Test-only note: `UCKeyTranslate`/TIS are documented "Not thread
   safe" in HIToolbox; the two layout-touching tests serialize on a test-only
   mutex (production driver is thread-bound, no Send/Sync, per contract).
   Without the lock, parallel test threads aborted (SIGABRT) — consistent with
   the documented API, not a posting path.

## linux_x11 repairs

1. **Transient remap path REMOVED; honest rejection instead (fixed).** The
   temporary `XChangeKeyboardMapping` → click → restore path (and
   `MappingGuard`, `choose_spare_keycode`, `XQueryKeymap`-based spare
   selection) is deleted. A transient remap followed by an immediate restore
   cannot prove every target client processed the events under the NEW mapping
   (slow clients re-query the restored map → wrong characters), so silent
   fake-universal Unicode was the wrong default. New policy: `text_scalar`
   resolves the keysym and clicks it ONLY if it is already bound at the
   UNSHIFTED level (column 0) of the current keymap, on a stable non-modifier
   keycode. Unmapped text (e.g. `é` on a plain US layout) is rejected with
   `invalid_text` BEFORE any side effect. **This driver does NOT claim X11
   Unicode injection beyond the current layout; that is an explicit,
   documented limitation.**
2. **Process-global `XSetErrorHandler` REMOVED (fixed).** The AtomicBool-flag
   handler swap was process-global state, not locked against other Xlib users
   in the process (e.g. the screenshots dependency), and could swallow
   unrelated displays' errors or dangle on chains. All handler installation is
   gone; Xlib's default error/IO handling applies, exactly as for any plain
   Xlib client (documented limitation: an async protocol error or dead display
   terminates the process, per Xlib default).
3. **`XSync` return contract (fixed).** `XSync`'s Status return is no longer
   interpreted as an error indicator (its per-call error semantics were
   guessed before); it is used purely to flush/drain the output buffer.
   `XDisplayKeycodes` no longer gates on its return (it always succeeds on a
   live display and returns Status 1); the out-param range is still validated.
4. **Unshifted-level mapping only (fixed).** `find_mapped_keycode` matched a
   keysym in ANY column, so a keysym bound only at a SHIFTED level (e.g. `A`
   at column 1 of the `a` key) resolved to a keycode that, clicked unshifted,
   produces a different character. Replaced by `find_unshifted_keycode`,
   matching column 0 only (base level of the first keyboard group), still
   excluding modifier-bound keycodes. Covered by new test
   `unshifted_lookup_matches_column_zero_only`.
5. **Stable press/release (kept).** All keycode resolution goes through one
   keymap snapshot per Driver call; press and release of the same key resolve
   to the same keycode within a session; the physical modifier state is never
   rewritten or released by this driver.
6. **Existing behavior kept:** Wayland rejection, XTEST requirement, checked
   `XTestFake*` returns, wheel buttons 4/5 (vertical) and 6/7 (horizontal),
   one Return/Tab click for `\n`/`\r`/`\t`, release retry on failed click
   cleanup, `XSync` after each mutation.

## Blocking-call inventory (Linux, per contract)

Blocking round trips without timeout: `XOpenDisplay` (connection setup) and
`XSync` (one per Driver mutation — move/button/key/scroll-batch/text scalar).
Everything else (`XTestFake*`, mapping/modifier queries are one round trip
each inside resolution) is noted here for the live-acceptance scheduler.

## Known limitations (honest)

- Linux: Unicode text beyond the current keymap's unshifted bindings is
  rejected (`invalid_text`), not injected. Supplementary-plane characters work
  only where a keysym exists AND the keymap binds it (rare). No Ctrl+Shift+U
  trick is attempted (per contract).
- Linux: Xlib default error handler applies (process terminates on X error /
  IO error); no process-global handler was installed by design.
- macOS: `Key::Character` remains ASCII letters/digits only (contract);
  non-ASCII text goes through `text_scalar` UTF-16 injection.
- No live-input/GUI validation was performed (per task); platform behavior
  beyond pure tests is unverified until the coordinator's live pass.

## Shared-contract compliance

No changes to shared interface (`lib.rs`, `types.rs`, `session.rs`, crate
`Cargo.toml`) — all were outside my write scope and untouched.
