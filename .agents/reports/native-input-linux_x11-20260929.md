# native-input linux_x11 worker report (2026-09-29)

Worker: Claude CLI (GLM sonnet routing). Task: `.agents/tasks/native-input-linux_x11-20260929.md` (contract: `.agents/tasks/native-input-contract-20260929.md`). Only owned files were written.

## Files delivered (owned scope only)

- `crates/native-input/src/linux_x11.rs` — direct libX11/libXtst FFI driver, `pub(crate) struct Platform` with `new()` + `Driver` impl.
- `crates/native-input/src/linux_x11/keysym.rs` — pure, FFI-free keysym/mapping logic + unit tests (testable on any host).

No edits to Cargo.toml, lib.rs, types.rs, session.rs, other platforms, or root integration. Only my two files were rustfmt'ed.

## Implementation summary

- Session validation in `Platform::new()`: rejects Wayland explicitly (`WAYLAND_DISPLAY` set or `XDG_SESSION_TYPE=wayland`) → `unsupported_session` even when DISPLAY exists; missing `DISPLAY` / failed `XOpenDisplay` → `no_display`; missing XTEST extension → `unsupported_session`; implausible keycode range → `input_failed`. Display is owned and closed in `Drop`.
- Named keys: keysymdef.h constants (Control/Shift/Alt/Meta L-variants, Return, Tab, Space, Backspace, Delete, Escape, arrows, Home/End/PageUp/PageDown, F1..F12 contiguous 0xFFBE..0xFFC9); keycode via `XKeysymToKeycode`; same keycode for press and release (stable mapping). Unmapped keysym → `input_failed`.
- `Key::Character`: only ASCII letters/digits accepted (reject before any effect) → `invalid_key` otherwise. Letters normalized to the lowercase keysym so `Character('A')` and `Character('a')` land on one physical keycode — correct for chord semantics; uppercase shift state is the caller's concern (consistent with the shared session wrapper owning modifiers).
- `text_scalar`: `\n`/`\r` each produce exactly ONE Return click, `\t` ONE Tab click, NUL → `invalid_text`. Unicode path: codepoint→keysym per keysymdef.h (Latin-1 direct for 0x20–0x7E and 0xA0–0xFF; 0x01000000-plane for 0x100–0x1FFFFF; control chars/Latin-1 gap/no-encoding → `invalid_text`).
- Unicode injection (honest implementation):
  1. Snapshot full keymap (`XGetKeyboardMapping`) + modifier set (`XGetModifierMapping`).
  2. If the keysym is already mapped on a non-modifier keycode, use it directly — no mapping change.
  3. Otherwise choose a spare keycode: every column NoSymbol, not modifier-bound, highest-first. If none → explicit `input_failed` (no Ctrl+Shift+U trick).
  4. Temporary remap with the global keysyms-per-keycode width preserved (target keysym in column 0, NoSymbol elsewhere); the full original row is saved and restored on ALL paths: explicit `restore_now()` on the success/failure paths plus an RAII `Drop` backstop for early returns. Error handler is swapped in around `XSync` to catch `BadValue`/`BadAlloc` and turned into structured `input_failed`; the previous handler is always restored.
- Mouse: `XTestFakeMotionEvent` on the default screen (root pixels); buttons 1/2/3 for Left/Middle/Right per `Direction`; wheels 5/4 (down/up) and 7/6 (right/left), positive length = DOWN/RIGHT. Every `XTestFake*` return status is checked; `XSync` result plus swapped-handler error flag checked per operation. No fake success anywhere.
- Partial-event cleanup: a failed release in a scalar click is retried once before the failure is reported; only this driver's own transient events are cleaned; no global releases, no screenshots, no sleeps, no clipboard.

## Verification actually performed

- Pure tests recorded and executed (TDD): the 11 unit tests in `keysym.rs` were run BEFORE finalizing the FFI layer via a scratch harness that includes the production file verbatim (`#[path]`), so the exact production code is what ran:
  - Harness: `.agents/runs/cargo-native-linux_x11/scratch-keysym` (scratch only, not product code; kept for reviewer re-runs).
  - `CARGO_TARGET_DIR=.agents/runs/cargo-native-linux_x11/target cargo test` → **11 passed, 0 failed** (final run; an earlier run surfaced 3 wrong test literals/expectations, which were fixed — the pure functions were correct).
- `cargo check --target x86_64-unknown-linux-gnu` on the shared skeleton → **PASS** (type-check of the real crate including my module; toolchain std for the linux target was added locally via `rustup target add`).
- `cargo clippy --target x86_64-unknown-linux-gnu` → clean after fixing 3 lints (`is_multiple_of`, slice `contains`).

## NOT verified (honest)

- **No runtime/link verification.** This host is macOS; there is no Linux linker or X11 server here. `cargo check` proves types/FFI signatures compile, NOT that the program links against libX11/libXtst or behaves correctly. The instructions' "don't claim cross-platform build" applies: I do not claim a Linux build, only a Linux-target type-check.
- No live X11 input was performed (prohibited for this task).
- The crate's Linux-target `cargo test` was not run (requires a Linux host).

## Limitations & blockers

1. **Asynchronous target lookup (contract-mandated honesty):** after a temporary remap, the injected event carries only a keycode; the receiving client resolves it to a keysym using its OWN cached mapping. We `XSync` (change is server-side committed, `MappingNotify` queued) and restore immediately — no sleeps are permitted inside scalar injection. Clients that never call `XRefreshKeyboardMapping` on `MappingNotify` may mis-resolve the spare keycode. This is the known xdotool-style tradeoff. Characters already present in the map (ASCII, Latin-1, and anything the layout defines) take path (2) above and are NOT subject to this limit. Claim: reliable for mapped keysyms; best-effort for temporarily remapped ones.
2. **Blocking calls:** `XOpenDisplay` (connection setup), `XSync`, `XGetKeyboardMapping`, `XGetModifierMapping` are unbounded round trips — a wedged X server blocks the worker thread. No timeout mechanism exists in raw Xlib without additional machinery.
3. **Process-global Xlib handlers:** the `XSetErrorHandler` swap is process-global while held (bounded to individual `XSync` windows). If another in-process component issues X calls concurrently in that window (e.g., the host crate's `enigo`/`screenshots` on Linux), error flags can be mis-attributed. Within this driver all X access is serialized by `&mut self`.
4. **Xlib IO error handler:** the default handler terminates the process if the display connection dies. I deliberately did not install a process-global IO handler (would affect unrelated components); documented instead.
5. **Spare-keycode safety:** excluded modifier-bound rows and required an all-NoSymbol row, but foreign held keys on an unused keycode cannot be detected from the client side. No known case on standard layouts (tail keycodes are unused).
6. Toolchain note: `rustup target add x86_64-unknown-linux-gnu` was run on this machine to enable the target type-check (local toolchain component, not global Claude config).

## Contract compliance / API feedback

- Shared API matched core's `types.rs` exactly; no contract changes needed. Proposals: none.
- Error codes used: `no_display`, `unsupported_session`, `input_failed`, `invalid_key`, `invalid_text` (no situation here justifies `permission_denied`; noted for the coordinator in case the core expects it for XAUTHORITY failures — Xlib surfaces those as a failed `XOpenDisplay`, currently mapped to `no_display`).
