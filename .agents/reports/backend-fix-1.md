# Backend fix report #1 — reviewed defect repair

Date: 2026-09-24. Scope: `src/backend/**` only. No commits, no pushes. **No
live screen capture and no desktop input injection were performed during this
fix** — all new tests run against a recording mock dispatcher. The live
capture diagnostic was NOT run (explicitly deferred per instructions; the
environment evidence shows no active displays: `CGGetActiveDisplayList` = 0).

## Defects fixed

### P1 — Scroll direction inverted (was negating both axes)

- **Root cause**: the previous implementation confused enigo's *internal*
  native wheel-delta sign handling (Windows `win_impl.rs` negates vertical
  into `MOUSEEVENTF_WHEEL`; macOS negates into CGEvent wheel deltas) with the
  enigo *API* convention. Installed enigo 0.3.0 `src/lib.rs` documents
  `Mouse::scroll` explicitly: "With `Axis::Vertical`, a positive length will
  result in scrolling down and negative ones up. With `Axis::Horizontal`, a
  positive length will result in scrolling to the right and negative ones to
  the left" — identical to the contract's positive = right/down.
- **Fix**: scroll deltas now pass through unchanged (`src/backend/
  dispatch_core.rs`, `inject_event`); translation comments explain why.
- **Regression tests** (extracted translation logic, real assertions — not
  comment tests): `dispatch::tests::scroll_passes_deltas_through_unchanged`
  and `scroll_skips_zero_axes_and_keeps_signs` drive `InputEvent::Scroll`
  through the state machine into a mock dispatcher and assert the exact
  `(delta, axis)` pairs dispatched, e.g. `Scroll{x:3,y:-5}` →
  `scroll(3, Horizontal)` + `scroll(-5, Vertical)`.

### P1 — Held items forgotten on failed release

- **Root cause**: `inject(Release)` removed the `HeldItem` BEFORE the native
  release call, and `release_all` drained the whole set before attempting, so
  a failed release vanished from tracking and later cleanup falsely reported
  success.
- **Fix** (`dispatch_core.rs`):
  - `inject(Release)` now attempts the release FIRST and removes the item
    from tracking ONLY on success; a failed release keeps it tracked so a
    later release / `release_all` genuinely retries it.
  - `release_all` iterates a sorted snapshot, attempts EVERY item, removes
    only successfully released items, and keeps failures tracked (reported in
    the aggregated `input_failed` error with each item named). A later retry
    actually re-dispatches the failed release.
  - Press semantics unchanged: record held BEFORE the call (partially
    delivered presses still get a release attempt).
- **Mock seam**: new `NativeInput` trait (`src/backend/dispatch.rs`) with a
  production `EnigoInput` pass-through and a `MockInput` in tests that
  records every low-level call and can inject press/release failures per
  item. `BackendCore<I: NativeInput>` (new `src/backend/dispatch_core.rs`)
  holds ALL held-state/scroll/validation logic; `DesktopBackend` is a thin
  shell wiring `EnigoInput` + capture/geometry.
- **Regression tests**: `failed_release_keeps_item_tracked_and_release_is_
  retried` (failed release stays tracked; retry dispatches a 2nd release and
  then clears), `release_all_preserves_failures_and_later_retry_actually_
  attempts_them` (all 3 items attempted despite 1 failure; failure remains
  tracked; later `release_all` dispatches exactly that one release again),
  `failed_press_still_tracks_item_for_later_release`, `release_all_with_
  empty_held_set_is_noop_success`, `invalid_names_fail_before_any_dispatch`
  (zero low-level calls on validation errors), `ascii_key_uses_real_key_
  event_not_text`, `drop_attempts_release_of_remaining_held_items` (Drop
  attempts verified via shared mock call log).

### P1 — macOS capture via `screencapture -D <id> -t png -` removed

