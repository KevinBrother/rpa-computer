# native-input mac final fixes (+ bounded Linux fixes) — 2026-09-29

Task: `.agents/tasks/native-input-mac-final-fixes-20260929.md`. Scope owned:
`crates/native-input/src/macos/**`, `linux_x11.rs`, `linux_x11/**`. No shared
files, no GUI, no SSH, no commits, no subagents.

## Verification (CARGO_TARGET_DIR=.agents/runs/cargo-native-mac-final)

- `cargo test -p rpa-native-input --target aarch64-apple-darwin`:
  **35 passed, 0 failed** (pure only; no input injection; parallel threads —
  no `--test-threads=1` needed).
- `cargo clippy -p rpa-native-input --all-targets` (aarch64-apple-darwin and
  x86_64-unknown-linux-gnu): 0 warnings.
- `cargo check -p rpa-native-input --all-targets --target x86_64-unknown-linux-gnu`:
  clean — this is the check the Linux container test previously failed
  (item 9); the removed `XK_NO_SYMBOL` reference at keysym.rs:244
  (`sample_keymap`) is fixed to `0 as KeySym`.
- `cargo check -p rpa-native-input --target x86_64-pc-windows-msvc`: clean.
- `cargo check --workspace` (host): clean.
- `rustfmt` applied ONLY to the five owned files.
- Constant values verified against installed SDK headers (not self-referential).

## macOS fixes

1. **Scroll unit (fixed).** `CG_SCROLL_EVENT_UNIT_LINE` was `0`, which is
   `kCGScrollEventUnitPixel` (CGEventTypes.h:47-48: Pixel=0, Line=1). Now `1`,
   so scroll ticks are LINE ticks as the contract ("wheel ticks") intends.
   Regression test `sdk_constants_match_installed_headers` asserts the header
   values directly.
2. **Non-coalesced flag (fixed).** `FLAG_NON_COALESCED` was `0x0100_0000`;
   `NX_NONCOALSESCEDMASK` is `0x00000100` (IOLLEvent.h:268). Corrected to
   `0x0000_0100`. The `NX_DEVICEL*KEYMASK` device masks (0x01/0x02/0x08/0x20,
   IOLLEvent.h:253-258) and the modifier masks were verified and kept. Same
   regression test covers them.
3. **Cache commit semantics (fixed).** The release path previously removed the
   cached keycode BEFORE `post_key_event`; now the cache is a pure
   `CharacterKeycodeCache` and mutation is committed only after the post
   succeeds:
   - failed release → entry retained → retry resolves to the same physical
     code even after a layout switch;
   - failed press → nothing inserted;
   - repeated press of a held key → original keycode retained
     (`or_insert`, never overwritten, no re-resolve).
   Failure seam: `CharResolver = Box<dyn Fn(char) -> Result<u16>>` lets tests
   inject deterministic fakes (counting resolver, layout-switch resolver,
   erroring resolver) — no live TIS in cache tests. Six new pure tests.
   **ASCII case normalization implemented (bounded, ~6 lines):** cache keys
   normalize ASCII letters to lowercase, so `'a'`/`'A'` alias one physical
   entry and an uppercase-alias release still consumes it. Documented in code.
4. **Comment corrections (done).** `CGEventCreateScrollWheelEvent2` described
   as "six parameters total (source, units, wheelCount, wheel1, wheel2,
   wheel3)"; `UCKeyTranslate` described as exported by CoreServices' CarbonCore
   framework (canonical Carbon declaration HIToolbox/UnicodeInput.h) —
   matching what the SDK actually ships (CarbonCore.tbd export; the
   HIToolbox UnicodeInput.h header is not shipped in this SDK). ABI widths
   unchanged: `UniCharCount` = `c_ulong`, `UInt32` = `u32` (already verified).
5. **C0/DEL text validation (fixed).** Raw `text_scalar` now rejects NUL,
   other C0 (U+0000–U+001F) and DEL (U+007F) except `\n`/`\r`/`\t` in
   `keymap::validate_text_char` (defense-in-depth mirroring the shared caller
   validation; no shared file touched). Test updated to cover U+0001, U+001B,
   U+007F rejections.