- **Root cause**: `-D` takes a display ORDINAL (1=main, 2=secondary), not a
  `CGDirectDisplayID`, and stdout `-` output is undocumented — the previous
  code was unverified and could produce wrong-display captures or a literal
  `-` file.
- **Fix** (`src/backend/macos.rs`): capture is now fully in-memory via the
  existing `screenshots` crate, which on macOS uses CoreGraphics
  `CGDisplay::screenshot` (CGWindowListCreateImage). No subprocess, no temp
  files. `CGPreflightScreenCaptureAccess` preflight retained (CG returns
  null/wallpaper-only when denied).
- **Dimensions**: the actual image produced by CoreGraphics (and the PNG
  re-encoded from it) is authoritative. `CGDisplayPixelsWide/High` is used
  only as a sanity cross-check when it reports MORE pixels than the capture —
  because in scaled logical display modes it can report the logical-mode
  size, which is SMALLER than the rendered backing store; a valid capture is
  never rejected for that (per review instruction). The old hard
  equality check is gone.
- The hardware-dependent unit test `backing_store_lookup_is_sane_for_main_
  display` (hardcoded display id 1, called CoreGraphics in ordinary `cargo
  test`) was **removed**; `reported_display_pixels` remains a pure query
  helper called only from `capture_screen`.

### P1 — Live diagnostic acceptance integrity

- **Fix** (`src/backend/live_test.rs`): `live_backend_capture_only` no longer
  returns `Ok` on `no_display`/any prerequisite failure. It now PANICS
  (fails) by default, printing the structured backend error. Only when the
  operator explicitly sets `RPA_LIVE_TEST_ALLOW_SKIP=1` does it print
  `SKIPPED … NOTE: this run is NOT capture/acceptance proof` and pass.
- Verified behavior in this environment: running it fails with
  `live backend prerequisite unavailable: DesktopBackend::new() ->
  [no_display] no primary display found`; with the env var it prints the
  explicit SKIP line. This matches the coordinator's environment evidence
  (0 active displays; online = [2,1], main = 2; TCC preflights true) — the
  session simply has no ACTIVE desktop, which is NOT a permission problem and
  NOT proof capture works. No wake/unlock attempts were made; no fabricated
  or wallpaper images are substituted anywhere.

### Windows review items (verified, code hardened)

- `GetDpiAwarenessContextForProcess(NULL)`: correct per the Win32 API
  contract — NULL means the CURRENT process (same as `GetCurrentProcess()`);
  documented in the FFI block and wrapped in `current_dpi_awareness_context()`.
- Successful awareness set is now ALSO verified: previously a successful
  `SetProcessDpiAwarenessContext` return was trusted without re-reading;
  `prepare_thread` now re-reads and compares the context after a successful
  set and errors (`internal`) on mismatch. Failure path unchanged (immediate
  error).
- ASCII chord keys (`Key::Unicode(c)`): verified against installed enigo
  0.3.0 sources — Windows `queue_key` first maps the char to a VIRTUAL KEY
  via `VkKeyScanExW` with the current layout and sends `keybd_event` with
  scan code (a REAL key event; `KEYEVENTF_UNICODE` text fallback occurs ONLY
  if no virtual-key mapping exists, which cannot happen for single ASCII
  alphanumerics on a standard layout; macOS maps to real keycodes likewise).
  Docs in `dispatch.rs` now state this accurately instead of claiming
  `KEYEVENTF_UNICODE` unconditionally. Note: enigo's fallback is inside its
  `key()` path and cannot be pre-queried; we restrict input to single ASCII
  alphanumerics precisely so the mapping always exists. No text fallback is
  ever triggered by OUR dispatch for shortcuts; modifiers are pressed only
  after the key name validated (`invalid_key` returns before any dispatch —
  proven by `invalid_names_fail_before_any_dispatch` asserting an empty
  mock call log).

## Files changed