6. **Production TIS lock (fixed).** TIS/TSM abort when called from two threads
   concurrently (seen in the coordinator's .ips). A process-local
   `static TIS_LOCK: Mutex<()>` now guards the ENTIRE
   TIS-copy + property-fetch + CFData + UCKeyTranslate lifetime inside
   `char_to_keycode` (library code, not test-only). The test-only
   `LAYOUT_LOCK` was removed; the layout test runs safely under parallel
   threads. **Documented limitation:** the mutex serializes only users of
   THIS library — it cannot coordinate other TIS users in the same process,
   and GUI embedding additionally requires all TIS calls on the application
   main thread (stated in the code docs and here).
7. Dead code removed: `keymap::key_code` (was only reachable via tests after
   the cache refactor) deleted; tests call `char_to_keycode` directly.

## macOS review item 6 — click state and delivery (report only, API unchanged)

- **Multi-click:** `post_mouse_event` always sets `kCGMouseEventClickState`
  (click count) to 1. Consequential macOS double-click recognition requires
  `CGEventSetIntegerValueField(event, kCGMouseEventClickState, N)` with the
  OS's current double-click interval timing — two native Press/Release calls
  with clickState=1 will NOT be recognized as an OS double-click. Supporting
  real multi-click would need tracking timing windows between our own posts
  (and matching the user's System Settings double-click interval), which is
  beyond this bounded driver; **honest limitation: this driver injects only
  single clicks.** The GUI fixture's single-click test therefore does not
  prove multi-click support, and none is claimed.
- **Delivery:** `CGEventPost` returns void; posting proves the event entered
  the HID event tap pipeline, NOT that any recipient application processed
  it. The driver reports creation/post-call failures only; downstream
  acceptance must be judged by live GUI verification, not by the driver's
  success return. This is a structural property of CGEventPost, not fixable
  in a bounded driver.
- **Scroll unit note:** with the Line-unit fix, `scroll(±n)` semantics now
  match "wheel ticks" (one line per tick) instead of the previous
  accidental pixel-unit deltas.

## Linux fixes (allowed scope: linux_x11.rs / linux_x11/**)

1. **Stable press/release keycode cache (fixed).** `Platform` now keeps a
   pure `KeycodeCache` (all keys): press resolves (cache-first), commits the
   keycode only after the fake press succeeded (`or_insert` — repeated press
   retains the original); release reuses the cached keycode and commits
   removal only after the fake release succeeded, so a failed release keeps
   the physical code for retry. No transient remapping involved. Five pure
   tests added (they run in the coordinator's Linux container pass).
2. **Key::Meta → XK_SUPER_L (fixed, documented).** `Key::Meta` now maps to
   `XK_SUPER_L` (0xFFEB, the Linux Win/Super key) as the cross-platform Meta
   intent; the legacy `XK_META_L` (0xFFE7, rarely bound on real layouts) was
   removed. Mapping policy documented in keysym.rs and covered by a test.
3. **`KeySym` = `c_ulong` (fixed).** Xlib's `KeySym` is `unsigned long`; the
   alias now mirrors that instead of a hard-coded `u64` (FFI `XGetKeyboardMapping`
   and all keysym constants follow). A test asserts the size equality.
4. **Scroll release-half failure (fixed).** Wheel ticks now go through
   `click_button`: press → release with a release RETRY, and if the retry
   also fails the error message preserves BOTH failures (same semantics as
   `click_keycode`). Failures are never silently dropped.
5. **Container test compile failure (fixed).** keysym.rs `sample_keymap`
   referenced the removed `XK_NO_SYMBOL`; replaced with a literal. Verified
   with `cargo check --all-targets --target x86_64-unknown-linux-gnu` (0
   errors, 0 warnings) — not just the lib target. No surrogate/stub code.
6. Unmapped-text rejection policy (reject before effects, `invalid_text`) is
   preserved unchanged.

## Limitations carried forward (unchanged from prior pass)

- Linux: Unicode text beyond the current keymap's unshifted bindings is
  rejected; no temporary remapping; no Ctrl+Shift+U trick.
- macOS: single-click only (see item 6); CGEventPost delivery not provable
  by return value; TIS main-thread requirement for GUI embedding.
- No live input/GUI validation performed in this pass (per task).