| File | Change |
|---|---|
| `src/backend/dispatch.rs` | NEW: `NativeInput` seam, `EnigoInput` pass-through, full mock-based regression test suite (13 tests) |
| `src/backend/dispatch_core.rs` | NEW: `BackendCore<I>` state machine — fixed held-state/release/scroll logic |
| `src/backend/desktop.rs` | Rewritten as thin shell over `BackendCore<EnigoInput>` + capture/geometry |
| `src/backend/macos.rs` | Subprocess capture replaced by in-memory `screenshots`/CoreGraphics capture; pixel cross-check softened to >= only; hardware unit test removed |
| `src/backend/live_test.rs` | Fails on unavailable prerequisites; explicit `RPA_LIVE_TEST_ALLOW_SKIP=1` structured-skip mode |
| `src/backend/windows.rs` | Post-set DPI awareness verification added; NULL-process semantics documented |
| `src/backend/mod.rs` | Module wiring (`dispatch`, `dispatch_core`), `HeldItem::sort_key`, `Direction: Hash` (needed by mock) |

## Test evidence (this machine, aarch64-apple-darwin)

- **Standalone backend test build** (cargo cannot build the workspace lib
  right now — see Blockers): `rustc --edition 2021 --test` of a crate
  containing ONLY `src/backend/**` + the same dependency rlibs cargo built —
  **24 passed, 0 failed, 1 ignored**. Includes all 13 new dispatch regression
  tests, keys/capture/mod tests. Ran the test binary twice; results stable.
- Live diagnostic (run once, capture-only, to verify fail-not-pass):
  default → FAILED with `[no_display]` (correct: no active desktop);
  `RPA_LIVE_TEST_ALLOW_SKIP=1` → prints `SKIPPED … NOT capture/acceptance
  proof`. It was NOT used as acceptance evidence.
- `rustfmt --check src/backend/*.rs` — clean.
- Windows-target check: cargo metadata/rlib artifacts for
  `x86_64-pc-windows-msvc` exist from the earlier cross build, but standalone
  `rustc` cannot resolve enigo's transitive `windows` crate versions without
  cargo's unit graph, so a Windows compile could not be reproduced outside
  cargo this round. `src/backend/windows.rs` changes are minimal,
  platform-gated FFI only (same signatures as the previously verified cross
  build: `SetProcessDpiAwarenessContext`, `GetDpiAwarenessContextForProcess`,
  `AreDpiAwarenessContextsEqual` — no new APIs).

## Blockers (NOT backend-caused)

`cargo build/test --lib` currently fails in `src/runtime/**` (core worker's
module, mid-rewrite): `E0433 image::ImageReader/Limits`, `E0599
take_session`, multiple `E0499/E0502` borrow errors, `E0616`. Zero errors
point into `src/backend/**`. Backend was therefore verified via the
standalone build described above (same source files, same dependency
artifacts). Once core's runtime compiles, `cargo test --lib` will run the
identical backend tests in-tree (they are `#[cfg(test)]` inside
`src/backend`).

## Honest limitations (unchanged unless noted)

1. macOS capture correctness in an active GUI session is UNVERIFIED on this
   machine (no active displays here). The live diagnostic must be run by the
   coordinator in an interactive console session; only a PASS there counts
   as capture proof. `CGPreflightScreenCaptureAccess == true` here is not
   evidence of capture capability.
2. The `screenshots` crate's macOS path captures via CGWindowList — when
   permission is denied despite a true preflight (TCC race/revocation), CG
   may return None → surfaces as `capture_failed` (structured), never a
   fabricated image.
3. Windows multi-monitor negative-origin coordinates remain an enigo 0.3
   limitation (primary-display target only, per contract).
4. Windows `inject` honesty unchanged: `new()` success does not prove input
   delivery (UIPI/locked desktop/Session 0); failures surface as
   `input_failed`.
5. `release_all`/Drop cannot guarantee a physically-released key if the OS
   input call itself fails — they guarantee: every item attempted, failures
   reported and retained for retry, never a false clean report.
